use crate::models::*;
use crate::state::core::AppState;
use tracing::{debug, info, warn};
use uuid::Uuid;

pub struct SequenceHandler;

impl SequenceHandler {

    /// Setta la sequenza iniziale (quando si riceve lo stato dal server)
    pub fn set_initial_user_sequence(state: &mut AppState, sequence: u64) {
        state.user_sequence_confirmed = sequence;
        state.user_sequence_received = sequence;
        state.user_sequence_shared.store(sequence, std::sync::atomic::Ordering::SeqCst);
        info!("Initial user sequence set to {}", sequence);
    }

    /// Metodo centralizzato per aggiornare la user sequence con gap detection
    /// Commenta le righe interne per forzare i resume durante il testing
    pub fn update_user_sequence(state: &mut AppState, sequence: u64) {
        let current = state.user_sequence_received;

        // Gap detection
        if sequence > current + 1 {
            let gap_size = sequence - current - 1;
            warn!(
                "User events gap detected! Expected {}, got {} (missing {} events)",
                current + 1,
                sequence,
                gap_size
            );
            state.sequence_stats.gaps_detected += 1;
            state.sequence_stats.last_gap_time = Some(std::time::Instant::now());

            // Trigger resume per gap grandi
            if gap_size >= 3 {
                warn!(
                    "Large user events gap ({}), requesting immediate resume",
                    gap_size
                );
                Self::request_user_events_resume(state, current);
            } else {
                debug!(
                    "Small user events gap ({}), will handle at next ping",
                    gap_size
                );
            }
        }

        // Aggiorna sequence_received
        if sequence > state.user_sequence_received {
            state.user_sequence_received = sequence;
            state.sequence_stats.total_events_received += 1;
        }

        // Aggiorna sequence_confirmed se continua
        if sequence == state.user_sequence_confirmed + 1 {
            state.user_sequence_confirmed = sequence;
            // Aggiorna anche l'Arc per il ping task indipendente
            state.user_sequence_shared.store(sequence, std::sync::atomic::Ordering::SeqCst);
            debug!("User sequence {} confirmed (continuous)", sequence);
        }
    }

    /// Metodo centralizzato per aggiornare le conversation sequences con gap detection
    /// Commenta le righe interne per forzare i resume durante il testing
    pub fn update_conversation_sequence(state: &mut AppState, conversation_id: Uuid, sequence: u64) {
        let current = state
            .conversation_sequences
            .get(&conversation_id)
            .copied()
            .unwrap_or(0);

        // Gap detection
        if sequence > current + 1 {
            let gap_size = sequence - current - 1;
            warn!(
                "Messages gap in conversation {}! Expected {}, got {} (missing {} messages)",
                conversation_id,
                current + 1,
                sequence,
                gap_size
            );
            state.sequence_stats.gaps_detected += 1;
            state.sequence_stats.last_gap_time = Some(std::time::Instant::now());

            // Trigger resume per gap grandi
            if gap_size >= 5 {
                warn!(
                    "Large messages gap ({}) in conversation {}, requesting immediate resume",
                    gap_size, conversation_id
                );
                Self::request_messages_resume(state, conversation_id, current);
            } else {
                debug!(
                    "Small messages gap ({}) in conversation {}, will handle at next ping",
                    gap_size, conversation_id
                );
            }
        }

        // Aggiorna conversation_sequences
        if sequence > current {
            state.conversation_sequences.insert(conversation_id, sequence);
        }

        // Aggiorna conversation_sequences_confirmed se continua
        let confirmed = state
            .conversation_sequences_confirmed
            .get(&conversation_id)
            .copied()
            .unwrap_or(0);
        if sequence == confirmed + 1 {
            state
                .conversation_sequences_confirmed
                .insert(conversation_id, sequence);
            debug!(
                "Conversation {} sequence {} confirmed",
                conversation_id, sequence
            );
        }
    }



