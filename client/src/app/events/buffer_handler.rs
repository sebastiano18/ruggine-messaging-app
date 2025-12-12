use crate::models::*;
use crate::state::core::AppState;
use std::collections::BTreeMap;
use tracing::debug;
use uuid::Uuid;

pub struct BufferHandler;

impl BufferHandler {
    /// Tenta di consegnare messaggi bufferizzati in ordine sequenziale
    pub fn try_deliver_buffered_messages(
        state: &mut AppState,
        conversation_id: Uuid,
    ) -> Vec<MessageDto> {
        let mut messages_to_deliver = Vec::new();

        if let Some(buffer) = state.message_reorder_buffer.get_mut(&conversation_id) {
            let current_confirmed = state
                .conversation_sequences_confirmed
                .get(&conversation_id)
                .copied()
                .unwrap_or(0);

            let mut current_expected = current_confirmed + 1;
            let mut sequences_to_remove = Vec::new();

            // ✅ Delivera messaggi consecutivi
            while let Some(msg) = buffer.get(&current_expected) {
                messages_to_deliver.push(msg.clone());
                sequences_to_remove.push(current_expected);
                current_expected += 1;
            }

            // Rimuovi messaggi deliverizzati
            for seq in sequences_to_remove {
                buffer.remove(&seq);
            }

            // ✅ CLEANUP: Rimuovi messaggi obsoleti (seq ≤ confirmed)
            // Questi non verranno mai deliverizzati perché sono già confermati
            let obsolete_sequences: Vec<u64> = buffer
                .keys()
                .filter(|&&seq| seq <= current_confirmed)
                .copied()
                .collect();

            if !obsolete_sequences.is_empty() {
                debug!(
                    "Cleaning up {} obsolete messages (seq ≤ {}) from buffer for conversation {}",
                    obsolete_sequences.len(),
                    current_confirmed,
                    conversation_id
                );

                for seq in obsolete_sequences {
                    buffer.remove(&seq);
                }
            }

            // Rimuovi buffer vuoto
            if buffer.is_empty() {
                state.message_reorder_buffer.remove(&conversation_id);
            }
        }

        messages_to_deliver
    }

    /// Tenta di consegnare user events bufferizzati in ordine sequenziale
    pub fn try_deliver_buffered_user_events(state: &mut AppState) -> Vec<serde_json::Value> {
        let mut events_to_deliver = Vec::new();

        let current_confirmed = state.user_sequence_confirmed;
        let mut current_expected = current_confirmed + 1;
        let mut sequences_to_remove = Vec::new();

        // ✅ Delivera eventi consecutivi
        while let Some(events) = state.user_event_reorder_buffer.get(&current_expected) {
            events_to_deliver.extend(events.clone());
            sequences_to_remove.push(current_expected);
            current_expected += 1;
        }

        // Rimuovi eventi deliverizzati
        for seq in sequences_to_remove {
            state.user_event_reorder_buffer.remove(&seq);
        }

        // ✅ CLEANUP: Rimuovi eventi obsoleti (seq ≤ confirmed)
        // Questi non verranno mai deliverizzati perché sono già confermati
        let obsolete_sequences: Vec<u64> = state
            .user_event_reorder_buffer
            .keys()
            .filter(|&&seq| seq <= current_confirmed)
            .copied()
            .collect();

        if !obsolete_sequences.is_empty() {
            debug!(
                "Cleaning up {} obsolete user events (seq ≤ {}) from buffer",
                obsolete_sequences.len(),
                current_confirmed
            );

            for seq in obsolete_sequences {
                state.user_event_reorder_buffer.remove(&seq);
            }
        }

        events_to_deliver
    }

    /// Bufferizza un messaggio per riordino successivo
    pub fn buffer_message_for_reorder(state: &mut AppState, msg: MessageDto) {
        if let Some(seq) = msg.sequence_num {
            let expected = state
                .conversation_sequences_confirmed
                .get(&msg.conversation_id)
                .copied()
                .unwrap_or(0)
                + 1;

            if seq > expected {
                debug!(
                    "Buffering message seq {} for conversation {} (expected {})",
                    seq, msg.conversation_id, expected
                );

                state
                    .message_reorder_buffer
                    .entry(msg.conversation_id)
                    .or_insert_with(BTreeMap::new)
                    .insert(seq, msg);
            }
        }
    }

    /// Bufferizza un user event per riordino successivo
    pub fn buffer_user_event_for_reorder(state: &mut AppState, seq: u64, event: serde_json::Value) {
        let expected = state.user_sequence_confirmed + 1;

        if seq > expected {
            debug!("Buffering user event seq {} (expected {})", seq, expected);

            state
                .user_event_reorder_buffer
                .entry(seq)
                .or_insert_with(Vec::new)
                .push(event);
        }
    }

    /// Ottiene il prossimo messaggio bufferizzato con una specifica sequenza
    /// Ritorna Some(msg) se il messaggio esiste, None altrimenti
    pub fn get_next_buffered_message(
        state: &mut AppState,
        conversation_id: Uuid,
        expected_seq: u64,
    ) -> Option<MessageDto> {
        if let Some(buffer) = state.message_reorder_buffer.get_mut(&conversation_id) {
            if let Some(msg) = buffer.remove(&expected_seq) {
                debug!(
                    "Retrieved buffered message seq {} for conversation {}",
                    expected_seq, conversation_id
                );

                // Pulisci il buffer se vuoto
                if buffer.is_empty() {
                    state.message_reorder_buffer.remove(&conversation_id);
                }

                return Some(msg);
            }
        }
        None
    }
}