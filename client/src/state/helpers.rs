// state/helpers.rs - Getter e funzioni di utility

use super::{AppState, STUB_TIMEOUT, USER_CHECK_TIMEOUT};
use crate::models::{Page, UiEvent, ErrorType};
use std::collections::HashMap;
use std::time::Instant;
use tracing::{debug, info, warn};
use uuid::Uuid;

// ========================================
// GETTER E UTILITY
// ========================================

impl AppState {
    /// Verifica se l'utente è autenticato
    pub fn is_authenticated(&self) -> bool {
        self.token.is_some()
    }

    /// Conta il numero totale di messaggi in cache
    pub fn get_total_cached_messages(&self) -> usize {
        self.conversation_messages.values().map(|v| v.len()).sum()
    }

    /// Ottiene informazioni di debug sullo stato dell'app
    pub fn get_debug_info(&self) -> HashMap<String, String> {
        let mut info = HashMap::new();
        info.insert(
            "conversations".to_string(),
            self.conversations
                .as_ref()
                .map_or(0, |c| c.len())
                .to_string(),
        );
        info.insert(
            "cached_messages".to_string(),
            self.get_total_cached_messages().to_string(),
        );
        info.insert("dm_stubs".to_string(), self.dm_stubs.len().to_string());
        info.insert(
            "group_stubs".to_string(),
            self.group_stubs.len().to_string(),
        );
        info.insert(
            "pending_user_check".to_string(),
            self.pending_user_check.is_some().to_string(),
        );
        info
    }

    /// Drena e processa tutti gli eventi in coda dal channel
    pub fn drain_events(&mut self) {
        while let Ok(ev) = self.ui_rx.try_recv() {
            crate::app::events::EventDispatcher::handle_event(self, ev);
        }

        // Cleanup periodico delle conferme
        use std::time::Duration;
        if self.last_confirmation_cleanup.elapsed() > Duration::from_secs(5) {
            self.cleanup_pending_confirmations();
            self.cleanup_pending_user_check();
            self.last_confirmation_cleanup = Instant::now();
        }
    }

    /// Verifica se un check utente è in corso
    pub fn is_checking_user(&self) -> bool {
        self.pending_user_check.is_some()
    }

    /// Callback quando riceviamo la risposta del check utente
    pub fn handle_user_check_result(
        &mut self,
        username: String,
        exists: bool,
        _user_id: Option<Uuid>,
        request_id: String,
    ) {
        // Verifica che sia la risposta che aspettavamo
        if self.user_check_request_id.as_ref() != Some(&request_id) {
            warn!(
            "Received stale user check response for request_id: {}",
            request_id
        );
            return;
        }

        // Pulisci stato pending
        self.pending_user_check = None;
        self.user_check_request_id = None;
        self.user_check_timestamp = None;

        // ========================================================================
        // GESTIONE VERIFICA UTENTE PER POPUP CREAZIONE GRUPPO
        // ========================================================================
        if let Some(pending) = &self.create_group_popup.pending_user_verification {
            if pending.to_lowercase() == username.to_lowercase() {
                if exists {
                    //  Utente esiste! Aggiungilo alla lista partecipanti
                    self.create_group_popup
                        .selected_participants
                        .insert(username.clone());
                    info!("User '{}' verified and added to group creation", username);
                } else {
                    //  Utente non esiste, mostra errore
                    let _ = self.ui_tx.send(UiEvent::Error(ErrorType::Generic(
                        format!("Utente '{}' non trovato", username),
                    )));
                    info!("User '{}' not found for group creation", username);
                }
                // Reset dello stato di verifica del popup
                self.create_group_popup.pending_user_verification = None;
                return; // Non continuare con la gestione DM
            }
        }

        // ========================================================================
        // GESTIONE VERIFICA UTENTE PER POPUP INVITO MEMBRI
        // ========================================================================
        if let Some(pending) = &self.invite_popup.pending_user_verification {
            if pending.to_lowercase() == username.to_lowercase() {
                if exists {
                    //  Utente esiste! Aggiungilo alla lista utenti da invitare
                    self.invite_popup.selected_users.insert(username.clone());
                    info!("User '{}' verified and added to invite list", username);
                } else {
                    //  Utente non esiste, mostra errore
                    let _ = self.ui_tx.send(UiEvent::Error(ErrorType::Generic(
                        format!("Utente '{}' non trovato", username),
                    )));
                    info!("User '{}' not found for invite", username);
                }
                // Reset dello stato di verifica del popup
                self.invite_popup.pending_user_verification = None;
                return; // Non continuare con la gestione DM
            }
        }

        // ========================================================================
        // GESTIONE VERIFICA UTENTE PER CREAZIONE DM
        // ========================================================================
        if exists {
            info!("User '{}' verified for DM creation", username);

            //Genera un cid temporaneo (non salvato)
            let temp_cid = Uuid::new_v4();

            // Apri la chat con il cid temporaneo
            self.cid = Some(temp_cid);
            self.page = crate::models::Page::Chat;
            self.conv_title = username.clone();
            self.messages.clear();

            info!("Opened DM UI with temporary cid {} for user {}", temp_cid, username);
        } else {
            // Utente non trovato
            let _ = self.ui_tx.send(UiEvent::Error(ErrorType::Generic(
                format!("Utente '{}' non esiste", username),
            )));
        }
    }

    // ========================================
    // GESTIONE STUB (DM e GRUPPI)
    // ========================================

