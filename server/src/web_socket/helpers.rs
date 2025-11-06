use chrono::Utc;
use serde_json::{Value, json};
use sqlx::Row;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::{
    error::{AppError, Result},
    state::AppState,
};

use super::actor::OutboundMsg;

/// Struttura per i dati di una nuova conversazione
#[derive(Debug)]
struct NewConversationData {
    conversation_id: Uuid,
    client_temp_id: Option<String>,
    creator_id: Uuid,
    creator_username: String,
    other_participant_id: Uuid,
    other_participant_username: String,
    initial_message_id: Uuid,
    initial_message_content: String,
    initial_message_sequence: u64,
    client_msg_id: Option<String>,
    created_at: i64,
}

pub async fn broadcast_to_conversation(
    state: &AppState,
    conversation_id: Uuid,
    payload: Value,
) -> Result<usize> {
    let tx = state.get_or_create_broadcast_tx(conversation_id).await;
    match tx.send(payload) {
        Ok(n) => {
            info!("broadcast {} subs for {}", n, conversation_id);
            Ok(n)
        }
        Err(e) => {
            warn!("broadcast fail {}: {}", conversation_id, e);
            Err(AppError::Internal(format!("broadcast error: {e}")))
        }
    }
}

/// Router principale per gestire i messaggi in arrivo
/// Router principale per gestire i messaggi in arrivo
pub async fn handle_incoming_message(
    state: &AppState,
    value: &mut Value,
    user_id: Uuid,
    username: &str,
) -> Result<()> {
    // Determina il tipo di operazione
    let message_type = value
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("chat_message");

    match message_type {
        "create_conversation" => handle_create_conversation(state, value, user_id, username).await,
        "chat_message" | "message" => {
            // La logica di decisione è ora dentro handle_chat_message
            handle_chat_message(state, value, user_id, username).await
        }
        "invite_user" => handle_invite_user(state, value, user_id).await,
        _ => Err(AppError::BadRequest(format!(
            "Unknown message type: {}",
            message_type
        ))),
    }
}

/// Verifica se una conversazione esiste nel database
async fn verify_conversation_exists(state: &AppState, conversation_id: Uuid) -> Result<bool> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conversations WHERE id = ?")
        .bind(conversation_id.to_string())
        .fetch_one(&state.pool)
        .await
        .map_err(AppError::from)?;

    Ok(count > 0)
}

