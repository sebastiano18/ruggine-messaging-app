// repositories/user_repo.rs
use crate::error::{Result, AppError};
use sqlx::{Row, SqlitePool, Transaction};
use uuid::Uuid;
use sqlx::Sqlite;

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
        let mut tx: Transaction<'_, Sqlite> = pool.begin().await.map_err(AppError::from)?;

        // 1) Pulisci eventi e sequenze utente (non hanno FK)
        sqlx::query("DELETE FROM user_events WHERE user_id = ?")
            .bind(user_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;

        sqlx::query("DELETE FROM user_sequences WHERE user_id = ?")
            .bind(user_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;

        // 2) Pulisci message_sequences delle conversazioni che verranno eliminate
        //    (quelle di cui l'utente è owner). Poiché message_sequences non ha FK, serve pulizia esplicita.
        sqlx::query(
            "DELETE FROM message_sequences
             WHERE conversation_id IN (SELECT id FROM conversations WHERE owner_id = ?)"
        )
        .bind(user_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;

        // 3) Elimina l'utente: CASCADE su conversations/participants/messages/invites farà il resto
        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(user_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;

        tx.commit().await.map_err(AppError::from)?;
        Ok(())
    }
}
