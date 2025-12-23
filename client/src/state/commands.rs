// state/commands.rs - Azioni utente e comandi verso il server

use super::{AppState, PendingDeletion};
use crate::models::*;
use tracing::{debug, error, info, warn};
use uuid::Uuid;
use std::time::Instant;

// ========================================
// COMANDI UTENTE (User-initiated actions)
// ========================================

impl AppState {
    // === USER VERIFICATION ===

    /// Richiede la creazione di un DM - prima verifica che l'utente esista
    pub fn request_dm_creation(&mut self, target_username: String) {
        // CHECK CONNESSIONE
        if self.ws_status != WsStatus::Connected {
            warn!("Cannot create DM: WebSocket not connected");
            let _ = self.ui_tx.send(UiEvent::Error(ErrorType::Connection));
            return;
        }

        // Pulisci username
        let username = target_username.trim().to_string();
        if username.is_empty() {
            let _ = self.ui_tx.send(UiEvent::Error(ErrorType::Generic(
                "Username non può essere vuoto".to_string(),
            )));
            return;
        }

        // Non permettere DM con se stessi
        if username.to_lowercase() == self.username.to_lowercase() {
            let _ = self.ui_tx.send(UiEvent::Error(ErrorType::Generic(
                "Non puoi chattare con te stesso".to_string(),
            )));
            return;
        }

        // Controlla se esiste già una conversazione con questo utente
        if let Some(ref convs) = self.conversations {
            if convs.iter().any(|c| {
                c.kind == "dm" && c.title.to_lowercase() == username.to_lowercase()
            }) {
                let _ = self.ui_tx.send(UiEvent::Error(ErrorType::Generic(
                    "Esiste già una chat con questo utente".to_string(),
                )));
                return;
            }
        }

        // Controlla se c'è già un check in corso
        if self.pending_user_check.is_some() {
            warn!("User check already in progress");
            return;
        }

        // Genera request_id per correlare richiesta/risposta
        let request_id = Uuid::new_v4().to_string();

        // Salva stato pending
        self.pending_user_check = Some(username.clone());
        self.user_check_request_id = Some(request_id.clone());
        self.user_check_timestamp = Some(Instant::now());

        info!("Checking if user exists: {}", username);

        // Invia richiesta di verifica via WebSocket
        self.send_via_websocket(Outgoing::CheckUser {
            username,
            request_id,
        });
    }

    // === CONVERSATION DELETION ===

    pub fn request_delete_confirmation(&mut self, conversation: &ConversationDto) {
        self.pending_deletion = Some(PendingDeletion {
            conversation: conversation.clone(),
        });
    }

    pub fn cancel_delete_confirmation(&mut self) {
        self.pending_deletion = None;
    }

    pub fn execute_pending_deletion(&mut self) {
        let Some(pending) = self.pending_deletion.take() else {
            return;
        };

        let conversation = pending.conversation;
        let cid = conversation.id;

        if self.is_dm_stub(cid) {
            self.remove_dm_stub(cid);
            if let Some(ref mut list) = self.conversations {
                list.retain(|c| c.id != cid);
            }
            self.conversation_messages.remove(&cid);

            if self.cid == Some(cid) {
                self.cid = None;
                self.conv_title.clear();
                self.messages.clear();
                self.page = Page::Conversations;
            }

            let _ = self
                .ui_tx
                .send(UiEvent::Info("Chat privata rimossa (locale)".into()));
        } else {
            if self.ws_status == WsStatus::Connected {
                // Determina se l'utente è owner o partecipante
                let is_owner = self
                    .user_id
                    .map_or(false, |uid| uid == conversation.owner_id);

                if conversation.kind == "group" && !is_owner {
                    // Partecipante che vuole uscire dal gruppo
                    self.send_via_websocket(Outgoing::LeaveGroup { cid });
                    let _ = self
                        .ui_tx
                        .send(UiEvent::Info("Uscita dal gruppo...".into()));
                } else {
                    // Owner che elimina il gruppo o eliminazione di DM
                    self.send_via_websocket(Outgoing::DeleteConversation { cid });

                    if conversation.kind == "group" {
                        let _ = self.ui_tx.send(UiEvent::Info("Gruppo eliminato".into()));
                    } else {
                        let _ = self
                            .ui_tx
                            .send(UiEvent::Info("Conversazione eliminata".into()));
                    }
                }
            } else {
                // Connessione non attiva
                let _ = self.ui_tx.send(UiEvent::Error(ErrorType::Connection));
            }
        }
    }

