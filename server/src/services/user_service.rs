use crate::{
    error::{AppError, Result},
    repositories::user_repo::UserRepo,
};
use argon2::{
    Argon2, PasswordHasher,
    password_hash::{PasswordHash, PasswordVerifier, SaltString},
};
use chrono::Duration;
use jsonwebtoken::{EncodingKey, Header, encode};
use serde::Serialize;




#[derive(Serialize)]
struct Claims {
    sub: String,
    uid: i64,
    exp: usize,
}
#[derive(Debug, Clone)]
pub struct UserService;
impl UserService {
    pub async fn register(pool: &sqlx::SqlitePool, name: &str, password: &str) -> Result<i64> {
        let salt = SaltString::generate(rand::thread_rng());
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| AppError::BadRequest(e.to_string()))?
            .to_string();
        UserRepo::create(pool, name, &hash).await
    }
    pub async fn login(
        pool: &sqlx::SqlitePool,
        jwt_secret: &str,
        name: &str,
        password: &str,
    ) -> Result<String> {
        let Some((uid, pwd_hash)) = UserRepo::find_by_name(pool, name).await? else {
            return Err(AppError::Unauthorized);
        };
        let parsed = PasswordHash::new(&pwd_hash).map_err(|_| AppError::Unauthorized)?;
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .map_err(|_| AppError::Unauthorized)?;
        let exp = (chrono::Utc::now() + Duration::hours(24)).timestamp() as usize;
        let claims = Claims {
            sub: name.into(),
            uid,
            exp,
        };
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(jwt_secret.as_bytes()),
        )
        .map_err(|_| AppError::Unauthorized)?;
        Ok(token)
    }
}
