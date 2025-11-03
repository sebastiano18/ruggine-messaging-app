use crate::models::*;
use crate::state::core::AppState;
use tracing::{debug, info, warn};
use uuid::Uuid;

pub struct SequenceHandler;

impl SequenceHandler {

    /// Metodo centralizzato per aggiornare la user sequence
    /// Commenta le righe interne per forzare i resume durante il testing
    pub fn update_user_sequence(state: &mut AppState, sequence: u64) {
        if sequence > 0 {
            state.user_sequence_confirmed = sequence;
            debug!("Updated user_sequence_confirmed to {}", sequence);
        }
    }

    /// Metodo centralizzato per aggiornare le conversation sequences
    /// Commenta le righe interne per forzare i resume durante il testing
    pub fn update_conversation_sequence(state: &mut AppState, conversation_id: Uuid, sequence: u64) {
        state.conversation_sequences.insert(conversation_id, sequence);
        state.conversation_sequences_confirmed.insert(conversation_id, sequence);
        debug!(
            "Updated conversation {} sequences to {}",
            conversation_id, sequence
        );
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
                state.request_user_events_resume(gap.client_seq);
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
}