use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString}
};
use jsonwebtoken as jwt;
use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};
use rand_core::OsRng;

pub fn hash_password(clear: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);

    let phc = Argon2::default()
        .hash_password(clear.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!(e))?;

    Ok(phc.to_string())
}
pub fn verify_password(clear: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .and_then(|ph| Argon2::default().verify_password(clear.as_bytes(), &ph).map(|_| ()))
        .is_ok()
}

#[derive(Serialize, Deserialize)]
pub struct Claims { pub sub: i64, pub exp: i64 }

pub fn issue_jwt(user_id: i64, secret: &str, hours: i64) -> String {
    let exp = (OffsetDateTime::now_utc() + Duration::hours(hours)).unix_timestamp();
    let claims = Claims { sub: user_id, exp };
    jwt::encode(
        &jwt::Header::default(),
        &claims,
        &jwt::EncodingKey::from_secret(secret.as_bytes())
    ).unwrap()
}

pub fn decode_jwt(token: &str, secret: &str) -> jwt::errors::Result<Claims> {
    let data = jwt::decode::<Claims>(
        token,
        &jwt::DecodingKey::from_secret(secret.as_bytes()),
        &jwt::Validation::default()
    )?;
    Ok(data.claims)
}
