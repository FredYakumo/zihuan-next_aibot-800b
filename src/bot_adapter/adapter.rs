
use std::collections::HashMap;
use futures_util::StreamExt;
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};
use tracing::{debug, error, info, warn};

use super::event::{self, EventHandler};
use super::models::{
    convert_message_from_json, MessageEvent, MessageType, RawMessageEvent,
};

/// BotAdapter connects to the QQ bot server via WebSocket and processes events
pub struct BotAdapter {
    url: String,
    token: String,
    event_handlers: HashMap<MessageType, EventHandler>,
}

impl BotAdapter {
    /// Create a new BotAdapter with the given WebSocket URL and authentication token
    pub fn new(url: impl Into<String>, token: impl Into<String>) -> Self {
        let mut event_handlers: HashMap<MessageType, EventHandler> = HashMap::new();
        event_handlers.insert(MessageType::Private, event::process_friend_message);
        event_handlers.insert(MessageType::Group, event::process_group_message);

        Self {
            url: url.into(),
            token: token.into(),
            event_handlers,
        }
    }

    /// Register a custom event handler for a specific message type
    pub fn register_handler(&mut self, message_type: MessageType, handler: EventHandler) {
        self.event_handlers.insert(message_type, handler);
    }

    /// Start the WebSocket connection and begin processing events
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Connecting to bot server at {}", self.url);

        // Build the WebSocket request with authorization header
        let request = http::Request::builder()
            .uri(&self.url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Host", extract_host(&self.url).unwrap_or("localhost"))
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header("Sec-WebSocket-Key", tokio_tungstenite::tungstenite::handshake::client::generate_key())
            .body(())?;

        let (ws_stream, _) = connect_async(request).await?;
        info!("Connected to the qq bot server successfully.");

        let (mut _write, mut read) = ws_stream.split();

        // Process incoming messages
        while let Some(msg_result) = read.next().await {
            match msg_result {
                Ok(WsMessage::Text(text)) => {
                    self.process_event(&text);
                }
                Ok(WsMessage::Binary(data)) => {
                    if let Ok(text) = String::from_utf8(data) {
                        self.process_event(&text);
                    } else {
                        warn!("Received binary message that is not valid UTF-8");
                    }
                }
                Ok(WsMessage::Close(_)) => {
                    info!("WebSocket connection closed");
                    break;
                }
                Ok(WsMessage::Ping(_)) | Ok(WsMessage::Pong(_)) => {
                    // Heartbeat messages, ignore
                }
                Ok(WsMessage::Frame(_)) => {
                    // Raw frame, ignore
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Process a single event message
    fn process_event(&self, message: &str) {
        debug!("Received message: {}", message);

        // Parse the JSON message
        let message_json: serde_json::Value = match serde_json::from_str(message) {
            Ok(v) => v,
            Err(e) => {
                error!("Failed to parse message as JSON: {}", e);
                return;
            }
        };

        // Check if this is a message event (has message_type field)
        if message_json.get("message_type").is_none() {
            debug!("Ignoring non-message event");
            return;
        }

        // Parse as RawMessageEvent
        let raw_event: RawMessageEvent = match serde_json::from_value(message_json) {
            Ok(e) => e,
            Err(e) => {
                error!("Failed to parse message event: {}", e);
                return;
            }
        };

        // Convert raw messages to typed messages
        let message_list: Vec<_> = raw_event.message
            .iter()
            .filter_map(|raw| {
                match convert_message_from_json(raw) {
                    Ok(msg) => Some(msg),
                    Err(e) => {
                        warn!("Failed to convert message: {}", e);
                        None
                    }
                }
            })
            .collect();

        // Create the MessageEvent
        let event = MessageEvent {
            message_id: raw_event.message_id,
            message_type: raw_event.message_type,
            sender: raw_event.sender,
            message_list,
        };

        // Dispatch to the appropriate handler
        if let Some(handler) = self.event_handlers.get(&event.message_type) {
            handler(&event);
        } else {
            warn!("No handler registered for message type: {}", event.message_type);
        }
    }
}

/// Extract host from URL for WebSocket handshake
fn extract_host(url: &str) -> Option<&str> {
    url.strip_prefix("ws://")
        .or_else(|| url.strip_prefix("wss://"))
        .and_then(|s| s.split('/').next())
        .and_then(|s| s.split(':').next())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_host() {
        assert_eq!(extract_host("ws://localhost:3001"), Some("localhost"));
        assert_eq!(extract_host("wss://example.com/path"), Some("example.com"));
        assert_eq!(extract_host("ws://192.168.1.1:8080/ws"), Some("192.168.1.1"));
    }
}
