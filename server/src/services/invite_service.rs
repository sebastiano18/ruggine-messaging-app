use uuid::Uuid;
use crate::{
    error::{Result, AppError},
    repositories::{
        conversation_repo::ConversationRepo,
        invite_repo::InviteRepo
    }
};

#[derive(Debug, Clone)]
pub struct InviteService;

impl InviteService {
    /// Crea un invito per una conversazione (solo partecipanti possono creare inviti)
    pub async fn create_invite(
        pool: &sqlx::SqlitePool,
        conversation_id: Uuid,
        requester_id: Uuid,
        expires_in_hours: Option<i32> // Default 24 ore se None
    ) -> Result<String> {
        // Verifica che il richiedente sia partecipante della conversazione
        if !ConversationRepo::is_participant(pool, conversation_id, requester_id).await? {
            return Err(AppError::Forbidden);
        }

        // Verifica che la conversazione sia di tipo "group" (non "dm")
        let conversation_kind = ConversationRepo::get_conversation_kind(pool, conversation_id).await?
            .ok_or(AppError::NotFound)?;
        if conversation_kind != "group" {
            return Err(AppError::BadRequest("Gli inviti sono disponibili solo per i gruppi".into()));
        }
        
        // Calcola scadenza (default 24 ore)
        let hours = expires_in_hours.unwrap_or(24);
        let expires_at = chrono::Utc::now().timestamp() + (hours as i64 * 3600);

        InviteRepo::create_invite(pool, conversation_id, expires_at).await
    }

    /// Usa un invito per unirsi a una conversazione
    pub async fn use_invite(
        pool: &sqlx::SqlitePool,
        token: &str,
        user_id: Uuid
    ) -> Result<Uuid> { // Restituisce conversation_id
        // Trova l'invito valido
        let conversation_id = InviteRepo::find_valid_invite(pool, token).await?
            .ok_or(AppError::NotFound)?;

        // Verifica che l'utente non sia già partecipante
        if ConversationRepo::is_participant(pool, conversation_id, user_id).await? {
            return Err(AppError::BadRequest("Sei già partecipante di questa conversazione".into()));
        }

        // Aggiungi l'utente alla conversazione
        ConversationRepo::add_member(pool, conversation_id, user_id).await?;

        // Marca l'invito come utilizzato (nel tuo schema è usa-e-getta)
        InviteRepo::mark_as_used(pool, token).await?;

        Ok(conversation_id)
    }

    /// Ottieni tutti gli inviti per una conversazione
    pub async fn get_invites_by_conversation(
        pool: &sqlx::SqlitePool,
        conversation_id: Uuid,
        requester_id: Uuid
    ) -> Result<Vec<InviteInfo>> {
        // Verifica che il richiedente sia partecipante
        if !ConversationRepo::is_participant(pool, conversation_id, requester_id).await? {
            return Err(AppError::Forbidden);
        }

        let invites = InviteRepo::by_conversation(pool, conversation_id).await?;

        Ok(invites.into_iter().map(|(id, token, expires_at, used)| {
            InviteInfo {
                id,
                token,
                expires_at,
                used,
                is_expired: expires_at <= chrono::Utc::now().timestamp(),
            }
        }).collect())
    }

    /// Elimina un invito (revoca)
    pub async fn delete_invite(
        pool: &sqlx::SqlitePool,
        token: &str,
        requester_id: Uuid
    ) -> Result<()> {
        // Trova l'invito per verificare i permessi
        let conversation_id = InviteRepo::find_valid_invite(pool, token).await?
            .ok_or(AppError::NotFound)?;

        // Verifica che il richiedente sia partecipante della conversazione
        if !ConversationRepo::is_participant(pool, conversation_id, requester_id).await? {
            return Err(AppError::Forbidden);
        }

        InviteRepo::delete_invite(pool, token).await
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct InviteInfo {
    pub id: String,           // UUID dell'invito
    pub token: String,        // Token dell'invito
    pub expires_at: i64,      // Timestamp di scadenza
    pub used: bool,           // Se è stato utilizzato
    pub is_expired: bool,     // Se è scaduto
}