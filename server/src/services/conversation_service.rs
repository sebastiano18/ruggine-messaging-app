use serde_json::json;
use tracing::{error, info, warn};
use uuid::Uuid;
use crate::{error::Result, repositories::conversation_repo::ConversationRepo, state::AppState};
use crate::web_socket::utils::send_event_to_multiple_users;

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
    pub async fn mine(pool: &sqlx::SqlitePool, user_id: Uuid) -> Result<Vec<(Uuid, String, String, Uuid, i64, i64, i64, i64)>> {
        ConversationRepo::by_user(pool, user_id).await
    }

    // NUOVO: Ottieni singola conversazione
    pub async fn get_conversation(
        pool: &sqlx::SqlitePool,
        conversation_id: Uuid,
        user_id: Uuid
    ) -> Result<Option<(Uuid, String, String, Uuid, i64, i64, i64, i64)>> {
        ConversationRepo::get_single_conversation(pool, conversation_id, user_id).await
    }

    // Aggiungi membro (solo per gruppi e solo se sei owner)
    pub async fn add_member(
        pool: &sqlx::SqlitePool,
        conversation_id: Uuid,
        member_id: Uuid,
        requester_id: Uuid
    ) -> Result<()> {
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

    /// Elimina una conversazione (DM o Gruppo).
    /// - Gruppo: solo l'owner
    /// - DM: partecipante o autore di almeno un messaggio
    pub async fn delete_conversation(
        pool: &sqlx::SqlitePool,
        conversation_id: Uuid,
        requester_id: Uuid,
    ) -> Result<()> {
        let kind_opt = ConversationRepo::get_conversation_kind(pool, conversation_id).await?;
        let kind = match kind_opt {
            Some(k) => k,
            None => return Err(crate::error::AppError::NotFound),
        };

        match kind.as_str() {
            "group" => {
                let is_owner = ConversationRepo::is_owner(pool, conversation_id, requester_id).await?;
                if !is_owner {
                    return Err(crate::error::AppError::Unauthorized);
                }
            }
            "dm" => {
                let allowed = ConversationRepo::user_has_dm_access(pool, conversation_id, requester_id).await?;
                if !allowed {
                    return Err(crate::error::AppError::Unauthorized);
                }
            }
            _ => {
                return Err(crate::error::AppError::Unauthorized);
            }
        }

        ConversationRepo::delete_conversation(pool, conversation_id).await?;
        Ok(())
    }

    /// Permette a un partecipante di uscire da un gruppo
    /// - Solo per gruppi (non DM)
    /// - L'owner non può uscire (deve eliminare il gruppo)
    pub async fn leave_group(
        pool: &sqlx::SqlitePool,
        conversation_id: Uuid,
        requester_id: Uuid,
    ) -> Result<()> {
        let kind_opt = ConversationRepo::get_conversation_kind(pool, conversation_id).await?;
        let kind = match kind_opt {
            Some(k) => k,
            None => return Err(crate::error::AppError::NotFound),
        };

        if kind != "group" {
            return Err(crate::error::AppError::BadRequest("Non è un gruppo".to_string()));
        }

        let is_owner = ConversationRepo::is_owner(pool, conversation_id, requester_id).await?;
        if is_owner {
            return Err(crate::error::AppError::BadRequest("L'owner non può uscire dal gruppo, deve eliminarlo".to_string()));
        }

        let is_participant = ConversationRepo::is_participant(pool, conversation_id, requester_id).await?;
        if !is_participant {
            return Err(crate::error::AppError::Unauthorized);
        }

        ConversationRepo::remove_member(pool, conversation_id, requester_id).await?;
        Ok(())
    }

    pub async fn list_participant_ids(
        pool: &sqlx::Pool<sqlx::Sqlite>,
        conversation_id: Uuid,
    ) -> Result<Vec<Uuid>> {
        ConversationRepo::list_participant_ids(pool, conversation_id).await
    }

    // Ottieni i membri di una conversazione con i loro dettagli
    pub async fn get_members(
        pool: &sqlx::SqlitePool,
        conversation_id: Uuid,
        requester_id: Uuid,
    ) -> Result<Vec<(Uuid, String, String, i64)>> {
        if !ConversationRepo::is_participant(pool, conversation_id, requester_id).await? {
            return Err(crate::error::AppError::Unauthorized);
        }

        ConversationRepo::get_members(pool, conversation_id).await
    }

    // ✅ OTTIMIZZATO: Usa helper per batch
    pub async fn broadcast_message_deleted(
        st: &AppState,
        conversation_id: Uuid,
        message_id: Uuid,
        participant_ids: Vec<Uuid>,
    ) {
        info!(
            "Broadcasting and persisting message_deleted event for message {} in conversation {}",
            message_id, conversation_id
        );

        let event_data = json!({
            "message_id": message_id,
            "conversation_id": conversation_id,
        });
        

        if let Err(e) = send_event_to_multiple_users(
            st,
            &participant_ids,
            "message_deleted",
            &event_data,
            Some(conversation_id),
        ).await {
            warn!("Failed to broadcast message_deleted: {}", e);
        }
    }

    // ✅ OTTIMIZZATO: Usa helper per batch
    pub async fn broadcast_conversation_deleted(
        st: &AppState,
        conversation_id: Uuid,
        by: Uuid,
        participant_ids: Vec<Uuid>,
        include_author: bool,
    ) {
        let recipients: Vec<Uuid> = if include_author {
            participant_ids
        } else {
            participant_ids.into_iter().filter(|&id| id != by).collect()
        };

        if recipients.is_empty() {
            return;
        }

        let payload = json!({
            "type": "conversation_deleted",
            "conversation_id": conversation_id,
            "by": by,
            "timestamp": chrono::Utc::now().timestamp()
        });



        if let Err(e) = send_event_to_multiple_users(
            st,
            &recipients,
            "conversation_deleted",
            &payload,
            Some(conversation_id),
        ).await {
            warn!(
                "Failed to broadcast conversation_deleted for conv {}: {}",
                conversation_id, e
            );
        }
    }

    // Espelli un membro da una conversazione
    pub async fn kick_member(
        pool: &sqlx::SqlitePool,
        conversation_id: Uuid,
        requester_id: Uuid,
        user_id_to_kick: Uuid,
    ) -> Result<()> {
        if !ConversationRepo::is_owner(pool, conversation_id, requester_id).await? {
            return Err(crate::error::AppError::Unauthorized);
        }

        if requester_id == user_id_to_kick {
            return Err(crate::error::AppError::BadRequest("Non puoi espellere te stesso".to_string()));
        }

        if ConversationRepo::is_owner(pool, conversation_id, user_id_to_kick).await? {
            return Err(crate::error::AppError::BadRequest("Non puoi espellere il proprietario".to_string()));
        }

        ConversationRepo::remove_member(pool, conversation_id, user_id_to_kick).await
    }

    // ✅ OTTIMIZZATO: Usa helper per batch
    pub async fn notify_user_left_group(
        state: &AppState,
        conversation_id: Uuid,
        user_id: Uuid,
        username: &str,
    ) -> Result<()> {
        let participants = ConversationRepo::list_participant_ids(&state.pool, conversation_id)
            .await?
            .into_iter()
            .filter(|&id| id != user_id)
            .collect::<Vec<_>>();

        if participants.is_empty() {
            info!("No remaining participants in group {}", conversation_id);
            return Ok(());
        }

        info!(
            "Notifying {} participants that user {} ({}) left conversation {}",
            participants.len(), user_id, username, conversation_id
        );

        let event = json!({
            "type": "user_left_group",
            "conversation_id": conversation_id,
            "user_id": user_id,
            "username": username,
            "timestamp": chrono::Utc::now().timestamp()
        });



        if let Err(e) = send_event_to_multiple_users(
            state,
            &participants,
            "user_left_group",
            &event,
            Some(conversation_id),
        ).await {
            warn!("Failed to notify user_left_group: {}", e);
        }

        Ok(())
    }

    // ✅ OTTIMIZZATO: Usa helper per batch
    pub async fn notify_user_deleted_account(
        state: &AppState,
        conversation_id: Uuid,
        deleted_user_id: Uuid,
        deleted_username: &str,
    ) -> Result<()> {
        let participants = ConversationRepo::list_participant_ids(&state.pool, conversation_id)
            .await?
            .into_iter()
            .filter(|&id| id != deleted_user_id)
            .collect::<Vec<_>>();

        if participants.is_empty() {
            info!("No remaining participants in group {}", conversation_id);
            return Ok(());
        }

        info!(
            "Notifying {} participants that user {} ({}) deleted their account in conversation {}",
            participants.len(), deleted_user_id, deleted_username, conversation_id
        );

        let event = json!({
            "type": "user_deleted_account",
            "conversation_id": conversation_id,
            "deleted_user_id": deleted_user_id,
            "deleted_username": deleted_username,
        });
        

        if let Err(e) = send_event_to_multiple_users(
            state,
            &participants,
            "user_deleted_account",
            &event,
            Some(conversation_id),
        ).await {
            warn!("Failed to notify user_deleted_account: {}", e);
        }

        Ok(())
    }

    // ✅ OTTIMIZZATO: Usa helper per batch
    pub async fn notify_conversation_deleted(
        state: &AppState,
        conversation_id: Uuid,
        deleted_user_id: Uuid,
        reason: &str,
    ) -> Result<()> {
        let participants = ConversationRepo::list_participant_ids(&state.pool, conversation_id)
            .await?
            .into_iter()
            .filter(|&id| id != deleted_user_id)
            .collect::<Vec<_>>();

        if participants.is_empty() {
            return Ok(());
        }

        let notification = json!({
            "type": "conversation_deleted",
            "conversation_id": conversation_id.to_string(),
            "reason": reason,
            "deleted_user_id": deleted_user_id.to_string(),
        });



        if let Err(e) = send_event_to_multiple_users(
            state,
            &participants,
            "conversation_deleted",
            &notification,
            Some(conversation_id),
        ).await {
            warn!("Failed to notify conversation_deleted: {}", e);
        }

        Ok(())
    }
}