    pub fn handle_pong(
        state: &mut AppState,
        current_user_sequence: u64,
        gaps_detected: bool,
        user_events_gap: Option<GapInfo>,
    ) {
        debug!(
            "Pong - server_user_seq: {}, gaps_detected: {}",
            current_user_sequence, gaps_detected
        );

        state.sequence_stats.pong_count += 1;
        state.missed_pings = 0;

        if let Some(gap) = user_events_gap {
            if gap.detected {
                warn!(
                    "User events gap detected: {} events missing (client: {}, server: {})",
                    gap.gap_size, gap.client_seq, gap.server_seq
                );
                state.sequence_stats.gaps_detected += 1;
                Self::request_user_events_resume(state, gap.client_seq);
            }
        }

        if gaps_detected {
            info!("Pong reported gap, resume request sent");
        }
    }

    pub fn handle_user_events_resume(state: &mut AppState, events: Vec<UserEventData>) {
        info!("Processing {} resumed user events", events.len());

        state.is_recovering_user_events = false;
        state.pending_resume_requests = state.pending_resume_requests.saturating_sub(1);
        state.sequence_stats.events_recovered += events.len() as u32;

        for event in events {
            SequenceHandler::update_user_sequence(state, event.sequence);

            let _ = state.ui_tx.send(UiEvent::UserNotification {
                sequence: event.sequence,
                event_type: event.event_type,
                event_data: event.event_data,
                conversation_id: event.conversation_id,
                recovery: true,
            });
        }

        // ✅ UN SOLO mark_read alla fine per la conversazione corrente
        if let Some(current_cid) = state.cid {
            if let Some(seq) = state.conversation_sequences.get(&current_cid).copied() {
                let outgoing = Outgoing::MarkRead {
                    conversation_id: current_cid,
                    sequence_num: seq,
                };
                let _ = state.ui_to_net_tx.try_send(outgoing);
                info!(
                "Auto mark_read after resume for conversation {} up to seq {}",
                current_cid, seq
            );
            }
        }
    }

