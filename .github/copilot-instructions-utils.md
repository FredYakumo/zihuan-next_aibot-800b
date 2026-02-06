# Copilot Instructions: src/util/ and src/main.rs

## Purpose
Configuration loading, logging setup, and message storage utilities.

---

## src/main.rs: Configuration Loading

### Configuration Structure

```rust
#[derive(Debug, Deserialize)]
struct Config {
    #[serde(rename = "BOT_SERVER_URL")]
    bot_server_url: Option<String>,
    #[serde(rename = "BOT_SERVER_TOKEN")]
    bot_server_token: Option<String>,
    #[serde(rename = "REDIS_HOST")]
    redis_host: Option<String>,
    #[serde(rename = "REDIS_PORT")]
    redis_port: Option<u16>,
    #[serde(rename = "REDIS_DB")]
    redis_db: Option<u8>,
    #[serde(rename = "REDIS_PASSWORD")]
    redis_password: Option<String>,
    #[serde(rename = "REDIS_URL")]
    redis_url: Option<String>,
}
```

### Loading Pattern
```rust
fn load_config() -> Config {
    match fs::read_to_string("config.yaml") {
        Ok(content) => serde_yaml::from_str(&content).unwrap_or_default(),
        Err(_) => Config { /* defaults */ },
    }
}
```

### Redis URL Construction with Password Encoding
```rust
fn pct_encode(input: &str) -> String {
    // Percent-encode everything except unreserved characters: ALPHA / DIGIT / '-' / '.' / '_' / '~'
    let mut out = String::new();
    for &b in input.as_bytes() {
        let c = b as char;
        if c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_' || c == '~' {
            out.push(c);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

let redis_url = if let (Some(host), Some(port)) = (config.redis_host, config.redis_port) {
    let db = config.redis_db.unwrap_or(0);
    let password = config.redis_password.as_deref().unwrap_or("");
    if !password.is_empty() {
        let enc = pct_encode(password);
        Some(format!("redis://:{}@{}:{}/{}", enc, host, port, db))
    } else {
        Some(format!("redis://{}:{}/{}", host, port, db))
    }
} else {
    None
};
```

### Initialization Flow
1. Looks for `config.yaml` in project root
2. If missing or parse error → Uses defaults and fallback to environment variables
3. Constructs Redis URL with percent-encoded password for special characters
4. Passes configuration to `BotAdapter::new()`

**Key insight**: Special characters in passwords (like `@`, `#`) must be percent-encoded for Redis URLs.

---

## Logging via LogUtil

### Initialization

```rust
use log_util::log_util::LogUtil;
use lazy_static::lazy_static;

lazy_static! {
    static ref BASE_LOG: LogUtil = LogUtil::new_with_path("zihuan_next", "logs");
}

#[tokio::main]
async fn main() {
    LogUtil::init_with_logger(&BASE_LOG).expect("Failed to initialize logger");
    info!("Application starting...");
}
```

### Features
- **File logging**: Logs to `./logs` directory
- **Console output**: Logs to stdout/stderr
- **Standard log levels**: Uses `log` crate macros

### Usage Pattern
```rust
use log::{debug, info, warn, error};

debug!("Detailed diagnostic info");
info!("Normal operation");
warn!("Non-critical issue");
error!("Operation failed");
```

### Log Macros
```rust
info!("Connecting to server at {}", url);
warn!("Redis connection failed: {}", e);
error!("Failed to parse message: {}", err);
debug!("Received message: {:?}", message);
```

---

## src/util/message_store.rs

### Storage Strategy

**Dual backend**:
- **Production**: Redis (persistent, shared across processes)
- **Development fallback**: In-memory HashMap (single process only)

### Initialization

```rust
use redis::Client;
use std::collections::HashMap;

pub struct MessageStore {
    redis_client: Option<Client>,
    memory_store: HashMap<String, String>,
}

impl MessageStore {
    pub async fn new(redis_url: Option<&str>) -> Self {
        let redis_client = if let Some(url) = redis_url {
            match Client::open(url) {
                Ok(client) => {
                    // Test connection
                    match client.get_connection() {
                        Ok(_) => {
                            info!("Connected to Redis");
                            Some(client)
                        }
                        Err(e) => {
                            warn!("Failed to connect to Redis: {}. Using memory cache.", e);
                            None
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to open Redis client: {}. Using memory cache.", e);
                    None
                }
            }
        } else {
            warn!("No Redis URL provided. Using memory cache (NOT for production!)");
            None
        };

        Self {
            redis_client,
            memory_store: HashMap::new(),
        }
    }
}
```

### API

#### store_message
```rust
pub async fn store_message(&mut self, message_id: &str, message: &str) {
    if let Some(client) = &self.redis_client {
        match client.get_connection() {
            Ok(mut conn) => {
                if let Err(e) = redis::cmd("SET")
                    .arg(message_id)
                    .arg(message)
                    .query::<()>(&mut conn)
                {
                    warn!("Failed to store in Redis: {}. Storing in memory.", e);
                    self.memory_store.insert(message_id.to_string(), message.to_string());
                }
            }
            Err(e) => {
                warn!("Redis connection error: {}. Storing in memory.", e);
                self.memory_store.insert(message_id.to_string(), message.to_string());
            }
        }
    } else {
        self.memory_store.insert(message_id.to_string(), message.to_string());
    }
}
```

#### get_message
```rust
pub async fn get_message(&self, message_id: &str) -> Option<String> {
    if let Some(client) = &self.redis_client {
        match client.get_connection() {
            Ok(mut conn) => {
                match redis::cmd("GET").arg(message_id).query::<String>(&mut conn) {
                    Ok(value) => return Some(value),
                    Err(_) => {}
                }
            }
            Err(_) => {}
        }
    }
    self.memory_store.get(message_id).cloned()
}
```

### Usage Pattern

```rust
// In BotAdapter initialization
let message_store = Arc::new(TokioMutex::new(MessageStore::new(redis_url.as_deref()).await));

// In event processing (async spawn to avoid blocking)
let store = self.message_store.clone();
let msg_id = message_id.to_string();
let msg_str = serde_json::to_string(&raw_event).unwrap_or_default();
tokio::spawn(async move {
    let mut store = store.lock().await;
    store.store_message(&msg_id, &msg_str).await;
});

// In handlers (retrieving context)
let store = message_store.lock().await;
if let Some(original) = store.get_message(&reply_message_id).await {
    // Process original JSON string
}
```

### Production Considerations

**Redis required for**:
- Multi-process deployments
- Cross-server message sharing
- Persistence across restarts

**Memory cache acceptable for**:
- Local development
- Single-process testing
- Short-lived sessions

**Warning**: Memory cache logs warning `NOT suitable for production!` — ensure Redis is configured before deploying.

---

## Integration Points

- **Config**: Loaded in `src/main.rs` from `config.yaml`
- **Logger**: `LogUtil` initialized in `src/main.rs`, accessed via `log` crate macros
- **Message Store**: Created in `BotAdapter::new()`, wrapped in `Arc<TokioMutex<>>` for async access
- **Redis URL**: Constructed in `src/main.rs` with percent-encoded password, passed to `MessageStore::new()`
