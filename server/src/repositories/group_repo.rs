use crate::error::Result;
use sqlx::{Row, SqlitePool};

#[derive(Debug, Clone)]
pub struct GroupRepo;

impl GroupRepo {
    pub async fn create(pool: &SqlitePool, name: &str, owner_id: i64) -> Result<i64> {
        let res = sqlx::query(
            "INSERT INTO groups(name, owner_id, created_at) VALUES(?, ?, strftime('%s','now'))",
        )
            .bind(name)
            .bind(owner_id)
            .execute(pool)
            .await?;

        Ok(res.last_insert_rowid())
    }

    pub async fn add_member(pool: &SqlitePool, group_id: i64, user_id: i64) -> Result<()> {
        sqlx::query(
            "INSERT OR IGNORE INTO group_members(group_id, user_id) VALUES(?, ?)",
        )
            .bind(group_id)
            .bind(user_id)
            .execute(pool)
            .await?;

        Ok(())
    }

    pub async fn by_owner(pool: &SqlitePool, owner_id: i64) -> Result<Vec<(i64, String)>> {
        let rows = sqlx::query(
            "SELECT id, name FROM groups WHERE owner_id = ? ORDER BY id DESC",
        )
            .bind(owner_id)
            .fetch_all(pool)
            .await?;

        let items = rows
            .into_iter()
            .map(|r| {
                let id: i64 = r.get("id");
                let name: String = r.get("name");
                (id, name)
            })
            .collect();

        Ok(items)
    }
}
