// services/user_service.rs
use crate::{error::{AppError, Result}, repositories::user_repo::UserRepo};
use argon2::{Argon2, PasswordHasher, PasswordVerifier, password_hash::{PasswordHash, SaltString}};
use chrono::Duration;
use jsonwebtoken::{EncodingKey, Header, encode};
use serde::Serialize;
use uuid::Uuid;

#[derive(Serialize)]
struct Claims {
    sub: String,
    uid: Uuid,   // <- UUID nativo
    exp: usize,
}

#[derive(Debug, Clone)]
pub struct UserService;

impl UserService {
    pub async fn register(pool: &sqlx::SqlitePool, username: &str, password: &str) -> Result<Uuid> {
        let salt = SaltString::generate(rand::thread_rng());
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| AppError::BadRequest(e.to_string()))?
            .to_string();
        UserRepo::create(pool, username, &hash).await
    }

    pub async fn login(
        pool: &sqlx::SqlitePool,
        jwt_secret: &str,
        username: &str,
        password: &str,
    ) -> Result<(String, Uuid)> {
        let Some((uid, pwd_hash)) = UserRepo::find_by_name(pool, username).await? else {
            return Err(AppError::Unauthorized);
        };

        let parsed = PasswordHash::new(&pwd_hash).map_err(|_| AppError::Unauthorized)?;
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .map_err(|_| AppError::Unauthorized)?;

        let exp = (chrono::Utc::now() + Duration::hours(24)).timestamp() as usize;
        let claims = Claims { sub: username.to_owned(), uid, exp };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(jwt_secret.as_bytes()),
        ).map_err(|_| AppError::Unauthorized)?;

        Ok((token, uid))
    }

    pub async fn get_user_id_by_username(pool: &sqlx::SqlitePool, username: &str) -> Result<Uuid> {
        let Some((uid, _)) = UserRepo::find_by_name(pool, username).await? else {
            return Err(AppError::NotFound);
        };
        Ok(uid)
    }

    pub async fn delete_user(pool: &sqlx::SqlitePool, user_id: Uuid) -> Result<()> {
        // Esegue una transazione che:
        // 1) Pulisce user_events e user_sequences
        // 2) Pulisce message_sequences delle conversazioni che saranno eliminate (owner_id = user_id)
        // 3) Elimina l'utente (le FK con CASCADE faranno il resto)
        UserRepo::delete_user_cascade(pool, user_id).await
    }
}
