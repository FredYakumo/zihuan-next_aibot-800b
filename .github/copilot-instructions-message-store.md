# MessageStore - Message Persistence and Caching

## Overview
The `MessageStore` provides a multi-tier message storage system with the following backends:
1. **Redis** - Fast in-memory cache for recent messages (primary)
2. **MySQL** - Persistent long-term storage via `message_record` table
3. **In-Memory HashMap** - Fallback when Redis/MySQL unavailable

## Configuration

### Environment Variables / config.yaml

```yaml
# Redis Configuration (optional)
REDIS_HOST: redis
REDIS_PORT: 6379
REDIS_DB: 0
# REDIS_PASSWORD: yourpassword  # Optional

# MySQL Configuration (optional)
MYSQL_HOST: 127.0.0.1
MYSQL_PORT: 3306
MYSQL_USER: zihuan_user
MYSQL_PASSWORD: your_mysql_password
MYSQL_DATABASE: zihuan_aibot
```

Alternatively, set environment variables:
```bash
export REDIS_URL=redis://localhost:6379/0
export DATABASE_URL=mysql://zihuan_user:password@127.0.0.1:3306/zihuan_aibot
```

## API

### Basic Message Storage
```rust
// Store a plain text message
store.store_message("msg_id_123", "Hello, World!").await;

// Retrieve a message (tries Redis → MySQL → Memory)
let message = store.get_message("msg_id_123").await;
```

### Full Message Record Persistence
```rust
use crate::util::message_store::{MessageStore, MessageRecord};
use chrono::Local;

// Create a full message record
let record = MessageRecord {
    message_id: "msg_001".to_string(),
    sender_id: "qq_123456".to_string(),
    sender_name: "Alice".to_string(),
    send_time: Local::now().naive_local(),
    group_id: Some("group_789".to_string()),
    group_name: Some("Dev Team".to_string()),
    content: "Check this code review".to_string(),
    at_target_list: "@bob,@charlie".to_string(),
};

// Persist to MySQL
let result = store.store_message_record(&record).await;
match result {
    Ok(_) => println!("Message persisted successfully"),
    Err(e) => eprintln!("Persistence failed: {}", e),
}

// Retrieve from MySQL
let retrieved = store.get_message_record("msg_001").await;
match retrieved {
    Ok(Some(record)) => println!("Found: {:?}", record),
    Ok(None) => println!("Message not found"),
    Err(e) => eprintln!("Retrieval failed: {}", e),
}
```

### Enhanced Retrieval with MySQL Fallback
```rust
// Try Redis first, then MySQL, then Memory
let message = store.get_message_with_mysql("msg_id_123").await;
```

## Database Schema

The `message_record` table in MySQL stores:
```sql
CREATE TABLE message_record (
    id INT PRIMARY KEY AUTO_INCREMENT,
    message_id VARCHAR(64) NOT NULL,
    sender_id VARCHAR(64) NOT NULL,
    sender_name VARCHAR(128) NOT NULL,
    send_time DATETIME NOT NULL,
    group_id VARCHAR(64),
    group_name VARCHAR(128),
    content VARCHAR(2048) NOT NULL,
    at_target_list VARCHAR(512) NOT NULL
);
```

## Initialization in BotAdapter

The `MessageStore` is initialized in [src/bot_adapter/adapter.rs](../src/bot_adapter/adapter.rs):
```rust
let message_store = Arc::new(TokioMutex::new(
    MessageStore::new(redis_url.as_deref(), database_url.as_deref()).await
));
```

## Error Handling

**Graceful Degradation:**
- If Redis fails → Falls back to MySQL
- If MySQL fails → Falls back to in-memory HashMap
- All failures are logged with `[MessageStore]` prefix for easy filtering

**Example Log Output:**
```
[MessageStore] Connected to Redis at redis://localhost:6379/0
[MessageStore] Connected to MySQL at mysql://user:***@localhost:3306/zihuan_aibot
[MessageStore] Message stored in Redis: msg_001
[MessageStore] Message record persisted to MySQL: msg_001
[MessageStore] Message retrieved from MySQL: msg_001
```

## Testing

### Unit Tests
```bash
# Test in-memory store (always works)
cargo test message_store::tests::test_memory_store

# Test with Redis (requires REDIS_URL env var)
export REDIS_URL=redis://localhost:6379
cargo test message_store::tests::test_redis_store

# Test with MySQL (requires DATABASE_URL env var)
export DATABASE_URL=mysql://user:pass@localhost:3306/zihuan_aibot
cargo test message_store::tests::test_mysql_store
```

## Notes

- **DateTime Handling**: Messages use `NaiveDateTime` (local timezone-naive) compatible with MySQL's DATETIME type
- **Thread-Safety**: `MessageStore` is wrapped in `Arc<TokioMutex<>>` for safe concurrent access
- **Connection Pooling**: MySQL uses SQLx's built-in connection pool for efficient resource management
- **Encoding**: MySQL passwords with special characters are automatically percent-encoded per RFC 3986
