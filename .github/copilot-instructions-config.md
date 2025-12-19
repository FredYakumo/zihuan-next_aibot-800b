# Copilot Instructions: src/config.rs

## Purpose
Centralized configuration loading with priority chain: YAML file → environment variables → defaults. Ensures bot always starts with valid required settings.

## Configuration Structure
```rust
pub struct Config {
    pub bot_server_url: String,        // Required (default: ws://localhost:3001)
    pub bot_server_token: String,      // Required (default: empty)
    pub redis_host: Option<String>,    // Optional Redis host
    pub redis_port: Option<u16>,       // Optional Redis port
    pub redis_db: Option<u8>,          // Optional Redis database
    pub redis_password: Option<String>,// Optional Redis password
    pub redis_url: Option<String>,     // Full Redis URL override
}
```

## Loading Priority Chain
```
1. Load config.yaml (if exists and valid YAML)
   ↓
2. For empty fields, check environment variables
   BOT_SERVER_URL → defaults to "ws://localhost:3001"
   BOT_SERVER_TOKEN → defaults to ""
   ↓
3. Return fully initialized Config with no empty strings
```

## Key Functions

### load_config() → Config
- Tries to read and parse `config.yaml` from current directory
- If parse fails or file missing, initializes empty Config
- Applies environment variable overrides to empty fields
- Guarantees `bot_server_url` and `bot_server_token` are non-empty strings
- Logs info/error messages for debugging

**Called from**: `src/main.rs` main function during startup

### build_redis_url(config: &Config) → Option<String>
Priority for building Redis URL:
```
1. config.redis_url (if set, use as-is)
   ↓
2. Environment variable REDIS_URL
   ↓
3. Construct from config.redis_host + redis_port + redis_db + redis_password
   - Uses pct_encode() on password if present
   - Returns None if insufficient parameters
```

**Example output**: `redis://:encoded%40password@localhost:6379/0`

### pct_encode(input: &str) → String
RFC 3986 percent-encoding for Redis password special characters:
- Keeps alphanumeric, `-`, `.`, `_`, `~` as-is
- Encodes everything else as `%HH` hex pairs
- **Use case**: Passwords containing `@`, `#`, `:`, or other URL-unsafe chars

**Example**: `my@pass#` → `my%40pass%23`

## Integration Points

### main.rs usage
```rust
use config::{load_config, build_redis_url};

let config = load_config();  // Guaranteed non-empty bot_server_url/token
let redis_url = build_redis_url(&config);  // Option<String>

// Pass to BotAdapter
let adapter = BotAdapter::new(config.bot_server_url, config.bot_server_token, redis_url, qq_id).await;
```

### config.yaml.example template
Provides developers with:
```yaml
BOT_SERVER_URL: ws://your-bot-server:3001
BOT_SERVER_TOKEN: your_bearer_token_here
REDIS_HOST: localhost
REDIS_PORT: 6379
REDIS_DB: 0
REDIS_PASSWORD: ""  # Leave empty if no auth
# OR use full URL:
# REDIS_URL: redis://user:password@localhost:6379/0
```

## Error Handling
- **Invalid YAML**: Logs error, uses empty Config, applies env/defaults
- **Missing redis_url parameters**: `build_redis_url()` returns None (MessageStore falls back to memory)
- **Empty bot_server_url after load**: Bot cannot connect; adapter receives default URL

## Extension Points
1. **Add new config field**:
   - Add to `Config` struct with proper type
   - Update `load_config()` to initialize from YAML or env
   - Update `config.yaml.example` documentation

2. **Change default values**:
   - Modify unwrap_or/unwrap_or_else in `load_config()`
   - Update both logic and `config.yaml.example`

3. **Add validation**:
   - Implement `Config::validate() → Result<(), String>` for complex rules
   - Call in `main.rs` after `load_config()`
   Example: Check bot_server_url is valid WebSocket URI
