use anyhow::{Context, Result};
use fake::{Fake, Faker};
use rand::prelude::*;
use std::time::Duration;
use tokio::time::{interval, Interval};

use crate::config::{RequestRateConfig, MessageSizeConfig};
use crate::scenarios::MockConnection;

pub struct RequestGenerator {
    rate_config: RequestRateConfig,
}

impl RequestGenerator {
    pub fn new(rate_config: RequestRateConfig) -> Self {
        Self { rate_config }
    }

    pub async fn create_timer(&self) -> Result<Interval> {
        match &self.rate_config {
            RequestRateConfig::Constant { rate } => {
                let interval_duration = Duration::from_secs_f64(1.0 / *rate as f64);
                Ok(interval(interval_duration))
            }
            RequestRateConfig::Variable { base_rate, .. } => {
                // For variable rates, start with base rate and adjust dynamically
                let interval_duration = Duration::from_secs_f64(1.0 / *base_rate as f64);
                Ok(interval(interval_duration))
            }
            RequestRateConfig::Burst { base_rate, .. } => {
                let interval_duration = Duration::from_secs_f64(1.0 / *base_rate as f64);
                Ok(interval(interval_duration))
            }
        }
    }

    pub fn get_current_rate(&self, elapsed: Duration) -> u32 {
        match &self.rate_config {
            RequestRateConfig::Constant { rate } => *rate,
            RequestRateConfig::Variable { pattern, base_rate, variation } => {
                self.calculate_variable_rate(pattern, *base_rate, *variation, elapsed)
            }
            RequestRateConfig::Burst { base_rate, burst_rate, burst_duration } => {
                self.calculate_burst_rate(*base_rate, *burst_rate, *burst_duration, elapsed)
            }
        }
    }

    fn calculate_variable_rate(&self, pattern: &str, base_rate: u32, variation: f64, elapsed: Duration) -> u32 {
        let elapsed_secs = elapsed.as_secs_f64();
        let multiplier = match pattern {
            "sine" => {
                let period = 60.0; // 1-minute period
                1.0 + variation * (2.0 * std::f64::consts::PI * elapsed_secs / period).sin()
            }
            "ramp" => {
                let ramp_duration = 300.0; // 5-minute ramp
                1.0 + variation * (elapsed_secs / ramp_duration).min(1.0)
            }
            "sawtooth" => {
                let period = 120.0; // 2-minute period
                let phase = (elapsed_secs % period) / period;
                1.0 + variation * phase
            }
            _ => 1.0, // Default to constant
        };
        
        (base_rate as f64 * multiplier) as u32
    }

    fn calculate_burst_rate(&self, base_rate: u32, burst_rate: u32, burst_duration: Duration, elapsed: Duration) -> u32 {
        let cycle_duration = burst_duration + Duration::from_secs(60); // 1-minute cycle
        let cycle_position = elapsed.as_secs() % cycle_duration.as_secs();
        
        if cycle_position < burst_duration.as_secs() {
            burst_rate
        } else {
            base_rate
        }
    }
}

pub struct MessageGenerator {
    size_config: MessageSizeConfig,
}

impl MessageGenerator {
    pub fn new(size_config: MessageSizeConfig) -> Self {
        Self {
            size_config,
        }
    }

    pub fn generate_size(&self) -> usize {
        match &self.size_config {
            MessageSizeConfig::Fixed { size } => *size,
            MessageSizeConfig::Range { min, max } => {
                use rand::{thread_rng, Rng};
                thread_rng().gen_range(*min..=*max)
            }
            MessageSizeConfig::Distribution { pattern } => {
                self.generate_distributed_size(pattern)
            }
        }
    }

    fn generate_distributed_size(&self, pattern: &str) -> usize {
        let mut rng = thread_rng();
        match pattern {
            "realistic" => {
                // Realistic RPC message size distribution
                // Small messages (64-1KB): 60%
                // Medium messages (1KB-16KB): 30%
                // Large messages (16KB-1MB): 10%
                let rand_val: f64 = rng.gen();
                
                if rand_val < 0.6 {
                    rng.gen_range(64..=1024)
                } else if rand_val < 0.9 {
                    rng.gen_range(1024..=16384)
                } else {
                    rng.gen_range(16384..=1048576)
                }
            }
            "small" => rng.gen_range(64..=1024),
            "medium" => rng.gen_range(1024..=16384),
            "large" => rng.gen_range(16384..=1048576),
            "exponential" => {
                // Exponential distribution with lambda = 0.001
                let lambda = 0.001;
                let u: f64 = rng.gen();
                (-u.ln() / lambda) as usize
            }
            _ => 1024, // Default size
        }
    }

