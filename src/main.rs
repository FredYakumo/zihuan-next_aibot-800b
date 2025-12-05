mod bot_adapter;
mod util;

use std::fs;
use serde::Deserialize;
use log::{info, error, warn};
use log_util::log_util::LogUtil;
use lazy_static::lazy_static;

use bot_adapter::adapter::BotAdapter;

lazy_static! {
    static ref BASE_LOG: LogUtil = LogUtil::new_with_path("zihuan_next_aibot", "logs");
}

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

fn load_config() -> Config {
    // Try to load from config.yaml
    match fs::read_to_string("config.yaml") {
        Ok(content) => {
            match serde_yaml::from_str(&content) {
                Ok(config) => {
                    info!("Loaded configuration from config.yaml");
                    config
                }
                Err(e) => {
                    error!("Failed to parse config.yaml: {}", e);
                    Config {
                        bot_server_url: None,
                        bot_server_token: None,
                        redis_host: None,
                        redis_port: None,
                        redis_db: None,
                        redis_password: None,
                        redis_url: None,
                    }
                }
            }
        }
        Err(e) => {
            info!("Could not read config.yaml ({}), using environment variables", e);
            Config {
                bot_server_url: None,
                bot_server_token: None,
                redis_host: None,
                redis_port: None,
                redis_db: None,
                redis_password: None,
                redis_url: None,
            }
        }
    }
}

/// Percent-encode a password for safe inclusion in a URL
fn pct_encode(input: &str) -> String {
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

#[tokio::main]
async fn main() {
    // Initialize logging using LogUtil
    LogUtil::init_with_logger(&BASE_LOG).expect("Failed to initialize logger");

    info!("zihuan_next_aibot-800b starting...");

    // Load configuration from config.yaml, fallback to environment variables
    let config = load_config();

    // Set REDIS_URL env var if not already set, using config.yaml fields
    if std::env::var("REDIS_URL").is_err() {
        let redis_url = if let Some(url) = config.redis_url.clone() {
            Some(url)
        } else if let (Some(host), Some(port)) = (config.redis_host.as_ref(), config.redis_port) {
            let db = config.redis_db.unwrap_or(0);
            let password = config.redis_password.as_deref().unwrap_or("");
            if !password.is_empty() {
                // Percent-encode password to safely include special characters like @ and #
                let enc = pct_encode(password);
                Some(format!("redis://:{}@{}:{}/{}", enc, host, port, db))
            } else {
                Some(format!("redis://{}:{}/{}", host, port, db))
            }
        } else {
            None
        };
        if let Some(url) = redis_url {
            std::env::set_var("REDIS_URL", url);
            info!("Set REDIS_URL from config.yaml");
        } else {
            warn!("No REDIS_URL or REDIS_HOST/PORT found in config.yaml; Redis will not be used.");
        }
    }

    let bot_server_url = config.bot_server_url
        .or_else(|| std::env::var("BOT_SERVER_URL").ok())
        .unwrap_or_else(|| "ws://localhost:3001".to_string());

    let bot_server_token = config.bot_server_token
        .or_else(|| std::env::var("BOT_SERVER_TOKEN").ok())
        .unwrap_or_default();

    // Create and start the bot adapter
    let adapter = BotAdapter::new(bot_server_url, bot_server_token).await;
    info!("Bot adapter initialized, connecting to server...");
    if let Err(e) = adapter.start().await {
        error!("Bot adapter error: {}", e);
    }
}
