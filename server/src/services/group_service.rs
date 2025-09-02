use crate::{error::Result, repositories::group_repo::GroupRepo};
#[derive(Debug, Clone)]
pub struct GroupService;
impl GroupService {
    pub async fn create(pool: &sqlx::SqlitePool, name: &str, owner: i64) -> Result<i64> {
        GroupRepo::create(pool, name, owner).await
    }
    pub async fn add_member(pool: &sqlx::SqlitePool, gid: i64, uid: i64) -> Result<()> {
        GroupRepo::add_member(pool, gid, uid).await
    }
    pub async fn mine(pool: &sqlx::SqlitePool, owner: i64) -> Result<Vec<(i64, String)>> {
        GroupRepo::by_owner(pool, owner).await
    }
}

