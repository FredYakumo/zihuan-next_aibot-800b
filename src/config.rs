use std::fs;
use serde::Deserialize;
use log::{info, error, warn};

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(rename = "BOT_SERVER_URL")]
    pub bot_server_url: String,
    #[serde(rename = "BOT_SERVER_TOKEN")]
    pub bot_server_token: String,
    #[serde(rename = "REDIS_HOST")]
    pub redis_host: Option<String>,
    #[serde(rename = "REDIS_PORT")]
    pub redis_port: Option<u16>,
    #[serde(rename = "REDIS_DB")]
    pub redis_db: Option<u8>,
    #[serde(rename = "REDIS_PASSWORD")]
    pub redis_password: Option<String>,
    #[serde(rename = "REDIS_URL")]
    pub redis_url: Option<String>,
    #[serde(rename = "REDIS_RECONNECT_MAX_ATTEMPTS")]
    pub redis_reconnect_max_attempts: Option<u32>,
    #[serde(rename = "REDIS_RECONNECT_INTERVAL_SECS")]
    pub redis_reconnect_interval_secs: Option<u64>,
    #[serde(rename = "MYSQL_HOST")]
    pub mysql_host: Option<String>,
    #[serde(rename = "MYSQL_PORT")]
    pub mysql_port: Option<u16>,
    #[serde(rename = "MYSQL_USER")]
    pub mysql_user: Option<String>,
    #[serde(rename = "MYSQL_PASSWORD")]
    pub mysql_password: Option<String>,
    #[serde(rename = "MYSQL_DATABASE")]
    pub mysql_database: Option<String>,
    #[serde(rename = "DATABASE_URL")]
    pub database_url: Option<String>,
}

/// Load configuration from config.yaml file
pub fn load_config() -> Config {
    // Try to load from config.yaml
    let mut config = match fs::read_to_string("config.yaml") {
        Ok(content) => {
            match serde_yaml::from_str(&content) {
                Ok(config) => {
                    info!("Loaded configuration from config.yaml");
                    config
                }
                Err(e) => {
                    error!("Failed to parse config.yaml: {}", e);
                    Config {
                        bot_server_url: String::new(),
                        bot_server_token: String::new(),
                        redis_host: None,
                        redis_port: None,
                        redis_db: None,
                        redis_password: None,
                        redis_url: None,
                        redis_reconnect_max_attempts: None,
                        redis_reconnect_interval_secs: None,
                        mysql_host: None,
                        mysql_port: None,
                        mysql_user: None,
                        mysql_password: None,
                        mysql_database: None,
                        database_url: None,
                    }
                }
            }
        }
        Err(e) => {
            info!("Could not read config.yaml ({}), using environment variables", e);
            Config {
                bot_server_url: String::new(),
                bot_server_token: String::new(),
                redis_host: None,
                redis_port: None,
                redis_db: None,
                redis_password: None,
                redis_url: None,
                redis_reconnect_max_attempts: None,
                redis_reconnect_interval_secs: None,
                mysql_host: None,
                mysql_port: None,
                mysql_user: None,
                mysql_password: None,
                mysql_database: None,
                database_url: None,
            }
        }
    };

    // Apply defaults and environment variable overrides
    if config.bot_server_url.is_empty() {
        config.bot_server_url = std::env::var("BOT_SERVER_URL")
            .unwrap_or_else(|_| "ws://localhost:3001".to_string());
    }
    
    if config.bot_server_token.is_empty() {
        config.bot_server_token = std::env::var("BOT_SERVER_TOKEN").unwrap_or_default();
    }

    if config.redis_reconnect_max_attempts.is_none() {
        if let Ok(val) = std::env::var("REDIS_RECONNECT_MAX_ATTEMPTS") {
            match val.parse() {
                Ok(parsed) => config.redis_reconnect_max_attempts = Some(parsed),
                Err(e) => warn!("REDIS_RECONNECT_MAX_ATTEMPTS Not Found ({}), using default 3", e),
            }
        }
    }

    if config.redis_reconnect_interval_secs.is_none() {
        if let Ok(val) = std::env::var("REDIS_RECONNECT_INTERVAL_SECS") {
            match val.parse() {
                Ok(parsed) => config.redis_reconnect_interval_secs = Some(parsed),
                Err(e) => warn!("Failed to parse REDIS_RECONNECT_INTERVAL_SECS ({}), using default 60s", e),
            }
        }
    }

    if config.redis_reconnect_max_attempts.is_none() {
        config.redis_reconnect_max_attempts = Some(3);
    }

    if config.redis_reconnect_interval_secs.is_none() {
        config.redis_reconnect_interval_secs = Some(60);
    }
    
    config
}

/// Percent-encode a password for safe inclusion in a URL
pub fn pct_encode(input: &str) -> String {
    // Encode everything except unreserved characters per RFC 3986: ALPHA / DIGIT / '-' / '.' / '_' / '~'
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

/// Build Redis URL from configuration
pub fn build_redis_url(config: &Config) -> Option<String> {
    if let Some(url) = config.redis_url.clone() {
        return Some(url);
    }
    
    if std::env::var("REDIS_URL").is_ok() {
        return std::env::var("REDIS_URL").ok();
    }
    
    if let (Some(host), Some(port)) = (config.redis_host.as_ref(), config.redis_port) {
        let db = config.redis_db.unwrap_or(0);
        let password = config.redis_password.as_deref().unwrap_or("");
        if !password.is_empty() {
            // Percent-encode password to safely include special characters like @ and #
            let enc = pct_encode(password);
            return Some(format!("redis://:{}@{}:{}/{}", enc, host, port, db));
        } else {
            return Some(format!("redis://{}:{}/{}", host, port, db));
        }
    }
    
    None
}

/// Build MySQL URL from configuration
pub fn build_mysql_url(config: &Config) -> Option<String> {
    if let Some(url) = config.database_url.clone() {
        return Some(url);
    }
    
    if std::env::var("DATABASE_URL").is_ok() {
        return std::env::var("DATABASE_URL").ok();
    }
    
    if let (Some(user), Some(host), Some(port), Some(database)) = (
        config.mysql_user.as_ref(),
        config.mysql_host.as_ref(),
        config.mysql_port,
        config.mysql_database.as_ref(),
    ) {
        let password = config.mysql_password.as_deref().unwrap_or("");
        if !password.is_empty() {
            let enc = pct_encode(password);
            return Some(format!(
                "mysql://{}:{}@{}:{}/{}",
                user, enc, host, port, database
            ));
        } else {
            return Some(format!("mysql://{}@{}:{}/{}", user, host, port, database));
        }
    }
    
    None
}