    /// Aggiunge un DM stub alla tracking map
    pub fn add_dm_stub(&mut self, stub_id: Uuid, target_username: String) {
        self.dm_stubs
            .insert(stub_id, (target_username.clone(), Instant::now()));
        debug!("Added DM stub: {} -> {}", stub_id, target_username);
    }

   

    /// Rimuove un DM stub dalla tracking map
    pub fn remove_dm_stub(&mut self, conversation_id: Uuid) {
        if let Some((target, _)) = self.dm_stubs.remove(&conversation_id) {
            debug!("Removed DM stub: {} -> {}", conversation_id, target);
        }
    }

    /// Verifica se un ID è uno stub DM
    pub fn is_dm_stub(&self, conversation_id: Uuid) -> bool {
        self.dm_stubs.contains_key(&conversation_id)
    }

    /// Verifica se un ID è uno stub di gruppo
    pub fn is_group_stub(&self, conversation_id: Uuid) -> bool {
        self.group_stubs.contains_key(&conversation_id)
    }

    // ========================================
    // CLEANUP METHODS
    // ========================================

    pub fn cleanup_old_data(&self) {
        let total_messages = self.get_total_cached_messages();
        if total_messages > 50000 {
            warn!(
                "High memory usage detected: {} cached messages",
                total_messages
            );
        }
    }

    pub fn cleanup_expired_stubs(&mut self) {
        let now = Instant::now();

        // === Cleanup group stubs scaduti ===
        let expired_groups: Vec<Uuid> = self
            .group_stubs
            .iter()
            .filter(|(_, (_, created))| now.duration_since(*created) > STUB_TIMEOUT)
            .map(|(id, _)| *id)
            .collect();

        for stub_id in expired_groups {
            if let Some((name, _)) = self.group_stubs.remove(&stub_id) {
                warn!("Removing expired group stub: {} ({})", name, stub_id);

                // Rimuovi dalla lista conversazioni
                if let Some(ref mut convs) = self.conversations {
                    convs.retain(|c| c.id != stub_id);
                }

                // Rimuovi messaggi cached
                self.conversation_messages.remove(&stub_id);

                // Se era la conversazione attiva, torna alla lista
                if self.cid == Some(stub_id) {
                    self.cid = None;
                    self.page = Page::Conversations;
                    self.messages.clear();
                    self.conv_title.clear();
                }

                // Notifica l'utente
                let _ = self.ui_tx.send(UiEvent::Error(ErrorType::GroupCreate));
            }
        }

        // === Cleanup DM stubs scaduti ===
        let expired_dms: Vec<Uuid> = self
            .dm_stubs
            .iter()
            .filter(|(_, (_, created))| now.duration_since(*created) > STUB_TIMEOUT)
            .map(|(id, _)| *id)
            .collect();

        for stub_id in expired_dms {
            if let Some((username, _)) = self.dm_stubs.remove(&stub_id) {
                warn!("Removing expired DM stub: {} ({})", username, stub_id);

                // Rimuovi dalla lista conversazioni
                if let Some(ref mut convs) = self.conversations {
                    convs.retain(|c| c.id != stub_id);
                }

                // Rimuovi messaggi cached
                self.conversation_messages.remove(&stub_id);

                // Se era la conversazione attiva, torna alla lista
                if self.cid == Some(stub_id) {
                    self.cid = None;
                    self.page = Page::Conversations;
                    self.messages.clear();
                    self.conv_title.clear();
                }

                // Notifica l'utente
                let _ = self.ui_tx.send(UiEvent::Error(ErrorType::MessageSend));
            }
        }

    }

    /// Pulisce conferme messaggi scadute
    pub fn cleanup_pending_confirmations(&mut self) {
        let now = chrono::Utc::now().timestamp();
        let timeout_secs = self.confirmation_timeout.as_secs() as i64;

        let mut expired = Vec::new();
        for (client_id, msg) in &self.pending_confirmations {
            if now - msg.created_at > timeout_secs {
                expired.push(client_id.clone());
            }
        }

        for client_id in expired {
            if let Some(msg) = self.pending_confirmations.remove(&client_id) {
                warn!("Message confirmation timeout for {}", client_id);

                // Marca come fallito il messaggio nella UI
                let _ = self.ui_tx.send(UiEvent::MessageSendFailed(msg.id));

                // Aggiorna il messaggio nella cache
                if let Some(messages) = self.conversation_messages.get_mut(&msg.conversation_id) {
                    for m in messages.iter_mut() {
                        if m.id == msg.id {
                            m.is_confirmed = Some(false);
                            break;
                        }
                    }
                }

                // Aggiorna nella lista messaggi corrente
                for m in self.messages.iter_mut() {
                    if m.id == msg.id {
                        m.is_confirmed = Some(false);
                        break;
                    }
                }

                info!("Marked message {} as failed after timeout", msg.id);
            }
        }
    }

    /// Pulisce verifiche utente in timeout
    pub fn cleanup_pending_user_check(&mut self) {
        if let Some(timestamp) = self.user_check_timestamp {
            if timestamp.elapsed() > USER_CHECK_TIMEOUT {
                warn!(
                    "User check timeout for username: {:?}",
                    self.pending_user_check
                );

                // Mostra errore
                let _ = self.ui_tx.send(UiEvent::Error(ErrorType::Generic(
                    "Timeout verifica utente. Riprova.".to_string(),
                )));

                // Pulisci stato
                self.pending_user_check = None;
                self.user_check_request_id = None;
                self.user_check_timestamp = None;
            }
        }
    }
}
