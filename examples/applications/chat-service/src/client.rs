use quiccrunch_core::{QuicClient, Result};
use quiccrunch_chat_service::{ChatRequest, ChatResponse};
use clap::Parser;
use std::io::{self, Write};

#[derive(Parser)]
#[command(name = "chat-client")]
#[command(about = "A chat client using QuicCrunch RPC")]
struct Args {
    #[arg(short, long, default_value = "127.0.0.1:50051")]
    server: String,
    
    #[arg(short, long)]
    user: String,
    
    #[arg(short, long, default_value = "general")]
    room: String,
    
    #[arg(short, long)]
    interactive: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    if args.user.is_empty() {
        println!("Error: --user is required");
        return Ok(());
    }
    
    let client = QuicClient::new()?;
    let connection = client.connect(&args.server).await?;
    
    println!("Connected to chat server at {}", args.server);
    
    if args.interactive {
        run_interactive_chat(&connection, &args.user, &args.room).await?;
    } else {
        println!("Use --interactive flag to start interactive chat session");
        show_help();
    }
    
    Ok(())
}

async fn run_interactive_chat(
    connection: &quiccrunch_core::Connection,
    user: &str,
    default_room: &str,
) -> Result<()> {
    println!("Chat Client - Interactive Mode");
    println!("User: {}", user);
    println!("Default room: {}", default_room);
    println!();
    
    // Auto-join the default room
    if let Err(e) = send_request(connection, ChatRequest::Join {
        user: user.to_string(),
        room: default_room.to_string(),
    }).await {
        println!("Failed to join default room: {}", e);
    }
    
    show_help();
    
    let mut current_room = default_room.to_string();
    
    loop {
        print!("{}@{} > ", user, current_room);
        io::stdout().flush().unwrap();
        
        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            break;
        }
        
        let input = input.trim();
        if input.is_empty() {
            continue;
        }
        
        if input == "quit" || input == "exit" {
            // Leave current room before exiting
            let _ = send_request(connection, ChatRequest::Leave {
                user: user.to_string(),
                room: current_room.clone(),
            }).await;
            break;
        }
        
        let parts: Vec<&str> = input.splitn(2, ' ').collect();
        let command = parts[0];
        
        match command {
            "/join" if parts.len() == 2 => {
                let new_room = parts[1].to_string();
                match send_request(connection, ChatRequest::Join {
                    user: user.to_string(),
                    room: new_room.clone(),
                }).await {
                    Ok(_) => {
                        // Leave the previous room
                        let _ = send_request(connection, ChatRequest::Leave {
                            user: user.to_string(),
                            room: current_room,
                        }).await;
                        current_room = new_room;
                    }
                    Err(e) => println!("Failed to join room: {}", e),
                }
            }
            "/leave" => {
                match send_request(connection, ChatRequest::Leave {
                    user: user.to_string(),
                    room: current_room.clone(),
                }).await {
                    Ok(_) => {
                        println!("Left room '{}'", current_room);
                        current_room = "general".to_string();
                        // Auto-join general room
                        let _ = send_request(connection, ChatRequest::Join {
                            user: user.to_string(),
                            room: current_room.clone(),
                        }).await;
                    }
                    Err(e) => println!("Failed to leave room: {}", e),
                }
            }
            "/history" => {
                let limit = if parts.len() == 2 {
                    parts[1].parse().ok()
                } else {
                    None
                };
                
                if let Err(e) = send_request(connection, ChatRequest::GetHistory {
                    room: current_room.clone(),
                    limit,
                }).await {
                    println!("Failed to get history: {}", e);
                }
            }
            "/rooms" => {
                if let Err(e) = send_request(connection, ChatRequest::ListRooms).await {
                    println!("Failed to list rooms: {}", e);
                }
            }
            "/users" => {
                if let Err(e) = send_request(connection, ChatRequest::ListUsers {
                    room: current_room.clone(),
                }).await {
                    println!("Failed to list users: {}", e);
                }
            }
            "/help" => show_help(),
            _ if input.starts_with('/') => {
                println!("Unknown command: {}. Type /help for available commands.", command);
            }
            _ => {
                // Regular message
                if let Err(e) = send_request(connection, ChatRequest::SendMessage {
                    user: user.to_string(),
                    room: current_room.clone(),
                    content: input.to_string(),
                }).await {
                    println!("Failed to send message: {}", e);
                }
            }
        }
    }
    
    println!("Goodbye!");
    Ok(())
}

async fn send_request(connection: &quiccrunch_core::Connection, request: ChatRequest) -> Result<()> {
    let request_bytes = serde_json::to_vec(&request)?;
    let response_bytes = connection.request(request_bytes).await?;
    
    match serde_json::from_slice::<ChatResponse>(&response_bytes) {
        Ok(response) => {
            match response {
                ChatResponse::Success { message } => {
                    println!("✓ {}", message);
                }
                ChatResponse::Error { error } => {
                    println!("✗ Error: {}", error);
                }
                ChatResponse::MessageReceived { message } => {
                    println!("[{}] {}: {}", 
                        message.timestamp.format("%H:%M:%S"),
                        message.user,
                        message.content
                    );
                }
                ChatResponse::History { messages } => {
                    if messages.is_empty() {
                        println!("No message history");
                    } else {
                        println!("--- Message History ---");
                        for msg in messages {
                            println!("[{}] {}: {}", 
                                msg.timestamp.format("%H:%M:%S"),
                                msg.user,
                                msg.content
                            );
                        }
                        println!("--- End History ---");
                    }
                }
                ChatResponse::RoomList { rooms } => {
                    if rooms.is_empty() {
                        println!("No active rooms");
                    } else {
                        println!("Active rooms: {}", rooms.join(", "));
                    }
                }
                ChatResponse::UserList { users } => {
                    if users.is_empty() {
                        println!("No users in room");
                    } else {
                        println!("Users in room: {}", users.join(", "));
                    }
                }
            }
        }
        Err(e) => {
            println!("Failed to parse response: {}", e);
            println!("Raw response: {}", String::from_utf8_lossy(&response_bytes));
        }
    }
    
    Ok(())
}

fn show_help() {
    println!("Commands:");
    println!("  /join <room>    - Join a chat room");
    println!("  /leave          - Leave current room");
    println!("  /history [n]    - Show message history (last n messages)");
    println!("  /rooms          - List active rooms");
    println!("  /users          - List users in current room");
    println!("  /help           - Show this help");
    println!("  quit            - Exit the chat");
    println!("  <message>       - Send a message to current room");
    println!();
}