/// Gestisce un messaggio che crea anche una nuova conversazione
async fn handle_message_with_new_conversation(
    state: &AppState,
    value: &mut Value,
    user_id: Uuid,
    username: &str,
) -> Result<()> {
    info!("Creating new conversation with initial message");

    // Estrai i dati necessari
    let content = value
        .get("content")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let client_msg_id = value
        .get("client_msg_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Crea la conversazione con il messaggio iniziale
    let mut create_request = json!({
        "type": "create_conversation",
        "target_username": value.get("target_username"),
        "conversation_type": "dm",
        "initial_message": content,
        "client_msg_id": client_msg_id
    });

    // CORREZIONE: Passa il client_temp_id se presente nel messaggio originale
    if let Some(temp_id) = value.get("client_temp_id") {
        create_request["client_temp_id"] = temp_id.clone();
    } else {
        // Fallback: se c'è un cid/conversation_id che non è un UUID valido nel DB,
        // potrebbe essere un client_temp_id
        if let Some(cid_str) = value
            .get("cid")
            .or_else(|| value.get("conversation_id"))
            .and_then(|v| v.as_str())
        {
            // Verifica se è un UUID
            if let Ok(uuid) = Uuid::parse_str(cid_str) {
                // Se è un UUID, verifica se esiste nel DB
                if !verify_conversation_exists(state, uuid)
                    .await
                    .unwrap_or(false)
                {
                    // Non esiste, quindi potrebbe essere un client_temp_id
                    create_request["client_temp_id"] = json!(cid_str);
                }
            } else {
                // Non è un UUID valido, quindi è sicuramente un client_temp_id
                create_request["client_temp_id"] = json!(cid_str);
            }
        }
    }

    handle_create_conversation(state, &mut create_request, user_id, username).await
}

/// Gestisce la creazione di una nuova conversazione
pub async fn handle_create_conversation(
    state: &AppState,
    value: &mut Value,
    user_id: Uuid,
    username: &str,
) -> Result<()> {
    info!("HANDLING NEW CONVERSATION REQUEST: {}", value);

    // Estrai i dati necessari per creare la conversazione
    let client_temp_id = value
        .get("client_temp_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let target_username = value
        .get("target_username")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            AppError::BadRequest("Target username required for new conversation".into())
        })?
        .trim();

    let conversation_type = value
        .get("conversation_type")
        .and_then(|v| v.as_str())
        .unwrap_or("dm");

    let initial_message = value
        .get("initial_message")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty());

    // Gestione client_temp_id - verifica duplicati
    let conversation_id = if let Some(ref temp_id) = client_temp_id {
        if let Some(existing_id) = state
            .conversation_confirmation_cache
            .get_by_temp_id(temp_id)
            .await
        {
            info!(
                "Conversation with temp_id {} already exists: {}",
                temp_id, existing_id
            );

            // Se c'è un messaggio iniziale, salvalo per la conversazione esistente
            if let Some(content) = initial_message {
                let conversation_id_str = existing_id.to_string();
                let user_id_str = user_id.to_string();
                let message_sequence = state.get_next_message_sequence(existing_id).await?;
                let msg_id = Uuid::new_v4();
                let msg_id_str = msg_id.to_string();
                let ts = Utc::now().timestamp();

                sqlx::query(
                    "INSERT INTO messages (id, conversation_id, author_id, content, created_at, sequence_num) VALUES (?, ?, ?, ?, ?, ?)"
                )
                    .bind(&msg_id_str)
                    .bind(&conversation_id_str)
                    .bind(&user_id_str)
                    .bind(&content)
                    .bind(ts)
                    .bind(message_sequence as i64)
                    .execute(&state.pool)
                    .await
                    .map_err(AppError::from)?;

                info!(
                    "Added message {} to existing conversation {}",
                    msg_id, existing_id
                );

                // Gestisci client_msg_id se presente
                if let Some(client_msg_id) = value.get("client_msg_id").and_then(|v| v.as_str()) {
                    state
                        .message_confirmation_cache
                        .insert(msg_id, client_msg_id.to_string())
                        .await;
                }

                // Invia conferma messaggio
                send_message_confirmation(
                    state,
                    msg_id,
                    existing_id,
                    message_sequence,
                    value
                        .get("client_msg_id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    ts,
                    user_id,
                )
                    .await?;

                // Broadcast il messaggio
                let broadcast_msg = json!({
                    "type": "chat_message",
                    "id": msg_id,
                    "conversation_id": existing_id,
                    "author_id": user_id,
                    "author_username": username,
                    "content": content,
                    "created_at": ts,
                    "sequence_num": message_sequence
                });

                broadcast_to_conversation(state, existing_id, broadcast_msg).await?;
            }

            return Ok(());
        } else {
            // Cache conversation_id generato
            let new_id = Uuid::new_v4();
            state
                .conversation_confirmation_cache
                .insert(new_id, temp_id.clone())
                .await;
            info!(
                "Cached new conversation {} with client_temp_id {}",
                new_id, temp_id
            );
            new_id
        }
    } else {
        Uuid::new_v4()
    };

    let conversation_id_str = conversation_id.to_string();
    let user_id_str = user_id.to_string();

    // Verifica che il target esista
    let target_row = sqlx::query("SELECT id, username FROM users WHERE username = ?")
        .bind(target_username)
        .fetch_optional(&state.pool)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::NotFound)?;

    let target_user_id_str: String = target_row.try_get("id").map_err(AppError::from)?;
    let target_user_id = Uuid::parse_str(&target_user_id_str).map_err(|_| {
        AppError::Internal("Invalid UUID format in database for target user".into())
    })?;
    let target_username_actual: String = target_row.try_get("username").map_err(AppError::from)?;

    // Impedisci conversazioni con se stessi
    if target_user_id == user_id {
        return Err(AppError::BadRequest(
            "Cannot create conversation with yourself".into(),
        ));
    }

    // Verifica duplicati per DM
    if conversation_type == "dm" {
        let existing_dm = sqlx::query(
            "SELECT c.id FROM conversations c
             JOIN participants p1 ON c.id = p1.conversation_id
             JOIN participants p2 ON c.id = p2.conversation_id
             WHERE c.kind = 'dm'
             AND p1.user_id = ? AND p2.user_id = ?
             LIMIT 1",
        )
            .bind(&user_id_str)
            .bind(&target_user_id_str)
            .fetch_optional(&state.pool)
            .await
            .map_err(AppError::from)?;

        if let Some(row) = existing_dm {
            let existing_id: String = row.try_get("id").map_err(AppError::from)?;
            return Err(AppError::BadRequest(format!(
                "DM conversation already exists: {}",
                existing_id
            )));
        }
    }

    // Inizia transazione
    let mut tx = state.pool.begin().await.map_err(AppError::from)?;

    // Crea conversazione
    let ts = Utc::now().timestamp();
    sqlx::query(
        "INSERT INTO conversations (id, kind, title, owner_id, created_at) VALUES (?, ?, ?, ?, ?)"
    )
        .bind(&conversation_id_str)
        .bind(conversation_type)
        .bind(None::<String>)
        .bind(&user_id_str)  // Aggiungi owner_id
        .bind(ts)
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;

    // Aggiungi partecipanti
    for (participant_id_str, role) in &[(user_id_str.clone(), "member"), (target_user_id_str.clone(), "member")] {
        sqlx::query(
            "INSERT INTO participants (conversation_id, user_id, role) VALUES (?, ?, ?)"
        )
            .bind(&conversation_id_str)
            .bind(participant_id_str)
            .bind(role)
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;
    }

    // Salva messaggio iniziale se presente
    let mut initial_msg_data: Option<(Uuid, String, u64, Option<String>)> = None;

    if let Some(content) = initial_message {
        let msg_id = Uuid::new_v4();
        let msg_id_str = msg_id.to_string();

        // Inizializza o aggiorna la sequenza nella tabella message_sequences
        let sequence_result = sqlx::query(
            "INSERT INTO message_sequences (conversation_id, current_sequence, last_updated)
             VALUES (?, 1, ?)
             ON CONFLICT(conversation_id) DO UPDATE
             SET current_sequence = current_sequence + 1,
                 last_updated = ?
             RETURNING current_sequence"
        )
            .bind(&conversation_id_str)
            .bind(ts)
            .bind(ts)
            .fetch_one(&mut *tx)
            .await;

        // Se RETURNING non funziona in SQLite, usa questo approccio alternativo:
        let sequence = if sequence_result.is_err() {
            // Prima prova INSERT
            let _insert_result = sqlx::query(
                "INSERT OR IGNORE INTO message_sequences (conversation_id, current_sequence, last_updated)
                 VALUES (?, 0, ?)"
            )
                .bind(&conversation_id_str)
                .bind(ts)
                .execute(&mut *tx)
                .await;

            // Poi UPDATE
            sqlx::query(
                "UPDATE message_sequences
                 SET current_sequence = current_sequence + 1,
                     last_updated = ?
                 WHERE conversation_id = ?"
            )
                .bind(ts)
                .bind(&conversation_id_str)
                .execute(&mut *tx)
                .await
                .map_err(AppError::from)?;

            // Infine SELECT per ottenere il valore
            let row = sqlx::query(
                "SELECT current_sequence FROM message_sequences WHERE conversation_id = ?"
            )
                .bind(&conversation_id_str)
                .fetch_one(&mut *tx)
                .await
                .map_err(AppError::from)?;

            row.try_get::<i64, _>("current_sequence").map_err(AppError::from)?
        } else {
            sequence_result.unwrap().try_get::<i64, _>("current_sequence").map_err(AppError::from)?
        };

        // Salva il messaggio
        sqlx::query(
            "INSERT INTO messages (id, conversation_id, author_id, content, created_at, sequence_num) VALUES (?, ?, ?, ?, ?, ?)"
        )
            .bind(&msg_id_str)
            .bind(&conversation_id_str)
            .bind(&user_id_str)
            .bind(&content)
            .bind(ts)
            .bind(sequence)
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;

        // Estrai client_msg_id se presente
        let client_msg_id = value
            .get("client_msg_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if let Some(ref client_id) = client_msg_id {
            state
                .message_confirmation_cache
                .insert(msg_id, client_id.clone())
                .await;
            info!(
                "Cached client_msg_id {} for initial message {}",
                client_id, msg_id
            );
        }

        initial_msg_data = Some((msg_id, content.to_string(), sequence as u64, client_msg_id));
    }

    // Commit transazione
    tx.commit().await.map_err(AppError::from)?;

    info!(
        "Created conversation {} between {} and {}",
        conversation_id, username, target_username_actual
    );

    // IMPORTANTE: Se c'è un messaggio iniziale, invia la conferma
    if let Some((msg_id, _, sequence, client_msg_id)) = &initial_msg_data {
        send_message_confirmation(
            state,
            *msg_id,
            conversation_id,
            *sequence,
            client_msg_id.clone(),
            ts,
            user_id,
        )
            .await?;
        info!("Sent message confirmation for initial message {}", msg_id);
    }

    // Prepara i dati per l'evento
    let new_conv_data = NewConversationData {
        conversation_id,
        client_temp_id,
        creator_id: user_id,
        creator_username: username.to_string(),
        other_participant_id: target_user_id,
        other_participant_username: target_username_actual,
        initial_message_id: initial_msg_data
            .as_ref()
            .map(|(id, _, _, _)| *id)
            .unwrap_or(Uuid::nil()),
        initial_message_content: initial_msg_data
            .as_ref()
            .map(|(_, c, _, _)| c.clone())
            .unwrap_or_default(),
        initial_message_sequence: initial_msg_data
            .as_ref()
            .map(|(_, _, s, _)| *s)
            .unwrap_or(0),
        client_msg_id: initial_msg_data.as_ref().and_then(|(_, _, _, cid)| cid.clone()),
        created_at: ts,
    };

    // Invia notifiche ai partecipanti
    send_conversation_created_events(state, new_conv_data).await?;

    // FIX: Se c'è un messaggio iniziale, crea anche gli eventi new_message per tutti i partecipanti
    if let Some((msg_id, content, sequence, client_msg_id)) = initial_msg_data {
        info!("Creating new_message events for initial message {} in conversation {}",
              msg_id, conversation_id);

        // Recupera tutti i partecipanti (dovrebbero essere solo 2 per un DM)
        let participants = vec![user_id, target_user_id];

        // Prepara i dati del messaggio per l'evento
        let mut message_event_data = json!({
            "id": msg_id,
            "conversation_id": conversation_id,
            "author_id": user_id,
            "author_username": username,
            "content": content,
            "created_at": ts,
            "sequence_num": sequence,
        });

        if let Some(ref client_id) = client_msg_id {
            message_event_data["client_msg_id"] = json!(client_id);
        }

        // Crea un evento user per ogni partecipante
        for participant_id in participants {
            let event_payload = json!({
                "type": "new_message",
                "conversation_id": conversation_id,
                "conversation_sequence": sequence,
                "message": message_event_data.clone()
            });

            match state
                .send_sequenced_event_to_user(
                    participant_id,
                    "new_message",
                    event_payload,
                    Some(conversation_id),
                )
                .await
            {
                Ok(seq) => {
                    info!(
                        "Sent new_message event (seq={}) for initial message to user {}",
                        seq, participant_id
                    );
                }
                Err(e) => {
                    error!(
                        "Failed to send new_message event to user {}: {}",
                        participant_id, e
                    );
                }
            }
        }

        info!("Initial message events created for all participants");
    }

    Ok(())
}

