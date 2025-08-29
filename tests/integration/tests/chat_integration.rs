use quiccrunch_core::{QuicClient, Result};
use quiccrunch_chat_service::{ChatRequest, ChatResponse};
use std::env;

async fn get_chat_server() -> String {
    env::var("CHAT_SERVER").unwrap_or_else(|_| "127.0.0.1:50052".to_string())
}

#[tokio::test]
async fn test_chat_room_operations() -> Result<()> {
    let server = get_chat_server().await;
    let client = QuicClient::new()?;
    let connection = client.connect(&server).await?;
    
    // Test joining a room
    let request = ChatRequest::Join {
        user: "test_user".to_string(),
        room: "test_room".to_string(),
    };
    let request_bytes = serde_json::to_vec(&request)?;
    let response_bytes = connection.request(request_bytes).await?;
    let response: ChatResponse = serde_json::from_slice(&response_bytes)?;
    
    match response {
        ChatResponse::Success { .. } => {},
        _ => panic!("Expected success response for joining room"),
    }
    
    // Test listing rooms
    let request = ChatRequest::ListRooms;
    let request_bytes = serde_json::to_vec(&request)?;
    let response_bytes = connection.request(request_bytes).await?;
    let response: ChatResponse = serde_json::from_slice(&response_bytes)?;
    
    match response {
        ChatResponse::RoomList { rooms } => {
            assert!(rooms.contains(&"test_room".to_string()));
        },
        _ => panic!("Expected room list response"),
    }
    
    Ok(())
}

#[tokio::test]
async fn test_chat_messaging() -> Result<()> {
    let server = get_chat_server().await;
    let client = QuicClient::new()?;
    let connection = client.connect(&server).await?;
    
    // Join a room first
    let request = ChatRequest::Join {
        user: "test_user_2".to_string(),
        room: "test_room_2".to_string(),
    };
    let request_bytes = serde_json::to_vec(&request)?;
    let _ = connection.request(request_bytes).await?;
    
    // Send a message
    let request = ChatRequest::SendMessage {
        user: "test_user_2".to_string(),
        room: "test_room_2".to_string(),
        content: "Hello, World!".to_string(),
    };
    let request_bytes = serde_json::to_vec(&request)?;
    let response_bytes = connection.request(request_bytes).await?;
    let response: ChatResponse = serde_json::from_slice(&response_bytes)?;
    
    match response {
        ChatResponse::MessageReceived { message } => {
            assert_eq!(message.user, "test_user_2");
            assert_eq!(message.content, "Hello, World!");
            assert_eq!(message.room, "test_room_2");
        },
        _ => panic!("Expected message received response"),
    }
    
    Ok(())
}

#[tokio::test]
async fn test_chat_history() -> Result<()> {
    let server = get_chat_server().await;
    let client = QuicClient::new()?;
    let connection = client.connect(&server).await?;
    
    // Join room and send messages
    let request = ChatRequest::Join {
        user: "test_user_3".to_string(),
        room: "test_room_3".to_string(),
    };
    let request_bytes = serde_json::to_vec(&request)?;
    let _ = connection.request(request_bytes).await?;
    
    // Send multiple messages
    for i in 1..=5 {
        let request = ChatRequest::SendMessage {
            user: "test_user_3".to_string(),
            room: "test_room_3".to_string(),
            content: format!("Message {}", i),
        };
        let request_bytes = serde_json::to_vec(&request)?;
        let _ = connection.request(request_bytes).await?;
    }
    
    // Get history
    let request = ChatRequest::GetHistory {
        room: "test_room_3".to_string(),
        limit: Some(3),
    };
    let request_bytes = serde_json::to_vec(&request)?;
    let response_bytes = connection.request(request_bytes).await?;
    let response: ChatResponse = serde_json::from_slice(&response_bytes)?;
    
    match response {
        ChatResponse::History { messages } => {
            assert_eq!(messages.len(), 3);
            assert_eq!(messages[0].content, "Message 3");
            assert_eq!(messages[1].content, "Message 4");
            assert_eq!(messages[2].content, "Message 5");
        },
        _ => panic!("Expected history response"),
    }
    
    Ok(())
}

#[tokio::test]
async fn test_chat_user_management() -> Result<()> {
    let server = get_chat_server().await;
    let client1 = QuicClient::new()?;
    let client2 = QuicClient::new()?;
    let connection1 = client1.connect(&server).await?;
    let connection2 = client2.connect(&server).await?;
    
    // Both users join the same room
    let request1 = ChatRequest::Join {
        user: "user1".to_string(),
        room: "multi_user_room".to_string(),
    };
    let request2 = ChatRequest::Join {
        user: "user2".to_string(),
        room: "multi_user_room".to_string(),
    };
    
    let request_bytes1 = serde_json::to_vec(&request1)?;
    let request_bytes2 = serde_json::to_vec(&request2)?;
    
    let _ = connection1.request(request_bytes1).await?;
    let _ = connection2.request(request_bytes2).await?;
    
    // List users in the room
    let request = ChatRequest::ListUsers {
        room: "multi_user_room".to_string(),
    };
    let request_bytes = serde_json::to_vec(&request)?;
    let response_bytes = connection1.request(request_bytes).await?;
    let response: ChatResponse = serde_json::from_slice(&response_bytes)?;
    
    match response {
        ChatResponse::UserList { users } => {
            assert_eq!(users.len(), 2);
            assert!(users.contains(&"user1".to_string()));
            assert!(users.contains(&"user2".to_string()));
        },
        _ => panic!("Expected user list response"),
    }
    
    Ok(())
}