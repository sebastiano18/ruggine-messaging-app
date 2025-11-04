use super::{
    auth_handler::AuthHandler,
    conversation_handler::ConversationHandler,
    message_handler::MessageHandler,
    sequence_handler::SequenceHandler,
    user_notification_handler::UserNotificationHandler,
    utils,
    websocket_handler::WebSocketHandler,
};

use crate::models::*;
use crate::state::core::AppState;
use tracing::{debug, error, info};

pub struct EventDispatcher;

impl EventDispatcher {
    pub fn handle_event(state: &mut AppState, event: UiEvent) {
        match event {
            UiEvent::LoginStarted => {
                AuthHandler::handle_login_started(state);
            }

            UiEvent::RegisterStarted => {
                AuthHandler::handle_register_started(state);
            }

            UiEvent::Logged(token, user_id, last_sequence) => {
                AuthHandler::handle_logged(state, token, user_id, last_sequence);
            }

            UiEvent::LoggedOut => {
                AuthHandler::handle_logged_out(state);
            }

            UiEvent::DeleteAccountStart => {
                AuthHandler::handle_delete_account_start(state);
            }

            UiEvent::DeleteAccountCancel => {
                AuthHandler::handle_delete_account_cancel(state);
            }

            UiEvent::DeleteAccountConfirm => {
                AuthHandler::handle_delete_account_confirm(state);
            }

            UiEvent::WsControlReady(_)
            | UiEvent::WsConnected
            | UiEvent::WsDisconnected
            | UiEvent::WsError(_)
            | UiEvent::WsIncoming(_) => {
                if let UiEvent::WsIncoming(ref msg) = event {
                    let conv_id = msg.conversation_id;
                    WebSocketHandler::handle(state, event);
                    utils::move_conversation_to_top(state, conv_id);
                } else {
                    WebSocketHandler::handle(state, event);
                }
            }

            UiEvent::PongReceived {
                current_user_sequence,
                gaps_detected,
                user_events_gap,
            } => {
                SequenceHandler::handle_pong(
                    state,
                    current_user_sequence,
                    gaps_detected,
                    user_events_gap,
                );
            }

            UiEvent::UserEventsResume { events } => {
                SequenceHandler::handle_user_events_resume(state, events);
            }

            UiEvent::MessagesResume {
                conversation_id,
                messages,
            } => {
                SequenceHandler::handle_messages_resume(state, conversation_id, messages);
            }

            UiEvent::InitialStateReceived {
                conversations,
                user_sequence,
            } => {
                ConversationHandler::handle_initial_state_received(
                    state,
                    conversations,
                    user_sequence,
                );
            }

            UiEvent::LastMessageUpdate {
                conversation_id,
                message,
            } => {
                ConversationHandler::handle_last_message_update(state, conversation_id, message);
            }

            UiEvent::ConversationMessagesReceived {
                conversation_id,
                messages,
                has_more,
            } => {
                ConversationHandler::handle_conversation_messages_received(
                    state,
                    conversation_id,
                    messages,
                    has_more,
                );
            }

            UiEvent::ConversationConfirmed {
                conversation,
                messages,
                client_temp_id,
            } => {
                ConversationHandler::handle_conversation_confirmed(
                    state,
                    conversation,
                    messages,
                    client_temp_id,
                );
            }

            UiEvent::OlderMessagesLoaded(new_messages) => {
                ConversationHandler::handle_older_messages_loaded(state, new_messages);
            }

            UiEvent::LoadingError => {
                ConversationHandler::handle_loading_error(state);
            }

            UiEvent::Opened(cid) => {
                ConversationHandler::handle_opened(state, cid);
            }

            UiEvent::ConversationDeleted(cid) => {
                ConversationHandler::handle_conversation_deleted(state, cid);
            }

            UiEvent::ConversationCreated(cid) => {
                ConversationHandler::handle_conversation_created(state, cid);
            }

            UiEvent::DmStubCreated(stub_id, target_username) => {
                ConversationHandler::handle_dm_stub_created(state, stub_id, target_username);
            }

            UiEvent::ConversationsLoaded(conversations) => {
                ConversationHandler::handle_conversations_loaded(state, conversations);
            }

            UiEvent::AllMessagesLoaded(all_messages) => {
                ConversationHandler::handle_all_messages_loaded(state, all_messages);
            }

            UiEvent::RefreshedMsgs(messages) => {
                ConversationHandler::handle_refreshed_msgs(state, messages);
            }

            UiEvent::SingleConversationLoaded(conv) => {
                ConversationHandler::handle_single_conversation_loaded(state, conv);
            }

            UiEvent::InitialLoadComplete => {
                ConversationHandler::handle_initial_load_complete(state);
            }

            UiEvent::LoadingProgress(msg) => {
                ConversationHandler::handle_loading_progress(msg);
            }

            UiEvent::MessageSendFailed(msg_id) => {
                MessageHandler::handle_message_send_failed(state, msg_id);
            }

            UiEvent::MessageConfirmation {
                client_msg_id,
                server_msg_id,
                sequence,
                status,
            } => {
                MessageHandler::handle_message_confirmation(
                    state,
                    client_msg_id,
                    server_msg_id,
                    sequence,
                    status,
                );
            }

            UiEvent::UserNotification {
                sequence,
                event_type,
                event_data,
                conversation_id,
                recovery,
            } => {
                UserNotificationHandler::handle_user_notification(
                    state,
                    sequence,
                    event_type,
                    event_data,
                    conversation_id,
                    recovery,
                );
            }

            UiEvent::Info(msg) => {
                info!("Info: {}", msg);
                state.set_ui_message(msg);
            }

            UiEvent::Error(msg) => {
                error!("Error: {}", msg);
                state.set_ui_message(msg);
            }

            UiEvent::InviteCreated(token) => {
                info!("Invite created: {}", token);
                state.last_created_invite = Some(token);
            }

            UiEvent::TriggerConversationFetch(cid, reason) => {
                ConversationHandler::handle_trigger_conversation_fetch(state, cid, reason);
            }

            UiEvent::ConversationCompleteFetched(conv, messages) => {
                ConversationHandler::handle_conversation_complete_fetched(state, conv, messages);
            }

            UiEvent::ConversationListUpdated => {
                ConversationHandler::handle_conversation_list_updated(state);
            }

            UiEvent::SendPing => {
                debug!("Manual ping requested");
                SequenceHandler::send_ping(state);
            }

            _ => {
                debug!("Unhandled event: {:?}", event);
            }
        }
    }
}