/// Gestisce l'invio di un messaggio in una conversazione esistente
pub async fn handle_chat_message(
    state: &AppState,
    value: &mut Value,
    user_id: Uuid,
    username: &str,
) -> Result<()> {
    info!("HANDLING CHAT MESSAGE: {}", value);

    // Controlla se è un messaggio che crea una nuova conversazione
    let has_temp_id = value.get("client_temp_id").is_some();
    let has_target = value.get("target_username").is_some();

    if has_temp_id && has_target {
        // Controlla se la conversazione è già stata creata
        if let Some(temp_id) = value.get("client_temp_id").and_then(|v| v.as_str()) {
            if let Some(conversation_id) = state
                .conversation_confirmation_cache
                .get_by_temp_id(temp_id)
                .await
            {
                // Conversazione già creata, procedi come messaggio normale
                info!(
                    "Conversation for temp_id {} already exists: {}",
                    temp_id, conversation_id
                );
                // Continua con il flusso normale usando conversation_id esistente
            } else {
                // Prima volta, crea la conversazione
                info!(
                    "First message for temp_id {}, creating conversation",
                    temp_id
                );
                return handle_message_with_new_conversation(state, value, user_id, username).await;
            }
        }
    }

    // Estrai l'ID della conversazione (gestisce sia conversation_id che client_temp_id)
    let conversation_id = extract_conversation_id(state, value).await?;
    let conversation_id_str = conversation_id.to_string();
    let user_id_str = user_id.to_string();

    // Estrai il contenuto del messaggio
    let content = value
        .get("content")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| AppError::BadRequest("Message content cannot be empty".into()))?;

    // Estrai client_msg_id se presente
    let client_msg_id = value
        .get("client_msg_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Verifica autorizzazione (che l'utente sia partecipante)
    let is_participant = verify_participant(state, conversation_id, user_id).await?;
    if !is_participant {
        return Err(AppError::Forbidden);
    }

    // Ottieni la sequenza del messaggio
    let message_sequence = state.get_next_message_sequence(conversation_id).await?;

    // Crea e salva il messaggio
    let msg_id = Uuid::new_v4();
    let msg_id_str = msg_id.to_string();
    let ts = Utc::now().timestamp();

    sqlx::query(
        "INSERT INTO messages (id, conversation_id, author_id, content, created_at, sequence_num) VALUES (?, ?, ?, ?, ?, ?)"
    )
        .bind(&msg_id_str)
        .bind(&conversation_id_str)
        .bind(&user_id_str)
        .bind(content)
        .bind(ts)
        .bind(message_sequence as i64)
        .execute(&state.pool)
        .await
        .map_err(AppError::from)?;

    info!(
        "Message {} saved with sequence {}",
        msg_id, message_sequence
    );

    // ⭐ NUOVO: Auto-marca il messaggio come letto per l'autore
    sqlx::query(
        "UPDATE participants
         SET last_read_sequence = ?
         WHERE conversation_id = ? AND user_id = ?"
    )
        .bind(message_sequence as i64)
        .bind(&conversation_id_str)
        .bind(&user_id_str)
        .execute(&state.pool)
        .await
        .map_err(AppError::from)?;

    debug!(
        "Auto-marked message seq {} as read for author {}",
        message_sequence, user_id
    );

    // Salva client_msg_id in cache se presente
    if let Some(ref client_id) = client_msg_id {
        state
            .message_confirmation_cache
            .insert(msg_id, client_id.clone())
            .await;
        info!("Cached client_msg_id {} for message {}", client_id, msg_id);
    }

    // Invia conferma al mittente
    send_message_confirmation(
        state,
        msg_id,
        conversation_id,
        message_sequence,
        client_msg_id.clone(),
        ts,
        user_id,
    )
        .await?;

    // ========================================================================
    // Crea eventi user per TUTTI i partecipanti
    // ========================================================================

    // Recupera tutti i partecipanti
    let participants = get_conversation_participants(state, conversation_id).await?;

    info!(
        "Creating user events for {} participants in conversation {}",
        participants.len(),
        conversation_id
    );

    // Prepara i dati del messaggio per l'evento
    let mut message_event_data = json!({
        "id": msg_id,
        "conversation_id": conversation_id,
        "author_id": user_id,
        "author_username": username,
        "content": content,
        "created_at": ts,
        "conversation_sequence": message_sequence,
    });

    if let Some(ref client_id) = client_msg_id {
        message_event_data["client_msg_id"] = json!(client_id);
    }

    // Crea un evento user per ogni partecipante
    for participant_id in participants {
        let event_payload = json!({
            "type": "new_message",
            "conversation_id": conversation_id,
            "conversation_sequence": message_sequence,
            "message": message_event_data.clone()
        });

        match state
            .send_sequenced_event_to_user(
                participant_id,
                "new_message",
                event_payload,
                Some(conversation_id),
            )
            .await
        {
            Ok(user_seq) => {
                debug!(
                    "Event seq={} created for user {} - msg {} conv {}",
                    user_seq, participant_id, msg_id, conversation_id
                );
            }
            Err(e) => {
                warn!("Failed to create event for user {}: {}", participant_id, e);
            }
        }
    }

    info!("User events created for all participants");

    // ========================================================================

    // Broadcast il messaggio agli altri partecipanti
    let mut broadcast_msg = json!({
        "type": "chat_message",
        "id": msg_id,
        "conversation_id": conversation_id,
        "author_id": user_id,
        "author_username": username,
        "content": content,
        "created_at": ts,
        "sequence": message_sequence
    });

    if let Some(ref client_id) = client_msg_id {
        broadcast_msg["client_msg_id"] = json!(client_id);
    }

    match broadcast_to_conversation(state, conversation_id, broadcast_msg).await {
        Ok(delivered) if delivered > 0 => {
            info!("Message {} delivered to {} receivers", msg_id, delivered);
        }
        Ok(_) => {
            info!("Message {} stored for future delivery", msg_id);
        }
        Err(e) => {
            warn!("Failed to broadcast message {}: {}", msg_id, e);
        }
    }

    Ok(())
}