    pub fn handle_messages_resume(
        state: &mut AppState,
        conversation_id: Uuid,
        messages: Vec<MessageDto>,
    ) {
        info!(
            "Processing {} resumed messages for {}",
            messages.len(),
            conversation_id
        );

        info!("Current UI messages for conversation {}:", conversation_id);
        if state.cid == Some(conversation_id) {
            for (idx, msg) in state.messages.iter().enumerate() {
                info!(
                    "  UI[{}]: id={}, client_msg_id={:?}, content_preview={}...",
                    idx,
                    msg.id,
                    msg.client_msg_id,
                    msg.content.chars().take(20).collect::<String>()
                );
            }
        }

        info!("Current pending confirmations:");
        for (client_id, pending_msg) in &state.pending_confirmations {
            info!(
                "  Pending: client_id={}, msg_id={}, conv_id={}, content_preview={}...",
                client_id,
                pending_msg.id,
                pending_msg.conversation_id,
                pending_msg.content.chars().take(20).collect::<String>()
            );
        }

        info!("Incoming resume messages:");
        for (idx, msg) in messages.iter().enumerate() {
            info!(
                "  Resume[{}]: id={}, client_msg_id={:?}, content_preview={}...",
                idx,
                msg.id,
                msg.client_msg_id,
                msg.content.chars().take(20).collect::<String>()
            );
        }

        state.is_recovering_messages.insert(conversation_id, false);
        state.pending_resume_requests = state.pending_resume_requests.saturating_sub(1);

        if let Some(max_seq) = messages.iter().filter_map(|m| m.sequence_num).max() {
            SequenceHandler::update_conversation_sequence(state, conversation_id, max_seq);
        }

        let mut existing_ids = std::collections::HashSet::new();

        if let Some(cache) = state.conversation_messages.get(&conversation_id) {
            for msg in cache {
                existing_ids.insert(msg.id);
            }
        }

        if state.cid == Some(conversation_id) {
            for msg in &state.messages {
                existing_ids.insert(msg.id);
            }
        }

        let mut pending_to_remove = Vec::new();
        let mut optimistic_to_remove = Vec::new();
        let mut truly_new_messages = Vec::new();

        for new_msg in messages {
            info!(
                "Processing resume message id={}, client_msg_id={:?}",
                new_msg.id, new_msg.client_msg_id
            );

            if existing_ids.contains(&new_msg.id) {
                info!(
                    "  -> Message {} already exists by server ID, skipping",
                    new_msg.id
                );
                continue;
            }

            let mut should_add = true;

            if let Some(ref server_client_id) = new_msg.client_msg_id {
                info!(
                    "  -> Checking for pending with client_id: {}",
                    server_client_id
                );

                if let Some(pending_msg) = state.pending_confirmations.get(server_client_id) {
                    info!(
                        "    FOUND in pending! Pending msg_id={}, will replace with server msg",
                        pending_msg.id
                    );
                    pending_to_remove.push(server_client_id.clone());
                    optimistic_to_remove.push((pending_msg.id, server_client_id.clone()));
                } else {
                    info!("    NOT found in pending confirmations");

                    if state.cid == Some(conversation_id) {
                        for ui_msg in &state.messages {
                            if ui_msg.client_msg_id.as_ref() == Some(server_client_id) {
                                info!(
                                    "    FOUND in UI messages! UI msg_id={}, will replace",
                                    ui_msg.id
                                );
                                optimistic_to_remove.push((ui_msg.id, server_client_id.clone()));
                                should_add = true;
                                break;
                            }
                        }
                    }
                }
            } else {
                info!("  -> No client_msg_id, will add as new message");
            }

            if should_add {
                truly_new_messages.push(new_msg);
            }
        }

        for client_id in &pending_to_remove {
            if let Some(_removed) = state.pending_confirmations.remove(client_id) {
                info!("Removed pending confirmation for client_id={}", client_id);
            }
        }

        for (optimistic_id, client_id) in &optimistic_to_remove {
            info!(
                "Removing optimistic message id={} with client_id={}",
                optimistic_id, client_id
            );

            if state.cid == Some(conversation_id) {
                let before = state.messages.len();
                state.messages.retain(|m| {
                    !(m.id == *optimistic_id || m.client_msg_id.as_ref() == Some(client_id))
                });
                let after = state.messages.len();
                info!("  Removed {} messages from UI", before - after);
            }

            if let Some(cache) = state.conversation_messages.get_mut(&conversation_id) {
                let before = cache.len();
                cache.retain(|m| {
                    !(m.id == *optimistic_id || m.client_msg_id.as_ref() == Some(client_id))
                });
                let after = cache.len();
                info!("  Removed {} messages from cache", before - after);
            }
        }

        if truly_new_messages.is_empty() {
            info!("No new messages to add after deduplication");
            return;
        }

        info!("Adding {} truly new messages", truly_new_messages.len());

        if let Some(cache) = state.conversation_messages.get_mut(&conversation_id) {
            cache.extend(truly_new_messages.clone());
            cache.sort_by(|a, b| match (a.sequence_num, b.sequence_num) {
                (Some(seq_a), Some(seq_b)) => seq_a.cmp(&seq_b),
                _ => a.created_at.cmp(&b.created_at),
            });
        } else {
            state
                .conversation_messages
                .insert(conversation_id, truly_new_messages.clone());
        }

        if state.cid == Some(conversation_id) {
            state.messages.extend(truly_new_messages);
            state
                .messages
                .sort_by(|a, b| match (a.sequence_num, b.sequence_num) {
                    (Some(seq_a), Some(seq_b)) => seq_a.cmp(&seq_b),
                    _ => a.created_at.cmp(&b.created_at),
                });

            info!("Final UI message count: {}", state.messages.len());
        }

        if let Some(buffered) = state.message_reorder_buffer.remove(&conversation_id) {
            info!(
                "Cleared {} buffered messages for conversation {} after resume",
                buffered.len(),
                conversation_id
            );
        }

        if let Some(current_cid) = state.cid {
            if current_cid == conversation_id {
                if let Some(seq) = state.conversation_sequences.get(&conversation_id).copied() {
                    let outgoing = Outgoing::MarkRead {
                        conversation_id,
                        sequence_num: seq,
                    };
                    let _ = state.ui_to_net_tx.try_send(outgoing);
                    info!(
                        "Auto mark_read after resume for conversation {} up to seq {}",
                        conversation_id, seq
                    );
                }
            }
        }
    }

