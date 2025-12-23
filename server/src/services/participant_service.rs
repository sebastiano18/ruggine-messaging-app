use crate::error::Result;
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ParticipantService;

impl ParticipantService {
    /// Marca i messaggi come letti fino a una certa sequence
    pub async fn mark_read(
        pool: &SqlitePool,
        conversation_id: Uuid,
        user_id: Uuid,
        sequence_num: i64,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE participants
            SET last_read_sequence = MAX(last_read_sequence, ?)
            WHERE conversation_id = ? AND user_id = ?
            "#
        )
            .bind(sequence_num)
            .bind(conversation_id.to_string())
            .bind(user_id.to_string())
            .execute(pool)
            .await?;

        Ok(())
    }
}