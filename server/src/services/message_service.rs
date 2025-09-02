use crate::{error::Result, repositories::message_repo::MessageRepo};
#[derive(Debug, Clone)]
pub struct MessageService;
impl MessageService {
    pub async fn list(
        pool: &sqlx::SqlitePool,
        cid: i64,
        limit: i64,
    ) -> Result<Vec<(i64, i64, String, i64)>> {
        MessageRepo::list_by_conversation(pool, cid, limit).await
    }
    pub async fn post(
        pool: &sqlx::SqlitePool,
        cid: i64,
        author: i64,
        content: &str,
    ) -> Result<i64> {
        MessageRepo::insert(pool, cid, author, content).await
    }
}
