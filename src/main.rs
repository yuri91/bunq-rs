use anyhow::Result;
use isahc::prelude::*;
use serde::Serialize;
use openssl::pkey::PKey;

const BASE: &str = "https://public-api.sandbox.bunq.com/v1/";

#[derive(Serialize)]
struct Installation<'a> {
    client_public_key: &'a str,
}

fn main() -> Result<()> {
    dotenv::dotenv()?;

    let api_key = std::env::var("API_KEY")?;

    let pem_private = include_bytes!("../private.pem");
    let pem_public = include_bytes!("../public.pem");
    let keypair = PKey::private_key_from_pem(pem_private)?;

    let client_public_key = std::str::from_utf8(pem_public)?;
    let body = Installation { client_public_key: &client_public_key};
    let mut response = isahc::post(format!("{}installation",BASE), serde_json::to_string(&body)?)?;
    println!("{}", response.text()?);
    Ok(())
}
