//! Handler modules organization and re-exports

// Module declarations
pub mod conversation;
pub mod message;
pub mod group;
pub mod user;
pub mod router;


// Re-export main handlers for convenience
pub use conversation::{handle_create_conversation, handle_delete_conversation};
pub use message::{handle_chat_message, handle_mark_read, handle_delete_message};
pub use group::{handle_create_group_with_participants, handle_invite_user, handle_leave_group};
pub use user::{handle_check_user, handle_user_events_resume_request};

// Re-export router
pub use router::handle_incoming_message;