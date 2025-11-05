use serde_json::json;
use tracing::{info, warn};
use uuid::Uuid;
use crate::{error::Result, repositories::conversation_repo::ConversationRepo, state::AppState};

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

    /// Elimina una conversazione (DM o Gruppo).
    /// - Gruppo: solo l'owner
    /// - DM: partecipante o autore di almeno un messaggio
    pub async fn delete_conversation(
        pool: &sqlx::SqlitePool,
        conversation_id: Uuid,
        requester_id: Uuid,
    ) -> Result<()> {
        // Scopri il tipo di conversazione
        let kind_opt = ConversationRepo::get_conversation_kind(pool, conversation_id).await?;
        let kind = match kind_opt {
            Some(k) => k,
            None => return Err(crate::error::AppError::NotFound),
        };

        match kind.as_str() {
            "group" => {
                // Solo owner
                let is_owner = ConversationRepo::is_owner(pool, conversation_id, requester_id).await?;
                if !is_owner {
                    return Err(crate::error::AppError::Unauthorized);
                }
            }
            "dm" => {
                // Partecipante o autore di almeno un messaggio
                let allowed = ConversationRepo::user_has_dm_access(pool, conversation_id, requester_id).await?;
                if !allowed {
                    return Err(crate::error::AppError::Unauthorized);
                }
            }
            _ => {
                // Tipo sconosciuto: trattalo come non autorizzato
                return Err(crate::error::AppError::Unauthorized);
            }
        }

        // Esegui la cancellazione (cascade rimuove messaggi/partecipanti/inviti)
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
        // Verifica che sia un gruppo
        let kind_opt = ConversationRepo::get_conversation_kind(pool, conversation_id).await?;
        let kind = match kind_opt {
            Some(k) => k,
            None => return Err(crate::error::AppError::NotFound),
        };

        if kind != "group" {
            return Err(crate::error::AppError::BadRequest("Non è un gruppo".to_string()));
        }

        // Verifica che l'utente non sia l'owner
        let is_owner = ConversationRepo::is_owner(pool, conversation_id, requester_id).await?;
        if is_owner {
            return Err(crate::error::AppError::BadRequest("L'owner non può uscire dal gruppo, deve eliminarlo".to_string()));
        }

        // Verifica che sia effettivamente un partecipante
        let is_participant = ConversationRepo::is_participant(pool, conversation_id, requester_id).await?;
        if !is_participant {
            return Err(crate::error::AppError::Unauthorized);
        }

        // Rimuove il partecipante
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
        // Verifica che il richiedente sia un partecipante della conversazione
        if !ConversationRepo::is_participant(pool, conversation_id, requester_id).await? {
            return Err(crate::error::AppError::Unauthorized);
        }

        ConversationRepo::get_members(pool, conversation_id).await
    }

    pub async fn broadcast_conversation_deleted(
        st: &AppState,
        conversation_id: Uuid,
        by: Uuid,
        participant_ids: Vec<Uuid>,
        include_author: bool,
    ) {
        let payload = json!({
            "type": "conversation_deleted",
            "conversation_id": conversation_id,
            "by": by,
            "timestamp": chrono::Utc::now().timestamp()
        });

        for pid in participant_ids {
            if !include_author && pid == by {
                continue;
            }

            if let Err(e) = st
                .send_sequenced_event_to_user(
                    pid,
                    "conversation_deleted",
                    payload.clone(),
                    Some(conversation_id),
                )
                .await
            {
                warn!(
                    error = %e,
                    user = %pid,
                    conv = %conversation_id,
                    "Failed to send conversation_deleted"
                );
            }
        }
    }

    // Espelli un membro da una conversazione
    pub async fn kick_member(
        pool: &sqlx::SqlitePool,
        conversation_id: Uuid,
        requester_id: Uuid,
        user_id_to_kick: Uuid,
    ) -> Result<()> {
        // Verifica che il richiedente sia il proprietario della conversazione
        if !ConversationRepo::is_owner(pool, conversation_id, requester_id).await? {
            return Err(crate::error::AppError::Unauthorized);
        }

        // Verifica che non stia cercando di espellere se stesso
        if requester_id == user_id_to_kick {
            return Err(crate::error::AppError::BadRequest("Non puoi espellere te stesso".to_string()));
        }

        // Verifica che l'utente da espellere non sia il proprietario
        if ConversationRepo::is_owner(pool, conversation_id, user_id_to_kick).await? {
            return Err(crate::error::AppError::BadRequest("Non puoi espellere il proprietario".to_string()));
        }

        // Rimuovi il membro
        ConversationRepo::remove_member(pool, conversation_id, user_id_to_kick).await
    }

    pub async fn broadcast_user_left_group(
        st: &AppState,
        conversation_id: Uuid,
        user_id: Uuid,
        username: String,
        participant_ids: Vec<Uuid>,
    ) {
        info!(
            "Broadcasting user_left_group: user={} ({}) left conversation={} to {} participants",
            user_id, username, conversation_id, participant_ids.len()
        );

        let payload = json!({
            "type": "user_left_group",
            "conversation_id": conversation_id,
            "user_id": user_id,
            "username": username,
            "timestamp": chrono::Utc::now().timestamp()
        });

        for pid in participant_ids {
            match st
                .send_sequenced_event_to_user(
                    pid,
                    "user_left_group",
                    payload.clone(),
                    Some(conversation_id),
                )
                .await
            {
                Ok(seq) => {
                    info!(
                        "✅ Sent user_left_group event (seq={}) to user {}",
                        seq, pid
                    );
                }
                Err(e) => {
                    warn!(
                        "❌ Failed to send user_left_group to user {}: {}",
                        pid, e
                    );
                }
            }
        }
    }
}