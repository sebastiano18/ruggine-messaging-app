use crate::error::{Result, AppError};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct UserRepo;

impl UserRepo {
    pub async fn find_by_name(pool: &SqlitePool, username: &str) -> Result<Option<(Uuid, String)>> {
        let row = sqlx::query("SELECT id, pass_hash FROM users WHERE username = ?")
            .bind(username)
            .fetch_optional(pool)
            .await?;

        Ok(row.map(|r| {
            let id: String = r.get("id");
            let id = Uuid::parse_str(&id).expect("DB must store valid UUIDs");
            let password: String = r.get("pass_hash");
            (id, password)
        }))
    }

    pub async fn create(pool: &SqlitePool, username: &str, password_hash: &str) -> Result<Uuid> {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users(id, username, pass_hash, created_at)
             VALUES(?, ?, ?, strftime('%s','now'))",
        )
            .bind(id.to_string())
            .bind(username)
            .bind(password_hash)
            .execute(pool)
            .await?;

        Ok(id)
    }

    pub async fn delete_user_cascade(pool: &SqlitePool, user_id: Uuid) -> Result<()> {
        tracing::info!("Starting cascade delete for user {}", user_id);

        let mut tx = pool.begin().await
            .map_err(|e| {
                tracing::error!("Failed to begin transaction: {}", e);
                AppError::from(e)
            })?;
        tracing::debug!("Transaction started successfully");

        // 1) Pulisci eventi e sequenze utente
        tracing::debug!("Deleting user_events for user {}", user_id);
        sqlx::query("DELETE FROM user_events WHERE user_id = ?")
            .bind(user_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!("Failed to delete user_events: {}", e);
                AppError::from(e)
            })?;
        tracing::debug!("Deleted user_events successfully");

        tracing::debug!("Deleting user_sequences for user {}", user_id);
        sqlx::query("DELETE FROM user_sequences WHERE user_id = ?")
            .bind(user_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!("Failed to delete user_sequences: {}", e);
                AppError::from(e)
            })?;
        tracing::debug!("Deleted user_sequences successfully");

        // 2) Pulisci message_sequences SOLO delle conversazioni che verranno eliminate
        //    (DM + gruppi dove è owner)
        tracing::debug!("Deleting message_sequences for conversations that will be deleted");
        sqlx::query(
            "DELETE FROM message_sequences
         WHERE conversation_id IN (
             SELECT c.id FROM conversations c
             JOIN participants p ON c.id = p.conversation_id
             WHERE p.user_id = ? AND c.kind = 'dm'
             
             UNION
             
             SELECT id FROM conversations
             WHERE owner_id = ? AND kind = 'group'
         )"
        )
            .bind(user_id.to_string())
            .bind(user_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!("Failed to delete message_sequences: {}", e);
                AppError::from(e)
            })?;
        tracing::debug!("Deleted message_sequences successfully");

        // 3) Elimina tutte le DM a cui partecipa (sono sempre a 2)
        tracing::debug!("Deleting DM conversations");
        sqlx::query(
            "DELETE FROM conversations
         WHERE kind = 'dm'
         AND id IN (
             SELECT conversation_id FROM participants WHERE user_id = ?
         )"
        )
            .bind(user_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!("Failed to delete DM conversations: {}", e);
                AppError::from(e)
            })?;
        tracing::debug!("Deleted DM conversations successfully");

        // 4) Elimina l'utente (CASCADE elimina: gruppi dove è owner, participants, messages, invites)
        tracing::debug!("Deleting user record");
        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(user_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!("Failed to delete user record: {}", e);
                AppError::from(e)
            })?;
        tracing::debug!("Deleted user record successfully");

        tracing::debug!("Committing transaction");
        tx.commit().await
            .map_err(|e| {
                tracing::error!("Failed to commit transaction: {}", e);
                AppError::from(e)
            })?;

        tracing::info!("Successfully deleted user {}", user_id);
        Ok(())
    }
}
