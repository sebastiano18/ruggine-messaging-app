use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use sqlx::SqlitePool;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::{accept_async, tungstenite::Message as WsMessage};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::models::*;
use crate::websocket::WebSocketConnection;

pub struct ChatServer {
    db: SqlitePool,
    connections: Arc<DashMap<Uuid, WebSocketConnection>>,
    user_sessions: Arc<DashMap<String, Uuid>>, // token -> user_id
}

impl ChatServer {
    pub fn new(db: SqlitePool) -> Self {
        Self {
            db,
            connections: Arc::new(DashMap::new()),
            user_sessions: Arc::new(DashMap::new()),
        }
    }

    pub async fn run(&self, addr: SocketAddr) -> Result<()> {
        let listener = TcpListener::bind(addr).await?;
        info!("WebSocket server listening on: {}", addr);

        while let Ok((stream, addr)) = listener.accept().await {
            info!("New connection from: {}", addr);
            let server = self.clone();
            tokio::spawn(async move {
                if let Err(e) = server.handle_connection(stream, addr).await {
                    error!("Error handling connection {}: {}", addr, e);
                }
            });
        }

        Ok(())
    }

    async fn handle_connection(&self, stream: TcpStream, addr: SocketAddr) -> Result<()> {
        let ws_stream = accept_async(stream).await?;
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        let (tx, mut rx) = mpsc::unbounded_channel();

        let connection_id = Uuid::new_v4();
        let connection = WebSocketConnection::new(connection_id, tx);
        
        // Store connection
        self.connections.insert(connection_id, connection);

        // Handle outgoing messages
        let connections_clone = self.connections.clone();
        let outgoing_task = tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                if let Ok(text) = serde_json::to_string(&message) {
                    if ws_sender.send(WsMessage::Text(text)).await.is_err() {
                        break;
                    }
                }
            }
            // Remove connection when done
            connections_clone.remove(&connection_id);
        });

        // Handle incoming messages
        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(WsMessage::Text(text)) => {
                    if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                        self.handle_client_message(connection_id, client_msg).await;
                    } else {
                        warn!("Failed to parse message from {}: {}", addr, text);
                    }
                }
                Ok(WsMessage::Close(_)) => {
                    info!("Connection {} closed", addr);
                    break;
                }
                Err(e) => {
                    error!("WebSocket error for {}: {}", addr, e);
                    break;
                }
                _ => {}
            }
        }

        // Cleanup
        self.connections.remove(&connection_id);
        outgoing_task.abort();

        Ok(())
    }

    async fn handle_client_message(&self, connection_id: Uuid, message: ClientMessage) {
        info!("Received message from connection {}: {:?}", connection_id, message);
        
        let response = match message {
            ClientMessage::Register { username, email, password } => {
                info!("Processing registration for user: {}", username);
                self.handle_register(username, email, password).await
            }
            ClientMessage::Login { username, password } => {
                info!("Processing login for user: {}", username);
                self.handle_login(connection_id, username, password).await
            }
            ClientMessage::Logout => {
                info!("Processing logout for connection: {}", connection_id);
                self.handle_logout(connection_id).await
            }
            ClientMessage::CreateGroup { name, description, is_private } => {
                info!("Processing create group: {} (private: {})", name, is_private);
                self.handle_create_group(connection_id, name, description, is_private).await
            }
            ClientMessage::SendMessage { group_id, content } => {
                info!("Processing send message to group: {}", group_id);
                self.handle_send_message(connection_id, group_id, content).await
            }
            ClientMessage::GetMessages { group_id, limit, offset } => {
                info!("Processing get messages for group: {}", group_id);
                self.handle_get_messages(connection_id, group_id, limit, offset).await
            }
            ClientMessage::InviteToGroup { group_id, username } => {
                info!("Processing invite user {} to group: {}", username, group_id);
                self.handle_invite_to_group(connection_id, group_id, username).await
            }
            ClientMessage::JoinGroup { group_id } => {
                info!("Processing join group: {}", group_id);
                self.handle_join_group(connection_id, group_id).await
            }
            ClientMessage::AcceptInvite { invite_id } => {
                info!("Processing accept invite: {}", invite_id);
                self.handle_accept_invite(connection_id, invite_id).await
            }
            ClientMessage::DeclineInvite { invite_id } => {
                info!("Processing decline invite: {}", invite_id);
                self.handle_decline_invite(connection_id, invite_id).await
            }
            ClientMessage::GetUsers => {
                info!("Processing get users list");
                self.handle_get_users(connection_id).await
            }
            ClientMessage::ListGroups => {
                info!("Processing list groups for user");
                self.handle_list_groups(connection_id).await
            }
            ClientMessage::Ping => {
                info!("Processing ping from connection: {}", connection_id);
                ServerMessage::Pong
            }
            _ => {
                warn!("Unhandled message type from connection: {}", connection_id);
                ServerMessage::Error { error: "Not implemented yet".to_string() }
            }
        };

        info!("Sending response to connection {}: {:?}", connection_id, response);
        if let Some(connection) = self.connections.get(&connection_id) {
            if let Err(e) = connection.send(response).await {
                error!("Failed to send response to connection {}: {:?}", connection_id, e);
            }
        } else {
            error!("Connection {} not found when trying to send response", connection_id);
        }
    }

    async fn handle_register(&self, username: String, email: String, password: String) -> ServerMessage {
        match crate::auth::register_user(&self.db, username, email, password).await {
            Ok(user) => ServerMessage::RegisterSuccess { user },
            Err(e) => ServerMessage::RegisterFailed { error: e.to_string() },
        }
    }

    async fn handle_login(&self, connection_id: Uuid, username: String, password: String) -> ServerMessage {
        match crate::auth::authenticate_user(&self.db, username, password).await {
            Ok(user) => {
                let token = crate::auth::generate_session_token();
                self.user_sessions.insert(token.clone(), user.id);
                
                // Associate connection with user
                if let Some(mut connection) = self.connections.get_mut(&connection_id) {
                    connection.set_user_id(Some(user.id));
                }
                
                // Load user's groups after successful login
                tokio::spawn({
                    let db = self.db.clone();
                    let connections = self.connections.clone();
                    let user_id = user.id;
                    async move {
                        if let Ok(groups) = crate::database::get_user_groups(&db, user_id).await {
                            if let Some(connection) = connections.get(&connection_id) {
                                let _ = connection.send(ServerMessage::GroupsList { groups }).await;
                            }
                        }
                    }
                });

                // Send pending invites to user after successful login
                tokio::spawn({
                    let db = self.db.clone();
                    let connections = self.connections.clone();
                    let user_id = user.id;
                    async move {
                        if let Ok(invites) = crate::database::get_pending_invites_for_user(&db, user_id).await {
                            if !invites.is_empty() {
                                if let Some(connection) = connections.get(&connection_id) {
                                    let invites_count = invites.len();
                                    let _ = connection.send(ServerMessage::PendingInvites { invites }).await;
                                    println!("Sent {} pending invites to user {} on login", invites_count, user_id);
                                }
                            }
                        }
                    }
                });
                
                ServerMessage::LoginSuccess { user, token }
            }
            Err(e) => ServerMessage::LoginFailed { error: e.to_string() },
        }
    }

    async fn handle_logout(&self, connection_id: Uuid) -> ServerMessage {
        if let Some(connection) = self.connections.get(&connection_id) {
            if let Some(user_id) = connection.user_id {
                // Update user offline status
                let _ = crate::database::update_user_online_status(&self.db, user_id, false).await;
                
                // Remove from sessions
                self.user_sessions.retain(|_, &mut v| v != user_id);
            }
        }
        
        ServerMessage::Success { message: "Logged out successfully".to_string() }
    }

    async fn handle_create_group(
        &self,
        connection_id: Uuid,
        name: String,
        description: Option<String>,
        is_private: bool,
    ) -> ServerMessage {
        info!("Creating group '{}' for connection {}", name, connection_id);
        
        if let Some(connection) = self.connections.get(&connection_id) {
            if let Some(user_id) = connection.user_id {
                info!("User {} creating group '{}'", user_id, name);
                
                let group = Group {
                    id: Uuid::new_v4(),
                    name: name.clone(),
                    description,
                    created_by: user_id,
                    created_at: chrono::Utc::now(),
                    is_private,
                };

                info!("Attempting to save group to database: {:?}", group);
                match crate::database::create_group(&self.db, &group).await {
                    Ok(_) => {
                        info!("Group {} saved successfully", group.id);
                        
                        // Add creator as owner
                        let member = GroupMember {
                            group_id: group.id,
                            user_id,
                            role: MemberRole::Owner,
                            joined_at: chrono::Utc::now(),
                        };
                        
                        info!("Adding creator as group member: {:?}", member);
                        if let Err(e) = crate::database::add_group_member(&self.db, &member).await {
                            error!("Failed to add group creator as member: {}", e);
                            return ServerMessage::Error { error: format!("Failed to add creator as member: {}", e) };
                        }
                        
                        info!("Group '{}' created successfully", group.name);
                        ServerMessage::GroupCreated { group }
                    }
                    Err(e) => {
                        error!("Failed to create group '{}': {}", name, e);
                        ServerMessage::Error { error: e.to_string() }
                    }
                }
            } else {
                warn!("Connection {} has no associated user_id", connection_id);
                ServerMessage::NotAuthorized
            }
        } else {
            error!("Connection {} not found", connection_id);
            ServerMessage::Error { error: "Connection not found".to_string() }
        }
    }

    async fn handle_send_message(
        &self,
        connection_id: Uuid,
        group_id: Uuid,
        content: String,
    ) -> ServerMessage {
        if let Some(connection) = self.connections.get(&connection_id) {
            if let Some(user_id) = connection.user_id {
                let message = Message {
                    id: Uuid::new_v4(),
                    group_id,
                    sender_id: user_id,
                    content,
                    message_type: MessageType::Text,
                    sent_at: chrono::Utc::now(),
                    edited_at: None,
                    username: None, // Will be populated by database query
                };

                match crate::database::save_message(&self.db, &message).await {
                    Ok(_) => {
                        // Broadcast message to all users in the group
                        self.broadcast_to_group(group_id, ServerMessage::MessageReceived { 
                            message: message.clone() 
                        }).await;
                        
                        ServerMessage::MessageSent { message }
                    }
                    Err(e) => ServerMessage::Error { error: e.to_string() },
                }
            } else {
                ServerMessage::NotAuthorized
            }
        } else {
            ServerMessage::Error { error: "Connection not found".to_string() }
        }
    }

    async fn handle_get_messages(
        &self,
        _connection_id: Uuid,
        group_id: Uuid,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> ServerMessage {
        let limit = limit.unwrap_or(50);
        let offset = offset.unwrap_or(0);

        match crate::database::get_group_messages(&self.db, group_id, limit, offset).await {
            Ok(messages) => ServerMessage::MessagesHistory { messages },
            Err(e) => ServerMessage::Error { error: e.to_string() },
        }
    }

    async fn handle_invite_to_group(
        &self,
        connection_id: Uuid,
        group_id: Uuid,
        username: String,
    ) -> ServerMessage {
        info!("Processing invite to group: connection_id={}, group_id={}, target_username={}", connection_id, group_id, username);
        
        if let Some(connection) = self.connections.get(&connection_id) {
            if let Some(inviter_id) = connection.user_id {
                info!("Inviter user_id: {}", inviter_id);
                
                // Check if inviter is in the group
                match crate::database::is_user_in_group(&self.db, inviter_id, group_id).await {
                    Ok(true) => {
                        info!("Inviter is member of group, proceeding with invite");
                        
                        // Find the user to invite
                        match crate::database::get_user_by_username(&self.db, &username).await {
                            Ok(Some(user_to_invite)) => {
                                info!("Found user to invite: {} ({})", user_to_invite.username, user_to_invite.id);
                                
                                // Check if user is already in the group
                                match crate::database::is_user_in_group(&self.db, user_to_invite.id, group_id).await {
                                    Ok(false) => {
                                        info!("User is not in group, checking for existing invites");
                                        
                                        // Check if there's already a pending invite
                                        match crate::database::has_pending_invite(&self.db, group_id, user_to_invite.id).await {
                                            Ok(false) => {
                                                info!("No pending invite found, creating new invite");
                                                
                                                // Create an invite instead of adding directly
                                                let invite = GroupInvite {
                                                    id: Uuid::new_v4(),
                                                    group_id,
                                                    inviter_id,
                                                    invited_user_id: user_to_invite.id,
                                                    invited_at: chrono::Utc::now(),
                                                    status: InviteStatus::Pending,
                                                };
                                                
                                                info!("Created invite object: {:?}", invite);
                                                
                                                match crate::database::create_group_invite(&self.db, &invite).await {
                                                    Ok(_) => {
                                                        info!("Invite saved to database successfully");
                                                        
                                                        // Get group name for notification
                                                        let group_name = match crate::database::get_group_by_id(&self.db, group_id).await {
                                                            Ok(Some(group)) => {
                                                                info!("Found group name: {}", group.name);
                                                                group.name
                                                            },
                                                            _ => {
                                                                warn!("Could not find group name");
                                                                "Unknown Group".to_string()
                                                            },
                                                        };
                                                        
                                                        // Get inviter name
                                                        let inviter_name = match crate::database::get_user_by_id(&self.db, inviter_id).await {
                                                            Ok(Some(user)) => {
                                                                info!("Found inviter name: {}", user.username);
                                                                user.username
                                                            },
                                                            _ => {
                                                                warn!("Could not find inviter name");
                                                                "Unknown User".to_string()
                                                            },
                                                        };
                                                        
                                                        // Send invite notification to the invited user
                                                        info!("Sending invite notification to user {}", user_to_invite.id);
                                                        self.notify_user_invite_received(
                                                            user_to_invite.id, 
                                                            invite.clone(), 
                                                            group_name, 
                                                            inviter_name
                                                        ).await;
                                                        
                                                        info!("Invite process completed successfully");
                                                        ServerMessage::UserInvited { group_id, username }
                                                    }
                                                    Err(e) => {
                                                        error!("Failed to create invite: {}", e);
                                                        ServerMessage::Error { error: format!("Failed to send invite: {}", e) }
                                                    }
                                                }
                                            }
                                            Ok(true) => {
                                                warn!("User {} already has a pending invite for group {}", username, group_id);
                                                ServerMessage::Error { error: "User already has a pending invite for this group".to_string() }
                                            }
                                            Err(e) => {
                                                error!("Failed to check for existing invites: {}", e);
                                                ServerMessage::Error { error: e.to_string() }
                                            }
                                        }
                                    }
                                    Ok(true) => {
                                        warn!("User {} is already in group {}", username, group_id);
                                        ServerMessage::Error { error: "User is already in the group".to_string() }
                                    }
                                    Err(e) => {
                                        error!("Failed to check group membership: {}", e);
                                        ServerMessage::Error { error: e.to_string() }
                                    }
                                }
                            }
                            Ok(None) => {
                                warn!("User {} not found", username);
                                ServerMessage::Error { error: "User not found".to_string() }
                            }
                            Err(e) => {
                                error!("Failed to find user {}: {}", username, e);
                                ServerMessage::Error { error: e.to_string() }
                            }
                        }
                    }
                    Ok(false) => {
                        warn!("User {} not in group {} - cannot invite", inviter_id, group_id);
                        ServerMessage::NotAuthorized
                    }
                    Err(e) => {
                        error!("Failed to check group membership: {}", e);
                        ServerMessage::Error { error: e.to_string() }
                    }
                }
            } else {
                warn!("Connection {} has no associated user_id", connection_id);
                ServerMessage::NotAuthorized
            }
        } else {
            error!("Connection {} not found", connection_id);
            ServerMessage::Error { error: "Connection not found".to_string() }
        }
    }

    async fn handle_join_group(&self, connection_id: Uuid, group_id: Uuid) -> ServerMessage {
        if let Some(connection) = self.connections.get(&connection_id) {
            if let Some(user_id) = connection.user_id {
                match crate::database::get_group_by_id(&self.db, group_id).await {
                    Ok(Some(group)) => {
                        // Check if user is already in the group
                        match crate::database::is_user_in_group(&self.db, user_id, group_id).await {
                            Ok(false) => {
                                let member = GroupMember {
                                    group_id,
                                    user_id,
                                    role: MemberRole::Member,
                                    joined_at: chrono::Utc::now(),
                                };
                                
                                match crate::database::add_group_member(&self.db, &member).await {
                                    Ok(_) => ServerMessage::GroupJoined { group },
                                    Err(e) => ServerMessage::Error { error: e.to_string() },
                                }
                            }
                            Ok(true) => ServerMessage::Error { error: "Already in group".to_string() },
                            Err(e) => ServerMessage::Error { error: e.to_string() },
                        }
                    }
                    Ok(None) => ServerMessage::Error { error: "Group not found".to_string() },
                    Err(e) => ServerMessage::Error { error: e.to_string() },
                }
            } else {
                ServerMessage::NotAuthorized
            }
        } else {
            ServerMessage::Error { error: "Connection not found".to_string() }
        }
    }

    async fn handle_get_users(&self, connection_id: Uuid) -> ServerMessage {
        if let Some(connection) = self.connections.get(&connection_id) {
            if connection.user_id.is_some() {
                match crate::database::get_all_users(&self.db).await {
                    Ok(users) => {
                        // Don't include password hashes in the response
                        let safe_users: Vec<User> = users.into_iter().map(|mut user| {
                            user.password_hash = "".to_string();
                            user
                        }).collect();
                        ServerMessage::UsersList { users: safe_users }
                    }
                    Err(e) => ServerMessage::Error { error: e.to_string() },
                }
            } else {
                ServerMessage::NotAuthorized
            }
        } else {
            ServerMessage::Error { error: "Connection not found".to_string() }
        }
    }

    async fn handle_list_groups(&self, connection_id: Uuid) -> ServerMessage {
        if let Some(connection) = self.connections.get(&connection_id) {
            if let Some(user_id) = connection.user_id {
                match crate::database::get_user_groups(&self.db, user_id).await {
                    Ok(groups) => ServerMessage::GroupsList { groups },
                    Err(e) => ServerMessage::Error { error: e.to_string() },
                }
            } else {
                ServerMessage::NotAuthorized
            }
        } else {
            ServerMessage::Error { error: "Connection not found".to_string() }
        }
    }

    async fn notify_user_invited(&self, user_id: Uuid, group_id: Uuid, username: String) {
        // Find connections for the invited user
        for connection in self.connections.iter() {
            if connection.user_id == Some(user_id) {
                let _ = connection.send(ServerMessage::UserInvited { group_id, username: username.clone() }).await;
            }
        }
    }

    async fn broadcast_to_group(&self, _group_id: Uuid, message: ServerMessage) {
        // TODO: Implement group membership checking and broadcast only to group members
        // For now, we'll broadcast to all connections (simplified)
        for connection in self.connections.iter() {
            if connection.user_id.is_some() {
                let _ = connection.send(message.clone()).await;
            }
        }
    }
    
    async fn handle_accept_invite(&self, connection_id: Uuid, invite_id: Uuid) -> ServerMessage {
        if let Some(connection) = self.connections.get(&connection_id) {
            if let Some(user_id) = connection.user_id {
                match crate::database::get_invite_by_id(&self.db, invite_id).await {
                    Ok(Some(invite)) => {
                        if invite.invited_user_id == user_id && invite.status == InviteStatus::Pending {
                            // Update invite status to accepted
                            if let Err(e) = crate::database::update_invite_status(&self.db, invite_id, InviteStatus::Accepted).await {
                                error!("Failed to update invite status: {}", e);
                                return ServerMessage::Error { error: "Failed to accept invite".to_string() };
                            }
                            
                            // Add user to group
                            let member = GroupMember {
                                group_id: invite.group_id,
                                user_id,
                                role: MemberRole::Member,
                                joined_at: chrono::Utc::now(),
                            };
                            
                            match crate::database::add_group_member(&self.db, &member).await {
                                Ok(_) => {
                                    // Get user info for notifications
                                    let username = match crate::database::get_user_by_id(&self.db, user_id).await {
                                        Ok(Some(user)) => user.username,
                                        _ => "Unknown User".to_string(),
                                    };
                                    
                                    // Send a system message to the group
                                    let system_message = Message {
                                        id: Uuid::new_v4(),
                                        group_id: invite.group_id,
                                        sender_id: user_id,
                                        content: format!("{} joined the group", username),
                                        message_type: MessageType::UserJoined,
                                        sent_at: chrono::Utc::now(),
                                        edited_at: None,
                                        username: None,
                                    };
                                    
                                    if let Err(e) = crate::database::save_message(&self.db, &system_message).await {
                                        error!("Failed to save system message: {}", e);
                                    }
                                    
                                    // Broadcast the system message to group members
                                    self.broadcast_to_group(invite.group_id, ServerMessage::MessageReceived { 
                                        message: system_message 
                                    }).await;
                                    
                                    ServerMessage::InviteAccepted { group_id: invite.group_id, username }
                                }
                                Err(e) => {
                                    error!("Failed to add user to group: {}", e);
                                    ServerMessage::Error { error: "Failed to join group".to_string() }
                                }
                            }
                        } else {
                            ServerMessage::Error { error: "Invalid invite or already processed".to_string() }
                        }
                    }
                    Ok(None) => ServerMessage::Error { error: "Invite not found".to_string() },
                    Err(e) => {
                        error!("Failed to get invite: {}", e);
                        ServerMessage::Error { error: "Failed to process invite".to_string() }
                    }
                }
            } else {
                ServerMessage::NotAuthorized
            }
        } else {
            ServerMessage::NotAuthorized
        }
    }
    
    async fn handle_decline_invite(&self, connection_id: Uuid, invite_id: Uuid) -> ServerMessage {
        if let Some(connection) = self.connections.get(&connection_id) {
            if let Some(user_id) = connection.user_id {
                match crate::database::get_invite_by_id(&self.db, invite_id).await {
                    Ok(Some(invite)) => {
                        if invite.invited_user_id == user_id && invite.status == InviteStatus::Pending {
                            // Update invite status to declined
                            match crate::database::update_invite_status(&self.db, invite_id, InviteStatus::Declined).await {
                                Ok(_) => {
                                    let username = match crate::database::get_user_by_id(&self.db, user_id).await {
                                        Ok(Some(user)) => user.username,
                                        _ => "Unknown User".to_string(),
                                    };
                                    
                                    ServerMessage::InviteDeclined { group_id: invite.group_id, username }
                                }
                                Err(e) => {
                                    error!("Failed to update invite status: {}", e);
                                    ServerMessage::Error { error: "Failed to decline invite".to_string() }
                                }
                            }
                        } else {
                            ServerMessage::Error { error: "Invalid invite or already processed".to_string() }
                        }
                    }
                    Ok(None) => ServerMessage::Error { error: "Invite not found".to_string() },
                    Err(e) => {
                        error!("Failed to get invite: {}", e);
                        ServerMessage::Error { error: "Failed to process invite".to_string() }
                    }
                }
            } else {
                ServerMessage::NotAuthorized
            }
        } else {
            ServerMessage::NotAuthorized
        }
    }

    async fn notify_user_invite_received(&self, user_id: Uuid, invite: GroupInvite, group_name: String, inviter_name: String) {
        info!("Notifying user {} about invite to group '{}'", user_id, group_name);
        
        let message = ServerMessage::InviteReceived { 
            invite: invite.clone(), 
            group_name: group_name.clone(), 
            inviter_name: inviter_name.clone() 
        };
        
        info!("Created invite message: {:?}", message);
        
        let mut connections_found = 0;
        
        // Find all connections for this user
        for connection in self.connections.iter() {
            if let Some(conn_user_id) = connection.user_id {
                if conn_user_id == user_id {
                    connections_found += 1;
                    info!("Found connection {} for user {}, sending invite notification", connection.id, user_id);
                    
                    if let Err(e) = connection.send(message.clone()).await {
                        error!("Failed to send invite notification to connection {}: {}", connection.id, e);
                    } else {
                        info!("Successfully sent invite notification to connection {}", connection.id);
                    }
                }
            }
        }
        
        if connections_found == 0 {
            warn!("No active connections found for user {}", user_id);
        } else {
            info!("Sent invite notification to {} connections for user {}", connections_found, user_id);
        }
    }
}

impl Clone for ChatServer {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            connections: self.connections.clone(),
            user_sessions: self.user_sessions.clone(),
        }
    }
}
