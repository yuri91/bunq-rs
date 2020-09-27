use anyhow::Result;
use isahc::prelude::*;

use openssl::pkey::PKey;

const BASE: &str = "https://public-api.sandbox.bunq.com/v1/";

fn main() -> Result<()> {
    dotenv::dotenv()?;

    let api_key = std::env::var("API_KEY")?;

    let pem = include_bytes!("../private.pem");
    let keypair = PKey::private_key_from_pem(pem)?;

    let mut response = isahc::post(format!("{}installation",BASE), format!("{{'client_public_key': '{}'}}", std::str::from_utf8(pem)?))?;
    println!("{}", response.text()?);
    Ok(())
}
