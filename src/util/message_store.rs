use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};
use redis::aio::Connection;
use redis::{AsyncCommands, RedisError};
use log::{info, warn, error, debug};
use sqlx::mysql::MySqlPool;
use sqlx::Row;
use chrono::{Local, NaiveDateTime};

struct RedisState {
    conn: Option<Connection>,
    use_memory: bool,
    reconnect_in_progress: bool,
}

/// MessageStore provides Redis-backed message storage with MySQL persistence and in-memory fallback
pub struct MessageStore {
    redis_state: Arc<Mutex<RedisState>>,
    redis_url: Option<String>,
    reconnect_max_attempts: u32,
    reconnect_interval_secs: u64,
    mysql_pool: Option<MySqlPool>,
    memory_store: Arc<Mutex<HashMap<String, String>>>,
}

#[derive(Debug, Clone)]
pub struct MessageRecord {
    pub message_id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub send_time: NaiveDateTime,
    pub group_id: Option<String>,
    pub group_name: Option<String>,
    pub content: String,
    pub at_target_list: Option<String>,
}

impl MessageStore {
    /// Initialize the message store, try Redis first, MySQL for persistence, fallback to memory
    pub async fn new(
        redis_url: Option<&str>,
        mysql_url: Option<&str>,
        max_reconnect_attempts: Option<u32>,
        reconnect_interval_secs: Option<u64>,
    ) -> Self {
        let memory_store = Arc::new(Mutex::new(HashMap::new()));
        let reconnect_max_attempts = max_reconnect_attempts.unwrap_or(3);
        let reconnect_interval_secs = reconnect_interval_secs.unwrap_or(60);
        let redis_url_owned = redis_url.map(|u| u.to_string());
        let mut redis_state = RedisState {
            conn: None,
            use_memory: true,
            reconnect_in_progress: false,
        };
        
        let mysql_pool = if let Some(url) = mysql_url {
            match MySqlPool::connect(url).await {
                Ok(pool) => {
                    info!("[MessageStore] Connected to MySQL at {}", url);
                    Some(pool)
                }
                Err(e) => {
                    error!("[MessageStore] Failed to connect to MySQL: {}", e);
                    None
                }
            }
        } else {
            warn!("[MessageStore] No MySQL URL provided. Persistent storage disabled.");
            None
        };

        if let Some(url) = redis_url {
            match redis::Client::open(url) {
                Ok(client) => match client.get_tokio_connection().await {
                    Ok(conn) => {
                        info!("[MessageStore] Connected to Redis at {}", url);
                        redis_state.conn = Some(conn);
                        redis_state.use_memory = false;
                    }
                    Err(e) => {
                        error!("[MessageStore] Failed to connect to Redis {}: {}", url, e);
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
            redis_state: Arc::new(Mutex::new(redis_state)),
            redis_url: redis_url_owned,
            reconnect_max_attempts,
            reconnect_interval_secs,
            mysql_pool,
            memory_store,
        }
    }

    async fn schedule_reconnect(&self) {
        let redis_url = match &self.redis_url {
            Some(url) => url.clone(),
            None => {
                warn!("[MessageStore] No Redis URL available for reconnection attempts.");
                return;
            }
        };

        {
            let mut state = self.redis_state.lock().await;
            if state.reconnect_in_progress {
                debug!("[MessageStore] Redis reconnection already in progress, skipping new attempt.");
                return;
            }
            state.reconnect_in_progress = true;
        }

        let state = self.redis_state.clone();
        let max_attempts = self.reconnect_max_attempts;
        let interval_secs = self.reconnect_interval_secs;
        let memory_store = self.memory_store.clone();

        tokio::spawn(async move {
            for attempt in 1..=max_attempts {
                match redis::Client::open(redis_url.as_str()) {
                    Ok(client) => match client.get_tokio_connection().await {
                        Ok(mut conn) => {
                            info!("[MessageStore] Redis reconnection succeeded on attempt {}", attempt);
                            
                            // Migrate in-memory data to Redis
                            let memory_data = {
                                let store = memory_store.lock().await;
                                store.clone()
                            };
                            
                            let mut migrated_count = 0;
                            let mut failed_count = 0;
                            for (key, value) in memory_data.iter() {
                                match conn.set::<_, _, ()>(key, value).await {
                                    Ok(_) => {
                                        migrated_count += 1;
                                        debug!("[MessageStore] Migrated message {} from memory to Redis", key);
                                    }
                                    Err(e) => {
                                        failed_count += 1;
                                        error!("[MessageStore] Failed to migrate message {} to Redis: {}", key, e);
                                    }
                                }
                            }
                            
                            if migrated_count > 0 {
                                info!("[MessageStore] Successfully migrated {} messages from memory to Redis", migrated_count);
                            }
                            if failed_count > 0 {
                                warn!("[MessageStore] Failed to migrate {} messages to Redis", failed_count);
                            }
                            
                            // Clear memory cache after successful migration
                            {
                                let mut store = memory_store.lock().await;
                                store.clear();
                                info!("[MessageStore] Cleared in-memory cache after Redis reconnection");
                            }
                            
                            // Update state
                            let mut guard = state.lock().await;
                            guard.conn = Some(conn);
                            guard.use_memory = false;
                            guard.reconnect_in_progress = false;
                            return;
                        }
                        Err(e) => {
                            error!(
                                "[MessageStore] Redis reconnection attempt {} failed: {}",
                                attempt, e
                            );
                        }
                    },
                    Err(e) => {
                        error!("[MessageStore] Invalid Redis URL during reconnection: {}", e);
                        break;
                    }
                }

                if attempt < max_attempts {
                    sleep(Duration::from_secs(interval_secs)).await;
                }
            }

            let mut guard = state.lock().await;
            guard.reconnect_in_progress = false;
            warn!(
                "[MessageStore] Exhausted Redis reconnection attempts ({} tries). Continuing with in-memory storage.",
                max_attempts
            );
        });
    }

    /// Store a message by ID
    pub async fn store_message(&self, message_id: &str, message: &str) {
        let mut need_reconnect = false;

        {
            let mut state = self.redis_state.lock().await;
            if !state.use_memory {
                if let Some(conn) = state.conn.as_mut() {
                    let result: Result<(), RedisError> = conn.set(message_id, message).await;
                    match result {
                        Ok(_) => {
                            debug!("[MessageStore] Message stored in Redis: {}", message_id);
                            return;
                        }
                        Err(e) => {
                            error!("[MessageStore] Failed to store message in Redis: {}", e);
                            state.use_memory = true;
                            state.conn = None;
                            need_reconnect = true;
                            warn!("[MessageStore] Switching to in-memory message store due to Redis error.");
                        }
                    }
                } else {
                    state.use_memory = true;
                    need_reconnect = true;
                    warn!("[MessageStore] Redis connection missing, switching to in-memory store.");
                }
            } else if self.redis_url.is_some() && !state.reconnect_in_progress {
                // Redis is configured but currently unavailable; trigger a reconnection attempt in the background.
                need_reconnect = true;
            }
        }

        if need_reconnect {
            self.schedule_reconnect().await;
        }
        // Fallback to memory
        let mut store = self.memory_store.lock().await;
        store.insert(message_id.to_string(), message.to_string());
        debug!("[MessageStore] Message stored in memory: {}", message_id);
    }

    /// Store a full message record to MySQL
    pub async fn store_message_record(&self, record: &MessageRecord) -> Result<(), String> {
        if let Some(pool) = &self.mysql_pool {
            let result = sqlx::query(
                r#"
                INSERT INTO message_record 
                (message_id, sender_id, sender_name, send_time, group_id, group_name, content, at_target_list)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                "#
            )
            .bind(&record.message_id)
            .bind(&record.sender_id)
            .bind(&record.sender_name)
            .bind(record.send_time)
            .bind(&record.group_id)
            .bind(&record.group_name)
            .bind(&record.content)
            .bind(&record.at_target_list)
            .execute(pool)
            .await;

            match result {
                Ok(_) => {
                    debug!("[MessageStore] Message record persisted to MySQL: {}", record.message_id);
                    Ok(())
                }
                Err(e) => {
                    error!("[MessageStore] Failed to store message record in MySQL: {}", e);
                    Err(format!("MySQL storage failed: {}", e))
                }
            }
        } else {
            warn!("[MessageStore] MySQL pool not available for persistence");
            Err("MySQL pool not configured".to_string())
        }
    }

    /// Retrieve a message record from MySQL by message_id
    pub async fn get_message_record(&self, message_id: &str) -> Result<Option<MessageRecord>, String> {
        if let Some(pool) = &self.mysql_pool {
            let result = sqlx::query(
                r#"
                SELECT message_id, sender_id, sender_name, send_time, group_id, group_name, content, at_target_list
                FROM message_record
                WHERE message_id = ?
                "#
            )
            .bind(message_id)
            .fetch_optional(pool)
            .await;

            match result {
                Ok(Some(row)) => {
                    let record = MessageRecord {
                        message_id: row.get("message_id"),
                        sender_id: row.get("sender_id"),
                        sender_name: row.get("sender_name"),
                        send_time: row.get("send_time"),
                        group_id: row.get("group_id"),
                        group_name: row.get("group_name"),
                        content: row.get("content"),
                        at_target_list: row.get("at_target_list"),
                    };
                    debug!("[MessageStore] Message record retrieved from MySQL: {}", message_id);
                    Ok(Some(record))
                }
                Ok(None) => {
                    debug!("[MessageStore] Message record not found in MySQL: {}", message_id);
                    Ok(None)
                }
                Err(e) => {
                    error!("[MessageStore] Failed to retrieve message record from MySQL: {}", e);
                    Err(format!("MySQL retrieval failed: {}", e))
                }
            }
        } else {
            Err("MySQL pool not configured".to_string())
        }
    }

    /// Get a message by ID
    pub async fn get_message(&self, message_id: &str) -> Option<String> {
        let mut need_reconnect = false;

        {
            let mut state = self.redis_state.lock().await;
            if !state.use_memory {
                if let Some(conn) = state.conn.as_mut() {
                    let result: Result<Option<String>, RedisError> = conn.get(message_id).await;
                    match result {
                        Ok(val) => return val,
                        Err(e) => {
                            error!("[MessageStore] Failed to get message from Redis: {}", e);
                            state.use_memory = true;
                            state.conn = None;
                            need_reconnect = true;
                            warn!("[MessageStore] Switching to in-memory message store due to Redis error.");
                        }
                    }
                } else {
                    state.use_memory = true;
                    need_reconnect = true;
                    warn!("[MessageStore] Redis connection missing, switching to in-memory store.");
                }
            } else if self.redis_url.is_some() && !state.reconnect_in_progress {
                need_reconnect = true;
            }
        }

        if need_reconnect {
            self.schedule_reconnect().await;
        }
        // Fallback to memory
        let store = self.memory_store.lock().await;
        store.get(message_id).cloned()
    }

    /// Get a message by ID from Redis, fallback to MySQL, then memory
    pub async fn get_message_with_mysql(&self, message_id: &str) -> Option<String> {
        let mut need_reconnect = false;
        // Try Redis first
        {
            let mut state = self.redis_state.lock().await;
            if !state.use_memory {
                if let Some(conn) = state.conn.as_mut() {
                    let result: Result<Option<String>, RedisError> = conn.get(message_id).await;
                    match result {
                        Ok(val) => {
                            if val.is_some() {
                                return val;
                            }
                        }
                        Err(e) => {
                            error!("[MessageStore] Failed to get message from Redis: {}", e);
                            state.use_memory = true;
                            state.conn = None;
                            need_reconnect = true;
                            warn!("[MessageStore] Switching to in-memory message store due to Redis error.");
                        }
                    }
                } else {
                    state.use_memory = true;
                    need_reconnect = true;
                    warn!("[MessageStore] Redis connection missing, switching to in-memory store.");
                }
            } else if self.redis_url.is_some() && !state.reconnect_in_progress {
                need_reconnect = true;
            }
        }

        if need_reconnect {
            self.schedule_reconnect().await;
        }

        // Try MySQL as fallback
        if let Some(pool) = &self.mysql_pool {
            if let Ok(Some(record)) = sqlx::query_as::<_, (String,)>(
                "SELECT content FROM message_record WHERE message_id = ? LIMIT 1"
            )
            .bind(message_id)
            .fetch_optional(pool)
            .await
            {
                debug!("[MessageStore] Message retrieved from MySQL: {}", message_id);
                return Some(record.0);
            }
        }

        // Fallback to memory
        let store = self.memory_store.lock().await;
        store.get(message_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::{MessageStore, MessageRecord};
    use tokio;

    #[tokio::test]
    async fn test_memory_store() {
        let store = MessageStore::new(None, None, None, None).await;
        store.store_message("id1", "hello").await;
        let val = store.get_message("id1").await;
        assert_eq!(val, Some("hello".to_string()));
    }

    #[tokio::test]
    async fn test_memory_store_overwrite() {
        let store = MessageStore::new(None, None, None, None).await;
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
        let store = MessageStore::new(redis_url.as_deref(), None, Some(3), Some(1)).await;
        store.store_message("id3", "redis_test").await;
        let val = store.get_message("id3").await;
        assert_eq!(val, Some("redis_test".to_string()));
    }

    // To test MySQL, set DATABASE_URL env var to a running MySQL instance
    #[tokio::test]
    async fn test_mysql_store() {
        let mysql_url = std::env::var("DATABASE_URL").ok();
        if mysql_url.is_none() {
            // Skip if no MySQL URL
            return;
        }
        let store = MessageStore::new(None, mysql_url.as_deref(), None, None).await;
        let record = MessageRecord {
            message_id: "test_msg_001".to_string(),
            sender_id: "user_123".to_string(),
            sender_name: "Test User".to_string(),
            send_time: Local::now().naive_local(),
            group_id: Some("group_456".to_string()),
            group_name: Some("Test Group".to_string()),
            content: "Hello, this is a test message".to_string(),
            at_target_list: Some("@user1,@user2".to_string()),
        };
        
        let result = store.store_message_record(&record).await;
        assert!(result.is_ok());

        let retrieved = store.get_message_record("test_msg_001").await;
        assert!(retrieved.is_ok());
        assert!(retrieved.unwrap().is_some());
    }
}