/// Estrae l'ID della conversazione dal valore, gestendo anche client_temp_id
/// Estrae l'ID della conversazione dal valore, gestendo anche client_temp_id
async fn extract_conversation_id(state: &AppState, value: &Value) -> Result<Uuid> {
    // PRIORITÀ 1: Se c'è un client_temp_id esplicito, usa quello
    if let Some(temp_id) = value.get("client_temp_id").and_then(|v| v.as_str()) {
        if let Some(real_id) = state
            .conversation_confirmation_cache
            .get_by_temp_id(temp_id)
            .await
        {
            info!(
                "Resolved client_temp_id {} to conversation {}",
                temp_id, real_id
            );
            return Ok(real_id);
        }
        // client_temp_id non trovato nella cache - NON è un errore
        // Potrebbe essere il primo messaggio
        debug!(
            "client_temp_id {} not found in cache, might be first message",
            temp_id
        );
    }

    // PRIORITÀ 2: Usa conversation_id/cid normale
    if let Some(cid_str) = value
        .get("conversation_id")
        .or_else(|| value.get("cid"))
        .and_then(|v| v.as_str())
    {
        // Prova a parsare come UUID
        if let Ok(uuid) = Uuid::parse_str(cid_str) {
            // IMPORTANTE: Verifica che la conversazione esista nel DB
            if verify_conversation_exists(state, uuid).await? {
                return Ok(uuid);
            }

            // UUID non esistente - potrebbe essere nella cache come temp_id
            if let Some(real_id) = state
                .conversation_confirmation_cache
                .get_by_temp_id(cid_str)
                .await
            {
                info!(
                    "Found UUID {} in temp_id cache, resolved to {}",
                    cid_str, real_id
                );
                return Ok(real_id);
            }

            return Err(AppError::BadRequest(format!(
                "Conversation {} does not exist",
                uuid
            )));
        }

        // Non è un UUID valido, prova come client_temp_id nella cache
        if let Some(real_id) = state
            .conversation_confirmation_cache
            .get_by_temp_id(cid_str)
            .await
        {
            info!(
                "Resolved non-UUID client_temp_id {} to conversation {}",
                cid_str, real_id
            );
            return Ok(real_id);
        }

        return Err(AppError::BadRequest(format!(
            "Invalid conversation ID: {}",
            cid_str
        )));
    }

    Err(AppError::BadRequest("Missing conversation ID".into()))
}