    pub fn generate_payload(&self, size: usize) -> Vec<u8> {
        // Generate realistic test data
        match size {
            0..=1024 => self.generate_simple_payload(size),
            1025..=16384 => self.generate_structured_payload(size),
            _ => self.generate_binary_payload(size),
        }
    }

    fn generate_simple_payload(&self, size: usize) -> Vec<u8> {
        // Simple repeated pattern for small messages
        let pattern = b"QuicCrunch Load Test Data ";
        let mut payload = Vec::with_capacity(size);
        
        while payload.len() < size {
            let remaining = size - payload.len();
            if remaining >= pattern.len() {
                payload.extend_from_slice(pattern);
            } else {
                payload.extend_from_slice(&pattern[..remaining]);
            }
        }
        
        payload
    }

    fn generate_structured_payload(&self, size: usize) -> Vec<u8> {
        // Generate JSON-like structured data for medium messages
        let mut payload = String::new();
        payload.push_str(r#"{"type":"load_test","timestamp":"#);
        payload.push_str(&chrono::Utc::now().timestamp().to_string());
        payload.push_str(r#"","data":"#);
        
        let data_size = size.saturating_sub(payload.len() + 3);
        let data: String = (0..data_size).map(|_| {
            let mut rng = thread_rng();
            char::from(b'A' + (rng.gen::<u8>() % 26))
        }).collect();
        payload.push_str(&data);
        payload.push_str(r#""}"#);
        
        payload.into_bytes()
    }

    fn generate_binary_payload(&self, size: usize) -> Vec<u8> {
        let mut payload = vec![0u8; size];
        let mut rng = thread_rng();
        rng.fill(&mut payload[..]);
        payload
    }
}

pub struct ConnectionGenerator {
    connection_count: u32,
}

impl ConnectionGenerator {
    pub fn new(connection_count: u32) -> Self {
        Self { connection_count }
    }

    pub async fn generate_connections(&self) -> Result<Vec<MockConnection>> {
        let mut connections = Vec::with_capacity(self.connection_count as usize);
        
        for i in 0..self.connection_count {
            let connection = MockConnection::new(
                format!("conn-{}", i),
                "quinn".to_string(),
            );
            
            // Simulate connection establishment delay
            let delay = Duration::from_millis(thread_rng().gen_range(1..=10));
            tokio::time::sleep(delay).await;
            
            connections.push(connection);
        }
        
        Ok(connections)
    }

    pub async fn generate_connections_gradually(&self, duration: Duration) -> Result<Vec<MockConnection>> {
        let mut connections = Vec::with_capacity(self.connection_count as usize);
        let interval_duration = duration / self.connection_count;
        let mut interval = tokio::time::interval(interval_duration);
        
        for i in 0..self.connection_count {
            interval.tick().await;
            
            let connection = MockConnection::new(
                format!("conn-{}", i),
                "quinn".to_string(),
            );
            
            connections.push(connection);
        }
        
        Ok(connections)
    }
}

pub struct TestDataGenerator;

impl TestDataGenerator {
    pub fn generate_user_data() -> UserData {
        UserData {
            id: uuid::Uuid::new_v4(),
            name: Faker.fake(),
            email: Faker.fake(),
            age: (18..=80).fake(),
            created_at: chrono::Utc::now(),
            preferences: Self::generate_preferences(),
        }
    }

    pub fn generate_transaction_data() -> TransactionData {
        TransactionData {
            id: uuid::Uuid::new_v4(),
            user_id: uuid::Uuid::new_v4(),
            amount: (1.0..=10000.0).fake(),
            currency: ["USD", "EUR", "GBP", "JPY"].choose(&mut thread_rng()).unwrap().to_string(),
            description: Faker.fake(),
            timestamp: chrono::Utc::now(),
            metadata: Self::generate_metadata(),
        }
    }

    pub fn generate_chat_message() -> ChatMessage {
        let message_types = ["text", "image", "file", "emoji"];
        let message_type = message_types.choose(&mut thread_rng()).unwrap();
        
        ChatMessage {
            id: uuid::Uuid::new_v4(),
            user_id: uuid::Uuid::new_v4(),
            room_id: uuid::Uuid::new_v4(),
            message_type: message_type.to_string(),
            content: match *message_type {
                "text" => Faker.fake(),
                "image" => format!("image_{}.jpg", uuid::Uuid::new_v4()),
                "file" => format!("document_{}.pdf", uuid::Uuid::new_v4()),
                "emoji" => "😀🎉🚀".to_string(),
                _ => Faker.fake(),
            },
            timestamp: chrono::Utc::now(),
            reactions: Vec::new(),
        }
    }

    fn generate_preferences() -> serde_json::Value {
        let themes = ["light", "dark"];
        let languages = ["en", "es", "fr", "de", "ja"];
        let mut rng = thread_rng();
        
        serde_json::json!({
            "theme": themes.choose(&mut rng).unwrap(),
            "language": languages.choose(&mut rng).unwrap(),
            "notifications": rng.gen::<bool>(),
            "privacy_level": rng.gen_range(1..=5)
        })
    }

    fn generate_metadata() -> serde_json::Value {
        let sources = ["web", "mobile", "api"];
        let mut rng = thread_rng();
        
        serde_json::json!({
            "source": sources.choose(&mut rng).unwrap(),
            "version": format!("v{}.{}.{}", 
                rng.gen_range(1..=3),
                rng.gen_range(0..=20),
                rng.gen_range(0..=100)
            ),
            "session_id": uuid::Uuid::new_v4(),
            "ip_address": format!("{}.{}.{}.{}", 
                rng.gen_range(1..=255),
                rng.gen_range(0..=255),
                rng.gen_range(0..=255),
                rng.gen_range(1..=255)
            )
        })
    }
}

// Test data structures
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UserData {
    pub id: uuid::Uuid,
    pub name: String,
    pub email: String,
    pub age: u8,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub preferences: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TransactionData {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub amount: f64,
    pub currency: String,
    pub description: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub room_id: uuid::Uuid,
    pub message_type: String,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub reactions: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_rate_generation() {
        let constant_rate = RequestRateConfig::Constant { rate: 1000 };
        let generator = RequestGenerator::new(constant_rate);
        
        let rate = generator.get_current_rate(Duration::from_secs(0));
        assert_eq!(rate, 1000);
        
        let rate = generator.get_current_rate(Duration::from_secs(60));
        assert_eq!(rate, 1000);
    }

    #[test]
    fn test_variable_rate_generation() {
        let variable_rate = RequestRateConfig::Variable {
            pattern: "sine".to_string(),
            base_rate: 1000,
            variation: 0.5,
        };
        let generator = RequestGenerator::new(variable_rate);
        
        let rate_0 = generator.get_current_rate(Duration::from_secs(0));
        let rate_15 = generator.get_current_rate(Duration::from_secs(15));
        let rate_30 = generator.get_current_rate(Duration::from_secs(30));
        
        // Rates should vary according to sine pattern
        assert_ne!(rate_0, rate_15);
        assert_ne!(rate_15, rate_30);
    }

    #[test]
    fn test_message_size_generation() {
        let generator = MessageGenerator::new(MessageSizeConfig::Fixed { size: 1024 });
        assert_eq!(generator.generate_size(), 1024);
        
        let generator = MessageGenerator::new(MessageSizeConfig::Range { min: 100, max: 1000 });
        let size = generator.generate_size();
        assert!(size >= 100 && size <= 1000);
    }

    #[test]
    fn test_payload_generation() {
        let generator = MessageGenerator::new(MessageSizeConfig::Fixed { size: 1024 });
        let payload = generator.generate_payload(1024);
        assert_eq!(payload.len(), 1024);
        
        let large_payload = generator.generate_payload(65536);
        assert_eq!(large_payload.len(), 65536);
    }

    #[tokio::test]
    async fn test_connection_generation() {
        let generator = ConnectionGenerator::new(10);
        let connections = generator.generate_connections().await.unwrap();
        assert_eq!(connections.len(), 10);
        
        for (i, conn) in connections.iter().enumerate() {
            assert_eq!(conn.id, format!("conn-{}", i));
        }
    }

    #[test]
    fn test_test_data_generation() {
        let user = TestDataGenerator::generate_user_data();
        assert!(!user.name.is_empty());
        assert!(user.email.contains('@'));
        assert!(user.age >= 18 && user.age <= 80);
        
        let transaction = TestDataGenerator::generate_transaction_data();
        assert!(transaction.amount > 0.0);
        assert!(!transaction.currency.is_empty());
        
        let message = TestDataGenerator::generate_chat_message();
        assert!(!message.content.is_empty());
        assert!(!message.message_type.is_empty());
    }
}