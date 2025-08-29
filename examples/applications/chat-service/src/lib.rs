use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatMessage {
    pub id: String,
    pub user: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub room: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ChatRequest {
    Join { user: String, room: String },
    Leave { user: String, room: String },
    SendMessage { user: String, room: String, content: String },
    GetHistory { room: String, limit: Option<usize> },
    ListRooms,
    ListUsers { room: String },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ChatResponse {
    Success { message: String },
    Error { error: String },
    MessageReceived { message: ChatMessage },
    History { messages: Vec<ChatMessage> },
    RoomList { rooms: Vec<String> },
    UserList { users: Vec<String> },
}

#[derive(Debug, Clone)]
pub struct ChatRoom {
    pub name: String,
    pub users: Vec<String>,
    pub messages: Vec<ChatMessage>,
    pub created_at: DateTime<Utc>,
}

impl ChatRoom {
    pub fn new(name: String) -> Self {
        Self {
            name,
            users: Vec::new(),
            messages: Vec::new(),
            created_at: Utc::now(),
        }
    }
    
    pub fn add_user(&mut self, user: String) -> bool {
        if !self.users.contains(&user) {
            self.users.push(user);
            true
        } else {
            false
        }
    }
    
    pub fn remove_user(&mut self, user: &str) -> bool {
        if let Some(pos) = self.users.iter().position(|u| u == user) {
            self.users.remove(pos);
            true
        } else {
            false
        }
    }
    
    pub fn add_message(&mut self, message: ChatMessage) {
        // Keep only last 100 messages per room
        if self.messages.len() >= 100 {
            self.messages.remove(0);
        }
        self.messages.push(message);
    }
    
    pub fn get_recent_messages(&self, limit: Option<usize>) -> Vec<ChatMessage> {
        let limit = limit.unwrap_or(50).min(self.messages.len());
        self.messages
            .iter()
            .rev()
            .take(limit)
            .rev()
            .cloned()
            .collect()
    }
}