/// Verifica se un utente è partecipante di una conversazione
async fn verify_participant(
    state: &AppState,
    conversation_id: Uuid,
    user_id: Uuid,
) -> Result<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM participants WHERE conversation_id = ? AND user_id = ?",
    )
    .bind(conversation_id.to_string())
    .bind(user_id.to_string())
    .fetch_one(&state.pool)
    .await
    .map_err(AppError::from)?;

    Ok(count > 0)
}

/// Invia conferma di un messaggio al mittente
async fn send_message_confirmation(
    state: &AppState,
    msg_id: Uuid,
    conversation_id: Uuid,
    sequence: u64,
    client_msg_id: Option<String>,
    created_at: i64,
    user_id: Uuid,
) -> Result<()> {
    let confirmation = json!({
        "type": "message_confirmation",
        "client_msg_id": client_msg_id,
        "server_msg_id": msg_id.to_string(),
        "conversation_id": conversation_id,
        "sequence": sequence,
        "created_at": created_at,
        "status": "saved"
    });

    let user_tx = state.get_or_create_user_notification_channel(user_id).await;
    match user_tx.send(confirmation) {
        Ok(receiver_count) => {
            debug!(
                "Sent message confirmation to user {} ({} receivers)",
                user_id, receiver_count
            );
            Ok(())
        }
        Err(e) => {
            warn!(
                "Failed to send message confirmation to user {}: {}",
                user_id, e
            );
            Err(AppError::Internal(format!(
                "Failed to send confirmation: {}",
                e
            )))
        }
    }
}

/// Invia conferma di creazione conversazione (per duplicati)
#[allow(dead_code)]
async fn send_conversation_confirmation(
    state: &AppState,
    conversation_id: Uuid,
    client_temp_id: String,
    user_id: Uuid,
) -> Result<()> {
    // Recupera i dettagli della conversazione dal database
    let conv_row = sqlx::query("SELECT kind, owner_id, created_at FROM conversations WHERE id = ?")
        .bind(conversation_id.to_string())
        .fetch_one(&state.pool)
        .await
        .map_err(AppError::from)?;

    let kind: String = conv_row.try_get("kind").map_err(AppError::from)?;
    let created_at: i64 = conv_row.try_get("created_at").map_err(AppError::from)?;

    let confirmation = json!({
        "type": "conversation_confirmation",
        "conversation": {
            "id": conversation_id,
            "client_temp_id": client_temp_id,
            "kind": kind,
            "created_at": created_at,
            "status": "already_exists"
        }
    });

    let user_tx = state.get_or_create_user_notification_channel(user_id).await;
    match user_tx.send(confirmation) {
        Ok(_) => {
            info!(
                "Sent duplicate conversation confirmation to user {}",
                user_id
            );
            Ok(())
        }
        Err(e) => {
            warn!("Failed to send conversation confirmation: {}", e);
            Err(AppError::Internal(format!(
                "Failed to send confirmation: {}",
                e
            )))
        }
    }
}

