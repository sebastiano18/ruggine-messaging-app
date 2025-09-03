// repositories/user_repo.rs
use crate::error::Result;
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
}
