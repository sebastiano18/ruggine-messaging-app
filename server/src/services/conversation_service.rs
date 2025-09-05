use uuid::Uuid;
use crate::{error::Result, repositories::conversation_repo::ConversationRepo};

#[derive(Debug, Clone)]
pub struct ConversationService;

impl ConversationService {
    // Crea un nuovo gruppo
    pub async fn create_group(pool: &sqlx::SqlitePool, name: &str, owner_id: Uuid) -> Result<Uuid> {
        ConversationRepo::create_group(pool, name, owner_id).await
    }

    // Crea o trova una DM
    pub async fn create_dm(pool: &sqlx::SqlitePool, user1_id: Uuid, user2_id: Uuid) -> Result<Uuid> {
        ConversationRepo::create_dm(pool, user1_id, user2_id).await
    }

    // Ottieni le conversazioni di un utente con tutti i campi necessari
    pub async fn mine(pool: &sqlx::SqlitePool, user_id: Uuid) -> Result<Vec<(Uuid, String, String, Uuid, i64)>> {
        ConversationRepo::by_user(pool, user_id).await
    }

    // Aggiungi membro (solo per gruppi e solo se sei owner)
    pub async fn add_member(
        pool: &sqlx::SqlitePool,
        conversation_id: Uuid,
        member_id: Uuid,
        requester_id: Uuid
    ) -> Result<()> {
        // Verifica che il richiedente sia il proprietario della conversazione
        if !ConversationRepo::is_owner(pool, conversation_id, requester_id).await? {
            return Err(crate::error::AppError::Unauthorized);
        }

        ConversationRepo::add_member(pool, conversation_id, member_id).await
    }

    // Verifica se un utente può accedere a una conversazione
    pub async fn can_access(
        pool: &sqlx::SqlitePool,
        conversation_id: Uuid,
        user_id: Uuid
    ) -> Result<bool> {
        ConversationRepo::is_participant(pool, conversation_id, user_id).await
    }
}