/// Invia eventi di creazione conversazione a tutti i partecipanti
async fn send_conversation_created_events(
    state: &AppState,
    data: NewConversationData,
) -> Result<()> {
    let participants = vec![
        (
            data.creator_id,
            data.creator_username.clone(),
            data.other_participant_username.clone(),
        ),
        (
            data.other_participant_id,
            data.other_participant_username.clone(),
            data.creator_username.clone(),
        ),
    ];

    for (participant_id, _participant_username, other_username) in participants {
        let mut event = json!({
            "type": "conversation_created_complete",
            "conversation": {
                "id": data.conversation_id,
                "kind": "dm",
                "title": null,
                "display_title": other_username,
                "owner_id": data.creator_id,
                "created_at": data.created_at,
                "message_count": if data.initial_message_id != Uuid::nil() { 1 } else { 0 },
                "participants": [
                    {
                        "user_id": data.creator_id.to_string(),
                        "username": data.creator_username.clone(),
                        "role": "member"
                    },
                    {
                        "user_id": data.other_participant_id.to_string(),
                        "username": data.other_participant_username.clone(),
                        "role": "member"
                    }
                ]
            }
        });

        // Aggiungi last_message se c'è un messaggio iniziale
        if data.initial_message_id != Uuid::nil() {
            let mut last_msg = json!({
                "id": data.initial_message_id,
                "author_id": data.creator_id,
                "author_username": data.creator_username.clone(),
                "content": data.initial_message_content.clone(),
                "created_at": data.created_at,
                "sequence_num": data.initial_message_sequence
            });

            if let Some(ref client_id) = data.client_msg_id {
                last_msg["client_msg_id"] = json!(client_id);
            }

            event["conversation"]["last_message"] = last_msg;
        }

        // Per il creatore, aggiungi client_temp_id e usa tipo diverso
        let event_type = if participant_id == data.creator_id {
            if let Some(ref temp_id) = data.client_temp_id {
                event["conversation"]["client_temp_id"] = json!(temp_id);
                event["type"] = json!("conversation_confirmation");
                "conversation_confirmation"
            } else {
                "conversation_created_complete"
            }
        } else {
            "conversation_created_complete"
        };

        // Invia evento sequenziato
        match state
            .send_sequenced_event_to_user(
                participant_id,
                event_type,
                event,
                Some(data.conversation_id),
            )
            .await
        {
            Ok(seq) => {
                info!(
                    "Sent {} (seq={}) to user {}",
                    event_type, seq, participant_id
                );
            }
            Err(e) => {
                warn!(
                    "Failed to send {} to user {}: {}",
                    event_type, participant_id, e
                );
            }
        }
    }

    Ok(())
}



pub async fn handle_user_notification(
    state: &AppState,
    notification: &Value,
    user_id: Uuid,
    out_tx: &mpsc::Sender<OutboundMsg>,
) -> Result<Option<Uuid>> {
    let notification_type = notification
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    match notification_type {
        "conversation_created_complete" | "conversation_confirmation" => {
            let conversation_id = notification
                .get("conversation")
                .and_then(|c| c.get("id"))
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
                .ok_or_else(|| AppError::BadRequest("invalid conversation_id".into()))?;

            info!(
                "Processing {} for user {} - conversation {}",
                notification_type, user_id, conversation_id
            );

            // Setup subscription al broadcast channel
            if let Err(e) = setup_conversation_subscription(state, user_id, conversation_id).await {
                warn!(
                    "Failed to setup conversation subscription for user {}: {}",
                    user_id, e
                );
            }

            // Forward dell'evento al client
            if let Ok(notification_txt) = serde_json::to_string(&notification) {
                if out_tx
                    .send(OutboundMsg::Text(notification_txt))
                    .await
                    .is_err()
                {
                    warn!("Failed to send {} to user {}", notification_type, user_id);
                } else {
                    info!("Forwarded {} to user {}", notification_type, user_id);
                }
            }

            // Ritorna il conversation_id per permettere setup aggiuntivo (es. stream manager)
            Ok(Some(conversation_id))
        }
        "conversation_deleted" => {
            // Forward dell'evento al client
            if let Ok(notification_txt) = serde_json::to_string(&notification) {
                if out_tx
                    .send(OutboundMsg::Text(notification_txt))
                    .await
                    .is_err()
                {
                    warn!("Failed to send {} to user {}", notification_type, user_id);
                } else {
                    debug!("Forwarded {} to user {}", notification_type, user_id);
                }
            }
            Ok(None)
        }
        _ => {
            // Forward altre notifiche
            if notification.get("sequence").is_some() {
                if let Ok(notification_txt) = serde_json::to_string(&notification) {
                    if out_tx
                        .send(OutboundMsg::Text(notification_txt))
                        .await
                        .is_err()
                    {
                        warn!("Failed to send sequenced event to user {}", user_id);
                    } else {
                        debug!("Forwarded sequenced event to user {}", user_id);
                    }
                }
            } else {
                debug!(
                    "Received notification type '{}' for user {}",
                    notification_type, user_id
                );
                if let Ok(txt) = serde_json::to_string(&notification) {
                    let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                }
            }
            Ok(None)
        }
    }
}

/// Configura l'iscrizione dell'utente al broadcast channel della conversazione
async fn setup_conversation_subscription(
    state: &AppState,
    user_id: Uuid,
    conversation_id: Uuid,
) -> Result<()> {
    let conversation_id_str = conversation_id.to_string();
    let user_id_str = user_id.to_string();

    let is_participant: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM participants WHERE conversation_id = ? AND user_id = ?",
    )
    .bind(&conversation_id_str)
    .bind(&user_id_str)
    .fetch_one(&state.pool)
    .await
    .map_err(AppError::from)?;

    if is_participant == 0 {
        return Err(AppError::Forbidden);
    }

    let conv_tx = state.get_or_create_broadcast_tx(conversation_id).await;
    let receiver_count = conv_tx.receiver_count();

    info!(
        "Conversation {} broadcast channel ready for user {} ({} current receivers)",
        conversation_id, user_id, receiver_count
    );

    Ok(())
}

/// Cleanup canali vuoti quando un utente si disconnette
pub async fn cleanup_empty_channels(state: &AppState, user_id: Uuid) {
    state.cleanup_empty_channels(user_id).await;
}

