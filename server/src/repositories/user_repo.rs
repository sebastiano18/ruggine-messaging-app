use crate::error::Result;
use sqlx::{Row, SqlitePool};

#[derive(Debug, Clone)]
pub struct UserRepo;

impl UserRepo {
    /// Ritorna (id, password_hash) se esiste l'utente
    pub async fn find_by_name(pool: &SqlitePool, username: &str) -> Result<Option<(i64, String)>> {
        let row = sqlx::query(
            "SELECT id, pass_hash FROM users WHERE username = ?",
        )
            .bind(username)
            .fetch_optional(pool)
            .await?;

        Ok(row.map(|r| {
            let id: i64 = r.get("id");
            let password: String = r.get("pass_hash");
            (id, password)
        }))
    }

    pub async fn create(pool: &SqlitePool, username: &str, password_hash: &str) -> Result<i64> {
        let res = sqlx::query(
            "INSERT INTO users(username, pass_hash, created_at) VALUES(?, ?, strftime('%s','now'))",
        )
            .bind(username)
            .bind(password_hash)
            .execute(pool)
            .await?;

        Ok(res.last_insert_rowid())
    }
}
