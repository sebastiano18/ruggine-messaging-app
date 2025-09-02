use crate::error::Result;
use sqlx::{Row, SqlitePool};

#[derive(Debug, Clone)]
pub struct UserRepo;

impl UserRepo {
    /// Ritorna (id, password_hash) se esiste l'utente
    pub async fn find_by_name(pool: &SqlitePool, name: &str) -> Result<Option<(i64, String)>> {
        let row = sqlx::query(
            "SELECT id, password FROM users WHERE name = ?",
        )
            .bind(name)
            .fetch_optional(pool)
            .await?;

        Ok(row.map(|r| {
            let id: i64 = r.get("id");
            let password: String = r.get("password");
            (id, password)
        }))
    }

    pub async fn create(pool: &SqlitePool, name: &str, password_hash: &str) -> Result<i64> {
        let res = sqlx::query(
            "INSERT INTO users(name, password, created_at) VALUES(?, ?, strftime('%s','now'))",
        )
            .bind(name)
            .bind(password_hash)
            .execute(pool)
            .await?;

        Ok(res.last_insert_rowid())
    }
}
