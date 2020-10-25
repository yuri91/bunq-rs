use anyhow::Result;
use isahc::prelude::*;
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::rsa::Rsa;
use openssl::sign::Signer;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

const BASE: &str = "https://api.bunq.com";

#[derive(Serialize)]
struct Installation<'a> {
    client_public_key: &'a str,
}

#[derive(Deserialize)]
struct Token {
    token: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InstallationResponse {
    token: Token,
}

#[derive(Serialize)]
struct DeviceServer<'a> {
    description: &'a str,
    secret: &'a str,
    permitted_ips: &'a [&'a str],
}

#[derive(Serialize)]
struct SessionServer<'a> {
    secret: &'a str,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SessionServerResponse {
    token: Token,
    user_person: UserPerson,
}

#[derive(Deserialize)]
struct UserPerson {
    id: i64,
}

fn sign<K: openssl::pkey::HasPrivate>(body: &str, key: &PKey<K>) -> Result<String> {
    let mut signer = Signer::new(MessageDigest::sha256(), key)?;

    let sig = signer.sign_oneshot_to_vec(body.as_bytes())?;

    Ok(base64::encode(&sig))
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawResponse {
    response: Vec<serde_json::Value>,
    pagination: Option<Pagination>,
}

struct Response<T> {
    response: T,
    pagination: Option<Pagination>,
}

impl RawResponse {
    fn decode_retarded<T: DeserializeOwned>(self) -> Result<Response<T>> {
        let mut map = serde_json::Map::new();
        for e in self.response {
            if let serde_json::Value::Object(e) = e {
                let (k, v) = e
                    .into_iter()
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("malformed response"))?;
                map.insert(k, v);
            }
        }
        Ok(Response {
            response: serde_json::from_value(map.into())?,
            pagination: self.pagination,
        })
    }
}

fn deserialize_retarded_response<T: DeserializeOwned>(r: &str) -> Result<Response<T>> {
    let r: RawResponse = serde_json::from_str(r)?;
    r.decode_retarded()
}

fn deserialize_normal_response<T: DeserializeOwned>(r: &str) -> Result<Response<T>> {
    let r: RawResponse = serde_json::from_str(r)?;
    Ok(Response {
        response: serde_json::from_value(r.response.into())?,
        pagination: r.pagination,
    })
}

#[derive(Serialize, Deserialize, Default)]
struct AppState {
    token: String,
    pem_private: String,
}
#[derive(Serialize, Deserialize, Default)]
pub struct BunqConfig {
    api_key: String,
    state: Option<AppState>,
}
pub struct BunqConfigReady {
    token: String,
    keypair: PKey<openssl::pkey::Private>,
    user_id: i64,
}
impl BunqConfig {
    pub fn load() -> Result<BunqConfig> {
        Ok(confy::load("bunq-rs")?)
    }
    pub fn save(&self) -> Result<()> {
        confy::store("bunq-rs", self)?;
        Ok(())
    }
    pub fn install(mut self) -> Result<BunqConfigReady> {
        let api_key = &self.api_key;

        let keypair = if let Some(state) = &self.state {
            PKey::private_key_from_pem(state.pem_private.as_bytes())?
        } else {
            let rsa = Rsa::generate(2048)?;
            let pem_private = rsa.private_key_to_pem()?;
            let pem_private = String::from_utf8(pem_private)?;

            let keypair = PKey::from_rsa(rsa)?;

            let pem_public = String::from_utf8(keypair.public_key_to_pem()?)?;

            let body = Installation {
                client_public_key: &pem_public,
            };
            let response = isahc::post(
                format!("{}/v1/installation", BASE),
                serde_json::to_string(&body)?,
            )?
            .text()?;
            let response: InstallationResponse = deserialize_retarded_response(&response)?.response;
            let token = response.token.token;

            let body = DeviceServer {
                description: "awesome",
                secret: &api_key,
                permitted_ips: &["31.21.118.143", "*"],
            };
            let body = serde_json::to_string(&body)?;
            let mut response = isahc::http::Request::post(format!("{}/v1/device-server", BASE))
                .header("X-Bunq-Client-Authentication", &token)
                .body(body)?
                .send()?;
            println!("{}", response.text()?);

            self.state = Some(AppState { pem_private, token });
            self.save()?;

            keypair
        };
        let token = self.state.unwrap().token;
        let body = SessionServer { secret: &api_key };
        let body = serde_json::to_string(&body)?;
        let sig = sign(&body, &keypair)?;
        let response = isahc::http::Request::post(format!("{}/v1/session-server", BASE))
            .header("X-Bunq-Client-Authentication", &token)
            .header("X-Bunq-Client-Signature", &sig)
            .body(body)?
            .send()?
            .text()?;
        let r: SessionServerResponse = deserialize_retarded_response(&response)?.response;
        Ok(BunqConfigReady {
            keypair,
            token: r.token.token,
            user_id: r.user_person.id,
        })
    }
}

impl BunqConfigReady {
    pub fn monetary_accounts(&self) -> Result<Vec<MonetaryAccountBank>> {
        let response = isahc::http::Request::get(format!(
            "{}/v1/user/{}/monetary-account",
            BASE, self.user_id
        ))
        .header("X-Bunq-Client-Authentication", &self.token)
        .body(())?
        .send()?
        .text()?;
        Ok(
            deserialize_normal_response::<Vec<MonetaryAccount>>(&response)?
                .response
                .into_iter()
                .map(|m| m.monetary_account_bank)
                .collect(),
        )
    }
    pub fn payments(&self, acc: &MonetaryAccountBank) -> Result<Vec<Payment>> {
        let next_page = |url: &str| -> Result<(_, _)> {
            let response = isahc::http::Request::get(url)
                .header("X-Bunq-Client-Authentication", &self.token)
                .body(())?
                .send()?
                .text()?;
            let Response {
                response,
                pagination,
            } = deserialize_normal_response::<Vec<PaymentPayment>>(&response)?;
            Ok((
                response.into_iter().map(|p| p.payment).collect(),
                pagination,
            ))
        };
        let mut url = format!(
            "/v1/user/{}/monetary-account/{}/payment",
            self.user_id, acc.id
        );
        let mut all = Vec::new();
        loop {
            let (mut payments, pag) = next_page(&format!("{}{}", BASE, url))?;
            all.append(&mut payments);
            if let Some(Pagination {
                older_url: Some(older_url),
                ..
            }) = pag
            {
                url = older_url;
            } else {
                break;
            }
        }
        Ok(all)
    }
}

#[derive(Deserialize, Debug)]
pub struct LabelMonetaryAccount {
    pub iban: Option<String>,
    pub display_name: String,
    pub merchant_category_code: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct Amount {
    pub value: String,
    pub currency: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct PaymentPayment {
    payment: Payment,
}

#[derive(Deserialize, Debug)]
struct Pagination {
    future_url: Option<String>,
    newer_url: Option<String>,
    older_url: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct Payment {
    pub alias: LabelMonetaryAccount,
    pub counterparty_alias: LabelMonetaryAccount,
    pub amount: Amount,
    pub balance_after_mutation: Amount,
    pub created: String,
    pub updated: String,
    pub description: String,
    pub id: i64,
    pub monetary_account_id: i64,
    #[serde(rename = "type")]
    pub type_: String,
    pub sub_type: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct MonetaryAccount {
    monetary_account_bank: MonetaryAccountBank,
}
#[derive(Deserialize, Debug)]
pub struct MonetaryAccountBank {
    pub id: i64,
    pub description: String,
}
