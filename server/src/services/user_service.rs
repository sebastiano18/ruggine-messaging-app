use crate::{
    error::{AppError, Result},
    repositories::{user_repo::UserRepo, conversation_repo::ConversationRepo},
    state::AppState,
};
use argon2::{Argon2, PasswordHasher, PasswordVerifier, password_hash::{PasswordHash, SaltString}};
use chrono::Duration;
use jsonwebtoken::{EncodingKey, Header, encode};
use serde::Serialize;
use uuid::Uuid;
use tracing::{info, error, warn};

#[derive(Serialize)]
struct Claims {
    sub: String,
    uid: Uuid,
    exp: i64,
}
#[derive(Debug, Clone)]
pub struct UserService;

impl UserService {
    pub async fn register(pool: &sqlx::SqlitePool, username: &str, password: &str) -> Result<Uuid> {
        // Validazione input
        if username.trim().is_empty() {
            return Err(AppError::BadRequest("Username non può essere vuoto".to_string()));
        }
        if password.len() < 4 {
            return Err(AppError::BadRequest("Password deve essere almeno 4 caratteri".to_string()));
        }

        // Controlla se username già esistente
        if UserRepo::find_by_name(pool, username).await?.is_some() {
            return Err(AppError::Conflict("Username già registrato".to_string()));
        }

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
        // Validazione input
        if username.trim().is_empty() || password.is_empty() {
            return Err(AppError::BadRequest("Username e password richiesti".to_string()));
        }

        let Some((uid, pwd_hash)) = UserRepo::find_by_name(pool, username).await? else {
            return Err(AppError::Unauthorized);
        };

        let parsed = PasswordHash::new(&pwd_hash).map_err(|_| AppError::Unauthorized)?;
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .map_err(|_| AppError::Unauthorized)?;

        let exp = (chrono::Utc::now() + Duration::hours(24)).timestamp();
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

    pub async fn notify_participants_of_deleted_user(
        state: &AppState,
        deleted_user_id: Uuid,
    ) -> Result<()> {
        use crate::services::conversation_service::ConversationService;

        let conversations = ConversationRepo::by_user(&state.pool, deleted_user_id).await?;

        // Ottieni username prima che l'utente venga eliminato
        let deleted_username = sqlx::query_scalar::<_, String>(
            "SELECT username FROM users WHERE id = ?"
        )
            .bind(deleted_user_id.to_string())
            .fetch_one(&state.pool)
            .await
            .unwrap_or_else(|_| "Unknown".to_string());

        info!(
        "Notifying participants about deletion of user {} ({}) in {} conversations",
        deleted_user_id, deleted_username, conversations.len()
    );

        for (conv_id, kind, _title, owner_id, _created_at, _last_read, _last_activity, _last_msg_seq) in conversations {
            let result = match kind.as_str() {
                "dm" => {
                    // DM: sempre eliminata
                    ConversationService::notify_conversation_deleted(
                        state,
                        conv_id,
                        deleted_user_id,
                        "user_deleted"
                    ).await
                }
                "group" => {
                    if owner_id == deleted_user_id {
                        // Gruppo owner: gruppo eliminato
                        ConversationService::notify_conversation_deleted(
                            state,
                            conv_id,
                            deleted_user_id,
                            "owner_deleted"
                        ).await
                    } else {
                        // Gruppo participant: utente ha eliminato il proprio account
                        ConversationService::notify_user_deleted_account(
                            state,
                            conv_id,
                            deleted_user_id,
                            &deleted_username
                        ).await
                    }
                }
                _ => {
                    warn!("Unknown conversation kind: {}", kind);
                    continue;
                }
            };

            if let Err(e) = result {
                error!("Failed to notify for conversation {}: {}", conv_id, e);
            }
        }

        Ok(())
    }
}