    /// Invia ping al server con la sequence corrente
    pub fn send_ping(state: &mut AppState) {
        let user_seq = Some(state.user_sequence_confirmed);
        debug!("Sending manual ping - user_seq: {:?}", user_seq);
        state.sequence_stats.ping_count += 1;

        // Use direct WebSocket channel (bypasses rate limiter, same as automatic pings)
        if let Some(ref ws_ctrl) = state.ws_ctrl {
            let json_msg = serde_json::json!({
                "type": "ping",
                "timestamp": chrono::Utc::now().timestamp(),
                "user_sequence": user_seq
            });

            if let Ok(msg_str) = serde_json::to_string(&json_msg) {
                match ws_ctrl.outgoing_tx.send(msg_str) {
                    Ok(_) => debug!("Manual ping sent directly to WebSocket"),
                    Err(e) => warn!("Failed to send manual ping: {}", e),
                }
            }
        } else {
            warn!("Cannot send manual ping: no WebSocket control available");
        }
    }

    /// Richiede resume degli user events dal server
    pub fn request_user_events_resume(state: &mut AppState, from_sequence: u64) {
        if state.is_recovering_user_events {
            debug!("User events resume already in progress");
            return;
        }

        state.is_recovering_user_events = true;
        state.pending_resume_requests += 1;

        let outgoing = Outgoing::RequestUserResume {
            from_sequence,
            limit: 100,
        };
        let _ = state.ui_to_net_tx.try_send(outgoing);

        info!(
            "Requested user events resume from sequence {}",
            from_sequence
        );
    }

    /// Richiede resume dei messaggi per una conversazione
    pub fn request_messages_resume(state: &mut AppState, conversation_id: Uuid, from_sequence: u64) {
        if *state
            .is_recovering_messages
            .get(&conversation_id)
            .unwrap_or(&false)
        {
            debug!(
                "Messages resume already in progress for {}",
                conversation_id
            );
            return;
        }

        state.is_recovering_messages.insert(conversation_id, true);
        state.pending_resume_requests += 1;

        let outgoing = Outgoing::RequestMessagesResume {
            conversation_id,
            from_sequence,
            limit: 100,
        };
        let _ = state.ui_to_net_tx.try_send(outgoing);

        info!(
            "Requested messages resume for {} from sequence {}",
            conversation_id, from_sequence
        );
    }


    /// Reset completo del sistema di sequenze
    pub fn reset_sequence_system(state: &mut AppState) {
        state.is_recovering_user_events = false;
        state.is_recovering_messages.clear();
        state.pending_resume_requests = 0;
        state.sequence_stats = Default::default();
        state.message_reorder_buffer.clear();
        state.user_event_reorder_buffer.clear();

        info!(
            "Sequence system reset - user_seq: {}, conv_seqs: {}",
            state.user_sequence_confirmed,
            state.conversation_sequences.len()
        );
    }

    /// Reset sequenze alla disconnessione WebSocket
    pub fn reset_sequence_on_disconnect(state: &mut AppState) {
        debug!(
            "WebSocket disconnected, preserving sequences - user: {}, conversations: {}",
            state.user_sequence_confirmed,
            state.conversation_sequences.len()
        );
        Self::reset_sequence_system(state);
    }

    /// Calcola lo stato di salute del sistema di sequenze
    pub fn get_sequence_health(state: &AppState) -> f64 {
        if state.sequence_stats.ping_count == 0 {
            return 1.0;
        }

        let pong_rate =
            state.sequence_stats.pong_count as f64 / state.sequence_stats.ping_count as f64;
        let gap_penalty = (state.sequence_stats.gaps_detected as f64 * 0.1).min(0.5);
        let missed_penalty = (state.missed_pings as f64 / state.max_missed_pings as f64) * 0.3;

        (pong_rate - gap_penalty - missed_penalty).max(0.0)
    }
}