/// Recupera tutti i partecipanti di una conversazione
async fn get_conversation_participants(
    state: &AppState,
    conversation_id: Uuid,
) -> Result<Vec<Uuid>> {
    let conversation_id_str = conversation_id.to_string();

    let rows = sqlx::query("SELECT user_id FROM participants WHERE conversation_id = ?")
        .bind(&conversation_id_str)
        .fetch_all(&state.pool)
        .await
        .map_err(AppError::from)?;

    let participants: Vec<Uuid> = rows
        .iter()
        .filter_map(|row| {
            row.try_get::<String, _>("user_id")
                .ok()
                .and_then(|s| Uuid::parse_str(&s).ok())
        })
        .collect();

    debug!(
        "Found {} participants for conversation {}",
        participants.len(),
        conversation_id
    );

    Ok(participants)
}

/// Gestisce la richiesta di resume degli eventi utente
pub async fn handle_user_events_resume_request(
    state: &AppState,
    value: &Value,
    user_id: Uuid,
    out_tx: &mpsc::Sender<OutboundMsg>,
) -> Result<()> {
    debug!("User resume request from user {}", user_id);

    let from_sequence = value
        .get("from_sequence")
        .and_then(|s| s.as_u64())
        .unwrap_or(0);

    let limit = value
        .get("limit")
        .and_then(|s| s.as_i64())
        .unwrap_or(100)
        .min(1000);

    match state
        .get_user_events_since(user_id, from_sequence, limit)
        .await
    {
        Ok(events) => {
            if !events.is_empty() {
                info!(
                    "Sending {} user events in resume to user {}",
                    events.len(),
                    user_id
                );
                state
                    .send_user_events_resume(user_id, events, out_tx)
                    .await?;
            } else {
                let response = json!({
                    "type": "user_resume_complete",
                    "from_sequence": from_sequence,
                    "current_sequence": state.get_current_user_sequence(user_id).await.unwrap_or(0),
                    "events_count": 0,
                    "message": "No events to resume"
                });
                if let Ok(txt) = serde_json::to_string(&response) {
                    out_tx
                        .send(OutboundMsg::Text(txt))
                        .await
                        .map_err(|_| AppError::Internal("Failed to send response".into()))?;
                }
            }
            Ok(())
        }
        Err(e) => {
            error!("Failed to get user events for resume: {}", e);
            let error_response = json!({
                "type": "error",
                "message": "Failed to retrieve user events",
                "error_code": "RESUME_ERROR",
                "details": e.to_string()
            });
            if let Ok(txt) = serde_json::to_string(&error_response) {
                let _ = out_tx.send(OutboundMsg::Text(txt)).await;
            }
            Err(AppError::Internal(format!(
                "Failed to get user events: {}",
                e
            )))
        }
    }
}

