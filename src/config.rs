use std::fs;
use serde::Deserialize;
use log::{info, error};

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(rename = "natural_language_model_api")]
    pub natural_language_model_api: Option<String>,
    #[serde(rename = "natural_language_model_api_key")]
    pub natural_language_model_api_key: Option<String>,
    #[serde(rename = "natural_language_model_name")]
    pub natural_language_model_name: Option<String>,
    #[serde(rename = "agent_model_api")]
    pub agent_model_api: Option<String>,
    #[serde(rename = "agent_model_api_key")]
    pub agent_model_api_key: Option<String>,
    #[serde(rename = "agent_model_name")]
    pub agent_model_name: Option<String>,
}

/// Load configuration from config.yaml file (LLM settings only)
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
                        natural_language_model_api: None,
                        natural_language_model_api_key: None,
                        natural_language_model_name: None,
                        agent_model_api: None,
                        agent_model_api_key: None,
                        agent_model_name: None,
                    }
                }
            }
        }
        Err(e) => {
            info!("Could not read config.yaml ({}), using environment variables", e);
            Config {
                natural_language_model_api: None,
                natural_language_model_api_key: None,
                natural_language_model_name: None,
                agent_model_api: None,
                agent_model_api_key: None,
                agent_model_name: None,
            }
        }
    };

    // LLM configs - apply environment variable overrides
    if config.natural_language_model_api.is_none() {
        config.natural_language_model_api = std::env::var("natural_language_model_api").ok();
    }

    if config.natural_language_model_api_key.is_none() {
        config.natural_language_model_api_key = std::env::var("natural_language_model_api_key").ok();
    }

    if config.natural_language_model_name.is_none() {
        config.natural_language_model_name = std::env::var("natural_language_model_name").ok();
    }

    if config.agent_model_api.is_none() {
        config.agent_model_api = std::env::var("agent_model_api").ok();
    }

    if config.agent_model_api_key.is_none() {
        config.agent_model_api_key = std::env::var("agent_model_api_key").ok();
    }

    if config.agent_model_name.is_none() {
        config.agent_model_name = std::env::var("agent_model_name").ok();
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
