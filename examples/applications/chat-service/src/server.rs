use quiccrunch_core::{QuicServer, MessageHandler, Result};
use quiccrunch_chat_service::{ChatRequest, ChatResponse, ChatMessage, ChatRoom};
use bytes::Bytes;
use clap::Parser;
use tracing::{info, error, warn};
use dashmap::DashMap;
use chrono::Utc;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "chat-server")]
#[command(about = "A multi-user chat service using QuicCrunch RPC")]
struct Args {
    #[arg(short, long, default_value = "127.0.0.1:50051")]
    address: String,
    
    #[arg(short, long)]
    verbose: bool,
    
    #[arg(long, default_value = "10")]
    max_rooms: usize,
    
    #[arg(long, default_value = "100")]
    max_users_per_room: usize,
}

#[derive(Clone)]
struct ChatHandler {
    rooms: Arc<DashMap<String, ChatRoom>>,
    max_rooms: usize,
    max_users_per_room: usize,
}

impl ChatHandler {
    fn new(max_rooms: usize, max_users_per_room: usize) -> Self {
        Self {
            rooms: Arc::new(DashMap::new()),
            max_rooms,
            max_users_per_room,
        }
    }
    
    fn handle_join(&self, user: String, room: String) -> ChatResponse {
        if self.rooms.len() >= self.max_rooms && !self.rooms.contains_key(&room) {
            return ChatResponse::Error {
                error: format!("Maximum number of rooms ({}) reached", self.max_rooms),
            };
        }
        
        let mut room_entry = self.rooms.entry(room.clone()).or_insert_with(|| {
            info!("Created new room: {}", room);
            ChatRoom::new(room.clone())
        });
        
        if room_entry.users.len() >= self.max_users_per_room {
            return ChatResponse::Error {
                error: format!("Room is full (max {} users)", self.max_users_per_room),
            };
        }
        
        if room_entry.add_user(user.clone()) {
            info!("User {} joined room {}", user, room);
            ChatResponse::Success {
                message: format!("Successfully joined room '{}'", room),
            }
        } else {
            ChatResponse::Error {
                error: "User already in room".to_string(),
            }
        }
    }
    
    fn handle_leave(&self, user: String, room: String) -> ChatResponse {
        if let Some(mut room_entry) = self.rooms.get_mut(&room) {
            if room_entry.remove_user(&user) {
                info!("User {} left room {}", user, room);
                
                // Remove empty rooms
                if room_entry.users.is_empty() {
                    drop(room_entry);
                    self.rooms.remove(&room);
                    info!("Removed empty room: {}", room);
                }
                
                ChatResponse::Success {
                    message: format!("Left room '{}'", room),
                }
            } else {
                ChatResponse::Error {
                    error: "User not in room".to_string(),
                }
            }
        } else {
            ChatResponse::Error {
                error: "Room not found".to_string(),
            }
        }
    }
    
    fn handle_send_message(&self, user: String, room: String, content: String) -> ChatResponse {
        if let Some(mut room_entry) = self.rooms.get_mut(&room) {
            if !room_entry.users.contains(&user) {
                return ChatResponse::Error {
                    error: "User not in room".to_string(),
                };
            }
            
            let message = ChatMessage {
                id: uuid::Uuid::new_v4().to_string(),
                user: user.clone(),
                content,
                timestamp: Utc::now(),
                room: room.clone(),
            };
            
            room_entry.add_message(message.clone());
            info!("Message sent by {} in room {}", user, room);
            
            ChatResponse::MessageReceived { message }
        } else {
            ChatResponse::Error {
                error: "Room not found".to_string(),
            }
        }
    }
    
    fn handle_get_history(&self, room: String, limit: Option<usize>) -> ChatResponse {
        if let Some(room_entry) = self.rooms.get(&room) {
            let messages = room_entry.get_recent_messages(limit);
            ChatResponse::History { messages }
        } else {
            ChatResponse::Error {
                error: "Room not found".to_string(),
            }
        }
    }
    
    fn handle_list_rooms(&self) -> ChatResponse {
        let rooms: Vec<String> = self.rooms.iter().map(|entry| entry.key().clone()).collect();
        ChatResponse::RoomList { rooms }
    }
    
    fn handle_list_users(&self, room: String) -> ChatResponse {
        if let Some(room_entry) = self.rooms.get(&room) {
            let users = room_entry.users.clone();
            ChatResponse::UserList { users }
        } else {
            ChatResponse::Error {
                error: "Room not found".to_string(),
            }
        }
    }
}

impl MessageHandler for ChatHandler {
    type Response = Bytes;
    
    async fn handle(&self, message: Bytes) -> Bytes {
        match serde_json::from_slice::<ChatRequest>(&message) {
            Ok(request) => {
                let response = match request {
                    ChatRequest::Join { user, room } => self.handle_join(user, room),
                    ChatRequest::Leave { user, room } => self.handle_leave(user, room),
                    ChatRequest::SendMessage { user, room, content } => {
                        self.handle_send_message(user, room, content)
                    }
                    ChatRequest::GetHistory { room, limit } => self.handle_get_history(room, limit),
                    ChatRequest::ListRooms => self.handle_list_rooms(),
                    ChatRequest::ListUsers { room } => self.handle_list_users(room),
                };
                
                match serde_json::to_vec(&response) {
                    Ok(bytes) => bytes.into(),
                    Err(e) => {
                        error!("Failed to serialize response: {}", e);
                        format!("{{\"error\":\"Serialization error: {}\"}}", e).into()
                    }
                }
            }
            Err(e) => {
                error!("Failed to parse request: {}", e);
                format!("{{\"error\":\"Invalid request format: {}\"}}", e).into()
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    // Initialize tracing
    if args.verbose {
        tracing_subscriber::fmt()
            .with_env_filter("info")
            .init();
    }
    
    info!("Starting chat server on {}", args.address);
    info!("Max rooms: {}, Max users per room: {}", args.max_rooms, args.max_users_per_room);
    
    let server = QuicServer::new()?;
    let mut listener = server.listen(&args.address).await?;
    
    info!("Chat server listening on {}", listener.local_addr()?);
    println!("Chat server ready! Listening on {}", listener.local_addr()?);
    
    let handler = ChatHandler::new(args.max_rooms, args.max_users_per_room);
    
    while let Ok(connection) = listener.accept().await {
        info!("New connection accepted");
        let handler = handler.clone();
        
        tokio::spawn(async move {
            while let Ok(mut stream) = connection.accept_stream().await {
                let handler = handler.clone();
                tokio::spawn(async move {
                    while let Ok(message) = stream.receive_message().await {
                        let response = handler.handle(message).await;
                        if let Err(e) = stream.send_message(response).await {
                            error!("Failed to send response: {}", e);
                            break;
                        }
                    }
                });
            }
        });
    }
    
    Ok(())
}