    // === WEBSOCKET COMMUNICATION ===

    pub fn send_via_websocket(&self, outgoing: Outgoing) {
        let error_type = match &outgoing {
            Outgoing::ChatMessage { .. } => ErrorType::MessageSend,
            Outgoing::DeleteConversation { .. } => ErrorType::ConversationDelete,
            Outgoing::InviteUser { .. } => ErrorType::Invite,
            Outgoing::LeaveGroup { .. } => ErrorType::GroupLeave,
            Outgoing::DeleteMessage { .. } => ErrorType::MessageDelete,
            Outgoing::CreateGroup { .. } | Outgoing::CreateGroupWithParticipants { .. } => {
                ErrorType::GroupCreate
            }
            Outgoing::RequestUserResume { .. } | Outgoing::RequestMessagesResume { .. } => {
                ErrorType::DataRecovery
            }
            Outgoing::CheckUser { .. } => ErrorType::Connection,
            _ => return, // Silenzioso per Ping, Typing, etc.
        };

        if let Err(_) = self.ui_to_net_tx.try_send(outgoing) {
            warn!("Failed to send message to WebSocket channel");
            let _ = self.ui_tx.send(UiEvent::Error(error_type));
        }
    }

    pub fn send_chat_message_ws(&self, content: String, client_msg_id: Option<String>) {
        if let Some(cid) = self.cid {
            // Estrai solo lo username dalla tupla (username, created_at)
            let target_username = self.dm_stubs.get(&cid).map(|(username, _)| username.clone());

            if target_username.is_some() {
                debug!(
                    "Sending message for DM stub {} with target: {:?}",
                    cid, target_username
                );
            }

            self.send_via_websocket(Outgoing::ChatMessage {
                cid,
                content,
                target_username,
                target_usernames: None,
                client_msg_id,
            });
        }
    }

    pub fn send_invite_users(&self, cid: Uuid, usernames: Vec<String>) {
        info!(
            "Sending invite for {} users to conversation {}",
            usernames.len(),
            cid
        );
        self.send_via_websocket(Outgoing::InviteUser { cid, usernames });
    }

    // === MESSAGE MANAGEMENT ===

    pub fn delete_message(&mut self, message_id: Uuid) {
        if self.ws_status == WsStatus::Connected {
            self.send_via_websocket(Outgoing::DeleteMessage { mid: message_id });
            info!(
                "Sent delete request for message {} via WebSocket",
                message_id
            );
        } else {
            error!("Cannot delete message, WebSocket is not connected.");
            let _ = self.ui_tx.send(UiEvent::Error(ErrorType::Connection));
        }
    }

    // === GROUP MANAGEMENT ===

