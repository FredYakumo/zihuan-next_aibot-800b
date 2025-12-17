use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use redis::aio::Connection;
use redis::{AsyncCommands, RedisError};
use log::{info, warn, error, debug};

/// MessageStore provides Redis-backed message storage with in-memory fallback
pub struct MessageStore {
    redis_conn: Option<Connection>,
    memory_store: Arc<Mutex<HashMap<String, String>>>,
    use_memory: bool,
}

impl MessageStore {
    /// Initialize the message store, try Redis first, fallback to memory
    pub async fn new(redis_url: Option<&str>) -> Self {
        let memory_store = Arc::new(Mutex::new(HashMap::new()));
        if let Some(url) = redis_url {
            match redis::Client::open(url) {
                Ok(client) => match client.get_tokio_connection().await {
                    Ok(conn) => {
                        info!("[MessageStore] Connected to Redis at {}", url);
                        return Self {
                            redis_conn: Some(conn),
                            memory_store,
                            use_memory: false,
                        };
                    }
                    Err(e) => {
                        error!("[MessageStore] Failed to connect to Redis {}: {}", url,e);
                        warn!("[MessageStore] Falling back to in-memory message store due to Redis connection error.");
                    }
                },
                Err(e) => {
                    error!("[MessageStore] Invalid Redis URL: {}", e);
                    warn!("[MessageStore] Falling back to in-memory message store due to invalid Redis URL.");
                }
            }
        } else {
            warn!("[MessageStore] No Redis URL provided. Using in-memory message store.");
        }
        Self {
            redis_conn: None,
            memory_store,
            use_memory: true,
        }
    }

    /// Store a message by ID
    pub async fn store_message(&mut self, message_id: &str, message: &str) {
        if !self.use_memory {
            if let Some(conn) = &mut self.redis_conn {
                let result: Result<(), RedisError> = conn.set(message_id, message).await;
                match result {
                    Ok(_) => {
                        debug!("[MessageStore] Message stored in Redis: {}", message_id);
                        return;
                    }
                    Err(e) => {
                        error!("[MessageStore] Failed to store message in Redis: {}", e);
                        self.use_memory = true;
                        warn!("[MessageStore] Switching to in-memory message store due to Redis error.");
                    }
                }
            }
        }
        // Fallback to memory
        let mut store = self.memory_store.lock().await;
        store.insert(message_id.to_string(), message.to_string());
        debug!("[MessageStore] Message stored in memory: {}", message_id);
    }

    /// Get a message by ID
    pub async fn get_message(&mut self, message_id: &str) -> Option<String> {
        if !self.use_memory {
            if let Some(conn) = &mut self.redis_conn {
                let result: Result<Option<String>, RedisError> = conn.get(message_id).await;
                match result {
                    Ok(val) => return val,
                    Err(e) => {
                        error!("[MessageStore] Failed to get message from Redis: {}", e);
                        self.use_memory = true;
                        warn!("[MessageStore] Switching to in-memory message store due to Redis error.");
                    }
                }
            }
        }
        // Fallback to memory
        let store = self.memory_store.lock().await;
        store.get(message_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::MessageStore;
    use tokio;

    #[tokio::test]
    async fn test_memory_store() {
        let mut store = MessageStore::new(None).await;
        store.store_message("id1", "hello").await;
        let val = store.get_message("id1").await;
        assert_eq!(val, Some("hello".to_string()));
    }

    #[tokio::test]
    async fn test_memory_store_overwrite() {
        let mut store = MessageStore::new(None).await;
        store.store_message("id2", "foo").await;
        store.store_message("id2", "bar").await;
        let val = store.get_message("id2").await;
        assert_eq!(val, Some("bar".to_string()));
    }

    // To test Redis, set REDIS_URL env var to a running Redis instance
    #[tokio::test]
    async fn test_redis_store() {
        let redis_url = std::env::var("REDIS_URL").ok();
        if redis_url.is_none() {
            // Skip if no Redis URL
            return;
        }
        let mut store = MessageStore::new(redis_url.as_deref()).await;
        store.store_message("id3", "redis_test").await;
        let val = store.get_message("id3").await;
        assert_eq!(val, Some("redis_test".to_string()));
    }
}
