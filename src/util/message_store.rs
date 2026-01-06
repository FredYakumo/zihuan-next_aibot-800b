use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};
use redis::aio::Connection;
use redis::{AsyncCommands};
use log::{info, warn, error, debug};
use sqlx::mysql::MySqlPool;
use sqlx::Row;
use chrono::NaiveDateTime;
use crate::util::mask_url_credentials;
use crate::error::Result;


struct RedisState {
    conn: Option<Connection>,
    use_memory: bool,
    reconnect_in_progress: bool,
}

struct MySqlState {
    pool: Option<MySqlPool>,
    use_memory: bool,
    reconnect_in_progress: bool,
}

/// MessageStore provides Redis-backed message storage with MySQL persistence and in-memory fallback
pub struct MessageStore {
    redis_state: Arc<Mutex<RedisState>>,
    redis_url: Option<String>,
    reconnect_max_attempts: u32,
    reconnect_interval_secs: u64,
    mysql_state: Arc<Mutex<MySqlState>>,
    mysql_url: Option<String>,
    mysql_reconnect_max_attempts: u32,
    mysql_reconnect_interval_secs: u64,
    memory_store: Arc<Mutex<HashMap<String, String>>>,
    mysql_memory_store: Arc<Mutex<HashMap<String, MessageRecord>>>,
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
        mysql_max_reconnect_attempts: Option<u32>,
        mysql_reconnect_interval_secs: Option<u64>,
    ) -> Self {
        let memory_store = Arc::new(Mutex::new(HashMap::new()));
        let mysql_memory_store = Arc::new(Mutex::new(HashMap::new()));
        let reconnect_max_attempts = max_reconnect_attempts.unwrap_or(3);
        let reconnect_interval_secs = reconnect_interval_secs.unwrap_or(60);
        let mysql_reconnect_max_attempts = mysql_max_reconnect_attempts.unwrap_or(3);
        let mysql_reconnect_interval_secs = mysql_reconnect_interval_secs.unwrap_or(60);
        let redis_url_owned = redis_url.map(|u| u.to_string());
        let mysql_url_owned = mysql_url.map(|u| u.to_string());
        let mut redis_state = RedisState {
            conn: None,
            use_memory: true,
            reconnect_in_progress: false,
        };
        let mut mysql_state = MySqlState {
            pool: None,
            use_memory: true,
            reconnect_in_progress: false,
        };
        
        if let Some(url) = mysql_url {
            let safe_url = mask_url_credentials(url);

            match MySqlPool::connect(url).await {
                Ok(pool) => {
                    info!("[MessageStore] Connected to MySQL at {}", safe_url);
                    mysql_state.pool = Some(pool);
                    mysql_state.use_memory = false;
                }
                Err(e) => {
                    error!("[MessageStore] Failed to connect to MySQL {}: {}", safe_url, e);
                    warn!("[MessageStore] Falling back to in-memory message record store due to MySQL connection error.");
                }
            }
        } else {
            warn!("[MessageStore] No MySQL URL provided. Persistent storage disabled.");
        }

        if let Some(url) = redis_url {
            let safe_url = mask_url_credentials(url);
            match redis::Client::open(url) {
                Ok(client) => match client.get_tokio_connection().await {
                    Ok(mut conn) => {
                        info!("[MessageStore] Connected to Redis at {}", safe_url);
                        
                        // Clear all existing messages in Redis on startup
                        match redis::cmd("FLUSHDB").query_async::<_, ()>(&mut conn).await {
                            Ok(_) => info!("[MessageStore] Cleared all existing messages from Redis on startup"),
                            Err(e) => warn!("[MessageStore] Failed to clear Redis on startup: {}", e),
                        }
                        
                        redis_state.conn = Some(conn);
                        redis_state.use_memory = false;
                    }
                    Err(e) => {
                        error!("[MessageStore] Failed to connect to Redis {}: {}", safe_url, e);
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
            mysql_state: Arc::new(Mutex::new(mysql_state)),
            mysql_url: mysql_url_owned,
            mysql_reconnect_max_attempts,
            mysql_reconnect_interval_secs,
            memory_store,
            mysql_memory_store,
        }
    }

    /// Load recent messages from MySQL into Redis or memory cache on startup
    /// This populates the cache with historical messages for faster access
    pub async fn load_messages_from_mysql(&self, limit: u32) -> Result<u32> {
        let state = self.mysql_state.lock().await;
        
        if state.pool.is_none() {
            warn!("[MessageStore] No MySQL pool available, skipping message loading");
            return Ok(0);
        }
        
        let pool = state.pool.as_ref().unwrap();
        
        // Query recent messages ordered by send_time DESC
        let records = sqlx::query(
            r#"
            SELECT message_id, sender_id, sender_name, send_time, group_id, group_name, content, at_target_list
            FROM message_record
            ORDER BY send_time DESC
            LIMIT ?
            "#
        )
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(|e| crate::string_error!("Failed to query messages from MySQL: {}", e))?;
        
        info!("[MessageStore] Loaded {} message records from MySQL", records.len());
        
        let mut loaded_count = 0;
        let mut redis_state = self.redis_state.lock().await;
        
        for row in records {
            let message_id: String = row.get("message_id");
            let content: String = row.get("content");
            
            // Try to store in Redis first, fallback to memory
            if !redis_state.use_memory {
                if let Some(conn) = redis_state.conn.as_mut() {
                    match conn.set::<_, _, ()>(&message_id, &content).await {
                        Ok(_) => {
                            loaded_count += 1;
                            debug!("[MessageStore] Loaded message {} into Redis from MySQL", message_id);
                        }
                        Err(e) => {
                            error!("[MessageStore] Failed to load message {} into Redis: {}", message_id, e);
                            // Switch to memory and store there
                            redis_state.use_memory = true;
                            let mut mem = self.memory_store.lock().await;
                            mem.insert(message_id.clone(), content.clone());
                            loaded_count += 1;
                        }
                    }
                }
            } else {
                // Store directly to memory
                let mut mem = self.memory_store.lock().await;
                mem.insert(message_id.clone(), content.clone());
                loaded_count += 1;
                debug!("[MessageStore] Loaded message {} into memory from MySQL", message_id);
            }
        }
        
        info!("[MessageStore] Successfully loaded {} messages from MySQL into cache", loaded_count);
        Ok(loaded_count)
    }

    /// Get messages by sender_id and optionally group_id from MySQL
    /// Returns messages ordered by send_time DESC (most recent first)
    pub async fn get_messages_by_sender(
        &self,
        sender_id: &str,
        group_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<MessageRecord>> {
        let state = self.mysql_state.lock().await;
        
        if state.pool.is_none() {
            warn!("[MessageStore] No MySQL pool available, checking memory buffer");
            // Fallback to memory buffer
            let mem = self.mysql_memory_store.lock().await;
            let mut records: Vec<MessageRecord> = mem.values()
                .filter(|r| {
                    r.sender_id == sender_id && 
                    (group_id.is_none() || r.group_id.as_deref() == group_id)
                })
                .cloned()
                .collect();
            
            // Sort by send_time DESC
            records.sort_by(|a, b| b.send_time.cmp(&a.send_time));
            records.truncate(limit as usize);
            
            return Ok(records);
        }
        
        let pool = state.pool.as_ref().unwrap();
        
        let records = if let Some(gid) = group_id {
            // Query with both sender_id and group_id
            sqlx::query(
                r#"
                SELECT message_id, sender_id, sender_name, send_time, group_id, group_name, content, at_target_list
                FROM message_record
                WHERE sender_id = ? AND group_id = ?
                ORDER BY send_time DESC
                LIMIT ?
                "#
            )
            .bind(sender_id)
            .bind(gid)
            .bind(limit)
            .fetch_all(pool)
            .await
            .map_err(|e| crate::string_error!("Failed to query messages by sender and group: {}", e))?
        } else {
            // Query by sender_id only
            sqlx::query(
                r#"
                SELECT message_id, sender_id, sender_name, send_time, group_id, group_name, content, at_target_list
                FROM message_record
                WHERE sender_id = ?
                ORDER BY send_time DESC
                LIMIT ?
                "#
            )
            .bind(sender_id)
            .bind(limit)
            .fetch_all(pool)
            .await
            .map_err(|e| crate::string_error!("Failed to query messages by sender: {}", e))?
        };
        
        let mut result = Vec::new();
        for row in records {
            result.push(MessageRecord {
                message_id: row.get("message_id"),
                sender_id: row.get("sender_id"),
                sender_name: row.get("sender_name"),
                send_time: row.get("send_time"),
                group_id: row.get("group_id"),
                group_name: row.get("group_name"),
                content: row.get("content"),
                at_target_list: row.get("at_target_list"),
            });
        }
        
        debug!("[MessageStore] Retrieved {} messages for sender {} (group: {:?})", 
               result.len(), sender_id, group_id);
        Ok(result)
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

    async fn schedule_mysql_reconnect(&self) {
        let mysql_url = match &self.mysql_url {
            Some(url) => url.clone(),
            None => {
                warn!("[MessageStore] No MySQL URL available for reconnection attempts.");
                return;
            }
        };

        {
            let mut state = self.mysql_state.lock().await;
            if state.reconnect_in_progress {
                debug!("[MessageStore] MySQL reconnection already in progress, skipping new attempt.");
                return;
            }
            state.reconnect_in_progress = true;
        }

        let state = self.mysql_state.clone();
        let memory_records = self.mysql_memory_store.clone();
        let max_attempts = self.mysql_reconnect_max_attempts;
        let interval_secs = self.mysql_reconnect_interval_secs;

        tokio::spawn(async move {
            for attempt in 1..=max_attempts {
                match MySqlPool::connect(mysql_url.as_str()).await {
                    Ok(pool) => {
                        info!("[MessageStore] MySQL reconnection succeeded on attempt {}", attempt);

                        // Migrate in-memory records to MySQL
                        let records_map = {
                            let store = memory_records.lock().await;
                            store.clone()
                        };

                        let mut migrated_count = 0;
                        let mut failed_count = 0;

                        for (_, record) in records_map.iter() {
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
                            .execute(&pool)
                            .await;

                            match result {
                                Ok(_) => {
                                    migrated_count += 1;
                                    debug!("[MessageStore] Migrated record {} from memory to MySQL", record.message_id);
                                }
                                Err(e) => {
                                    failed_count += 1;
                                    error!("[MessageStore] Failed to migrate record {} to MySQL: {}", record.message_id, e);
                                }
                            }
                        }

                        if migrated_count > 0 {
                            info!("[MessageStore] Successfully migrated {} records from memory to MySQL", migrated_count);
                        }
                        if failed_count > 0 {
                            warn!("[MessageStore] Failed to migrate {} records to MySQL", failed_count);
                        }

                        // Clear memory after successful migration attempt
                        {
                            let mut store = memory_records.lock().await;
                            store.clear();
                            info!("[MessageStore] Cleared in-memory MySQL record cache after reconnection");
                        }

                        // Update state
                        let mut guard = state.lock().await;
                        guard.pool = Some(pool);
                        guard.use_memory = false;
                        guard.reconnect_in_progress = false;
                        return;
                    }
                    Err(e) => {
                        error!(
                            "[MessageStore] MySQL reconnection attempt {} failed: {}",
                            attempt, e
                        );
                    }
                }

                if attempt < max_attempts {
                    sleep(Duration::from_secs(interval_secs)).await;
                }
            }

            let mut guard = state.lock().await;
            guard.reconnect_in_progress = false;
            warn!(
                "[MessageStore] Exhausted MySQL reconnection attempts ({} tries). Continuing with in-memory records.",
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
                    match conn.set::<_, _, ()>(message_id, message).await {
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
    pub async fn store_message_record(&self, record: &MessageRecord) -> Result<()> {
        let mut need_reconnect = false;
        {
            let mut state = self.mysql_state.lock().await;
            if !state.use_memory {
                if let Some(pool) = &state.pool {
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
                            return Ok(());
                        }
                        Err(e) => {
                            error!("[MessageStore] Failed to store message record in MySQL: {}", e);
                            state.use_memory = true;
                            state.pool = None;
                            need_reconnect = true;
                            warn!("[MessageStore] Switching to in-memory message record store due to MySQL error.");
                        }
                    }
                } else {
                    state.use_memory = true;
                    need_reconnect = true;
                    warn!("[MessageStore] MySQL pool missing, switching to in-memory record store.");
                }
            }
        }

        if need_reconnect {
            self.schedule_mysql_reconnect().await;
        }

        // Store record in memory buffer
        let mut mem = self.mysql_memory_store.lock().await;
        mem.insert(record.message_id.clone(), record.clone());
        debug!("[MessageStore] Message record stored in memory buffer: {}", record.message_id);
        Ok(())
    }

    /// Retrieve a message record from MySQL by message_id
    pub async fn get_message_record(&self, message_id: &str) -> Result<Option<MessageRecord>> {
        {
            let state = self.mysql_state.lock().await;
            if let Some(pool) = &state.pool {
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
                        return Ok(Some(record))
                    }
                    Ok(None) => {
                        debug!("[MessageStore] Message record not found in MySQL: {}", message_id);
                        // Fall through to memory buffer lookup below
                    }
                    Err(e) => {
                        error!("[MessageStore] Failed to retrieve message record from MySQL: {}", e);
                        // Fall through to memory buffer lookup below
                    }
                }
            }
        }

        // Fallback to memory buffer
        let mem = self.mysql_memory_store.lock().await;
        Ok(mem.get(message_id).cloned())
    }

    /// Get a message by ID
    pub async fn get_message(&self, message_id: &str) -> Option<String> {
        let mut need_reconnect = false;

        {
            let mut state = self.redis_state.lock().await;
            if !state.use_memory {
                if let Some(conn) = state.conn.as_mut() {
                    match conn.get::<_, Option<String>>(message_id).await {
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
                    match conn.get::<_, Option<String>>(message_id).await {
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
        {
            let state = self.mysql_state.lock().await;
            if let Some(pool) = state.pool.as_ref() {
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
        }

        // Try memory buffer for MySQL records
        let mem = self.mysql_memory_store.lock().await;
        if let Some(rec) = mem.get(message_id) {
            return Some(rec.content.clone());
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
    use chrono::Local;

    #[tokio::test]
    async fn test_memory_store() {
        let store = MessageStore::new(None, None, None, None, None, None).await;
        store.store_message("id1", "hello").await;
        let val = store.get_message("id1").await;
        assert_eq!(val, Some("hello".to_string()));
    }

    #[tokio::test]
    async fn test_memory_store_overwrite() {
        let store = MessageStore::new(None, None, None, None, None, None).await;
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
        let store = MessageStore::new(redis_url.as_deref(), None, Some(3), Some(1), None, None).await;
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
        let store = MessageStore::new(None, mysql_url.as_deref(), None, None, Some(3), Some(1)).await;
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