/// Gestisce l'invito di un utente a un gruppo tramite WebSocket
pub async fn handle_invite_user(
    state: &AppState,
    value: &Value,
    inviter_id: Uuid,
) -> Result<()> {
    info!("Handling invite_user request");

    // Estrai conversation_id
    let conversation_id_str = value
        .get("cid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("Missing conversation_id (cid)".into()))?;

    let conversation_id = Uuid::parse_str(conversation_id_str)
        .map_err(|_| AppError::BadRequest("Invalid conversation_id format".into()))?;

    // Supporta sia singolo username che array di usernames
    let target_usernames: Vec<String> = if let Some(username_str) = value.get("username").and_then(|v| v.as_str()) {
        // Singolo username
        vec![username_str.trim().to_string()]
    } else if let Some(usernames_array) = value.get("usernames").and_then(|v| v.as_array()) {
        // Array di usernames
        usernames_array
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        return Err(AppError::BadRequest("Missing 'username' or 'usernames' field".into()));
    };

    if target_usernames.is_empty() {
        return Err(AppError::BadRequest("No valid usernames provided".into()));
    }

    info!(
        "User {} inviting {} users to conversation {}",
        inviter_id, target_usernames.len(), conversation_id
    );

    // Verifica che la conversazione esista e sia un gruppo (una sola volta)
    let conv_row = sqlx::query(
        "SELECT kind, owner_id FROM conversations WHERE id = ?"
    )
        .bind(conversation_id.to_string())
        .fetch_optional(&state.pool)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::NotFound)?;

    let kind: String = conv_row.try_get("kind").map_err(AppError::from)?;
    let owner_id_str: String = conv_row.try_get("owner_id").map_err(AppError::from)?;
    let owner_id = Uuid::parse_str(&owner_id_str)
        .map_err(|_| AppError::Internal("Invalid owner_id in database".into()))?;

    // Verifica che sia un gruppo
    if kind != "group" {
        return Err(AppError::BadRequest("Can only invite users to groups".into()));
    }

    // Verifica che l'inviter sia l'owner
    if inviter_id != owner_id {
        return Err(AppError::Forbidden);
    }

    // Ottieni informazioni sulla conversazione (una sola volta)
    let conv_info = sqlx::query(
        "SELECT id, kind, title, owner_id, created_at FROM conversations WHERE id = ?"
    )
        .bind(conversation_id.to_string())
        .fetch_one(&state.pool)
        .await
        .map_err(AppError::from)?;

    let conv_title: Option<String> = conv_info.try_get("title").ok();
    let conv_created_at: i64 = conv_info.try_get("created_at").map_err(AppError::from)?;

    // Statistiche per il riepilogo finale
    let mut added_count = 0;
    let mut skipped_count = 0;

    // Processa ogni username
    for target_username in target_usernames {
        info!("Processing invite for username: '{}'", target_username);

        // Trova l'utente da invitare
        let target_user_row = match sqlx::query("SELECT id, username FROM users WHERE username = ?")
            .bind(&target_username)
            .fetch_optional(&state.pool)
            .await
        {
            Ok(Some(row)) => row,
            Ok(None) => {
                warn!("User '{}' not found, skipping", target_username);
                skipped_count += 1;
                continue;
            }
            Err(e) => {
                warn!("Database error looking up user '{}': {}, skipping", target_username, e);
                skipped_count += 1;
                continue;
            }
        };

        let target_user_id_str: String = match target_user_row.try_get("id") {
            Ok(id) => id,
            Err(e) => {
                warn!("Error extracting user id for '{}': {}, skipping", target_username, e);
                skipped_count += 1;
                continue;
            }
        };

        let target_user_id = match Uuid::parse_str(&target_user_id_str) {
            Ok(id) => id,
            Err(e) => {
                warn!("Invalid UUID for user '{}': {}, skipping", target_username, e);
                skipped_count += 1;
                continue;
            }
        };

        let target_username_actual: String = match target_user_row.try_get("username") {
            Ok(name) => name,
            Err(_) => target_username.clone(),
        };

        // Verifica che l'utente non sia già membro
        let is_member: i64 = match sqlx::query_scalar(
            "SELECT COUNT(*) FROM participants WHERE conversation_id = ? AND user_id = ?"
        )
            .bind(conversation_id.to_string())
            .bind(&target_user_id_str)
            .fetch_one(&state.pool)
            .await
        {
            Ok(count) => count,
            Err(e) => {
                warn!("Error checking membership for '{}': {}, skipping", target_username_actual, e);
                skipped_count += 1;
                continue;
            }
        };

        if is_member > 0 {
            info!("User '{}' is already a member, skipping", target_username_actual);
            skipped_count += 1;
            continue;
        }

        // Aggiungi l'utente al gruppo
        match sqlx::query(
            "INSERT INTO participants (conversation_id, user_id, role) VALUES (?, ?, ?)"
        )
            .bind(conversation_id.to_string())
            .bind(&target_user_id_str)
            .bind("member")
            .execute(&state.pool)
            .await
        {
            Ok(_) => {
                info!("User '{}' added to group {} by owner {}", target_username_actual, conversation_id, inviter_id);
            }
            Err(e) => {
                error!("Failed to add user '{}' to group: {}, skipping", target_username_actual, e);
                skipped_count += 1;
                continue;
            }
        }

        // Crea evento per notificare il nuovo membro
        let user_sequence = match state.get_next_user_sequence(target_user_id).await {
            Ok(seq) => seq,
            Err(e) => {
                warn!("Failed to get user sequence for '{}': {}", target_username_actual, e);
                0 // Fallback, ma l'utente è stato aggiunto
            }
        };

        let ts = Utc::now().timestamp();

        // Salva evento nella tabella user_events per il nuovo membro
        let _ = sqlx::query(
            "INSERT INTO user_events (user_id, sequence_num, event_type, event_data, conversation_id, created_at)
             VALUES (?, ?, ?, ?, ?, ?)"
        )
            .bind(&target_user_id_str)
            .bind(user_sequence as i64)
            .bind("new_conversation")
            .bind(json!({
                "conversation": {
                    "id": conversation_id,
                    "kind": &kind,
                    "title": &conv_title,
                    "owner_id": owner_id,
                    "created_at": conv_created_at,
                    "last_read_sequence": 0,
                    "last_activity": ts,
                    "last_msg_seq": 0
                }
            }).to_string())
            .bind(conversation_id.to_string())
            .bind(ts)
            .execute(&state.pool)
            .await;

        // Notifica il nuovo membro via user notification channel
        let notification = json!({
            "type": "user_notification",
            "sequence": user_sequence,
            "event_type": "new_conversation",
            "event_data": {
                "conversation": {
                    "id": conversation_id,
                    "kind": &kind,
                    "title": &conv_title,
                    "owner_id": owner_id,
                    "created_at": conv_created_at,
                    "last_read_sequence": 0,
                    "last_activity": ts,
                    "last_msg_seq": 0
                }
            },
            "conversation_id": conversation_id
        });

        if let Some(user_tx) = state.user_notification_channels.read().await.get(&target_user_id) {
            match user_tx.send(notification.clone()) {
                Ok(_) => info!("Sent new_conversation notification to user {}", target_user_id),
                Err(e) => warn!("Failed to send notification to user {}: {}", target_user_id, e),
            }
        } else {
            info!("User {} has no notification channel (offline or not subscribed)", target_user_id);
        }

        // Broadcast a tutti i membri del gruppo (incluso il nuovo)
        let member_added_msg = json!({
            "type": "member_added",
            "conversation_id": conversation_id,
            "user_id": target_user_id,
            "username": target_username_actual,
            "added_by": inviter_id,
            "timestamp": ts
        });

        // Best effort broadcast - non è un errore fatale se fallisce
        match broadcast_to_conversation(state, conversation_id, member_added_msg).await {
            Ok(n) => info!("Broadcast member_added to {} subscribers", n),
            Err(e) => warn!("Failed to broadcast member_added (non-fatal): {}", e),
        }

        added_count += 1;
    }

    // Log riepilogo finale
    info!(
        "Invite operation completed: {} added, {} skipped for conversation {}",
        added_count, skipped_count, conversation_id
    );

    if added_count == 0 && skipped_count > 0 {
        return Err(AppError::BadRequest(format!(
            "No users were added. {} user(s) skipped.",
            skipped_count
        )));
    }

    Ok(())
}