    pub fn create_group_with_participants(&mut self) {
        // CHECK CONNESSIONE: Blocca subito se non connesso
        if self.ws_status != WsStatus::Connected {
            warn!("Cannot create group: WebSocket not connected");
            let _ = self.ui_tx.send(UiEvent::Error(ErrorType::Connection));
            return;
        }

        let group_name = self.create_group_popup.group_name.trim().to_string();
        let participants: Vec<String> = self
            .create_group_popup
            .selected_participants
            .iter()
            .cloned()
            .collect();

        info!(
        "Creating group '{}' with {} participants via WebSocket: {:?}",
        group_name,
        participants.len(),
        participants
    );

        // Crea stub per il gruppo
        let stub_id = Uuid::new_v4();
        let now = chrono::Utc::now().timestamp(); //  Riusa questo timestamp

        let stub_conversation = ConversationDto {
            id: stub_id,
            kind: "group".to_string(),
            title: group_name.clone(),
            owner_id: self.user_id.unwrap_or(Uuid::nil()),
            created_at: now,
            last_read_sequence: 0,
            last_activity: now,
            last_msg_seq: 0,
        };

        // Aggiungi stub alla lista conversazioni
        if let Some(ref mut convs) = self.conversations {
            convs.insert(0, stub_conversation);
        }

        // Traccia lo stub CON TIMESTAMP
        self.group_stubs
            .insert(stub_id, (group_name.clone(), Instant::now()));

        //  Aggiungi i membri selezionati alla members_list dello stub
        let mut stub_members: Vec<ParticipantInfo> = Vec::new();

        // Aggiungi l'utente corrente come owner
        if let Some(user_id) = self.user_id {
            stub_members.push(ParticipantInfo {
                user_id,
                username: self.username.clone(),
                role: "owner".to_string(),
                joined_at: Some(now), //  Aggiungi joined_at per owner
            });
        }

        // Aggiungi i partecipanti selezionati
        for username in &participants {
            stub_members.push(ParticipantInfo {
                user_id: Uuid::nil(), // Placeholder - verrà aggiornato dal server
                username: username.clone(),
                role: "member".to_string(),
                joined_at: Some(now), //  Aggiungi joined_at per membri
            });
        }

        self.members_list.insert(stub_id, stub_members);
        info!(
        "Added {} members to stub {} members_list",
        participants.len() + 1,
        stub_id
    );

        // Apri il gruppo stub
        self.cid = Some(stub_id);
        self.page = Page::Chat;
        self.conv_title = group_name.clone();

        // Invia al server
        let outgoing = Outgoing::CreateGroupWithParticipants {
            group_name,
            participant_usernames: participants,
            client_temp_id: Some(stub_id.to_string()),
        };

        if let Err(_e) = self.ui_to_net_tx.try_send(outgoing) {
            // Cleanup in caso di errore
            if let Some(ref mut convs) = self.conversations {
                convs.retain(|c| c.id != stub_id);
            }
            self.group_stubs.remove(&stub_id);
            self.conversation_messages.remove(&stub_id);
            self.members_list.remove(&stub_id);
            self.messages.clear();
            self.cid = None;
            self.page = Page::Conversations;

            // Notifica l'utente dell'errore
            let _ = self.ui_tx.send(UiEvent::Error(ErrorType::GroupCreate));

            return;
        }

        info!("Created group stub {} and opened it", stub_id);
        self.create_group_popup.reset();
    }

    /// Crea un DM stub per iniziare una nuova chat privata
    pub fn create_dm_stub(&mut self, target_username: String) -> Option<Uuid> {
        // CHECK CONNESSIONE: Blocca subito se non connesso
        if self.ws_status != WsStatus::Connected {
            warn!("Cannot create DM: WebSocket not connected");
            let _ = self.ui_tx.send(UiEvent::Error(ErrorType::Connection));
            return None;
        }

        let stub_id = Uuid::new_v4();

        info!(
            "Creating DM stub for {} with temp ID: {}",
            target_username, stub_id
        );

        // Traccia lo stub CON TIMESTAMP
        self.dm_stubs
            .insert(stub_id, (target_username.clone(), Instant::now()));

        Some(stub_id)
    }
    

    // === MESSAGE LOADING ===

    pub fn load_older_messages(&mut self) {
        let Some(cid) = self.cid else { return };
        let Some(ref token) = self.token else { return };

        // Guard: evita caricamenti multipli simultanei
        if self.is_loading_more {
            return;
        }

        // Guard: se sappiamo che non ci sono più messaggi, non caricare
        if !*self.has_more_messages.get(&cid).unwrap_or(&true) {
            return;
        }

        // Determina il punto di partenza per la paginazione
        let before_seq = self
            .messages
            .first()
            .and_then(|m| m.sequence_num)
            .map(|seq| seq as i64);

        // Se il primo messaggio ha sequence 1, siamo all'inizio
        if let Some(seq) = before_seq {
            if seq <= 1 {
                self.has_more_messages.insert(cid, false);
                info!("First message has sequence 1, no older messages to load");
                return;
            }
        }

        self.is_loading_more = true;

        let base = self.base.clone();
        let token = token.clone();
        let tx = self.ui_tx.clone();
        let waker = self.egui_waker.clone();

        self.rt.spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

            match crate::api::chat::get_messages_paginated(
                &base,
                &token,
                cid,
                Some(30),
                before_seq,
            )
                .await
            {
                Ok(messages) => {
                    let _ = tx.send(UiEvent::OlderMessagesLoaded(messages));
                    waker();
                }
                Err(e) => {
                    error!("Failed to load messages: {}", e);
                    let _ = tx.send(UiEvent::LoadingError);
                    waker();
                }
            }
        });
    }
    
}