use tokio::sync::mpsc;
use uuid::Uuid;
use crate::models::ServerMessage;

#[derive(Debug)]
pub struct WebSocketConnection {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    sender: mpsc::UnboundedSender<ServerMessage>,
}

impl WebSocketConnection {
    pub fn new(id: Uuid, sender: mpsc::UnboundedSender<ServerMessage>) -> Self {
        Self {
            id,
            user_id: None,
            sender,
        }
    }

    pub fn set_user_id(&mut self, user_id: Option<Uuid>) {
        self.user_id = user_id;
    }

    pub async fn send(&self, message: ServerMessage) -> Result<(), mpsc::error::SendError<ServerMessage>> {
        self.sender.send(message)
    }
}
