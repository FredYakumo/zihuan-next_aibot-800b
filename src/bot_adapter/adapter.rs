
use std::collections::HashMap;
use futures_util::StreamExt;
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};
use log::{debug, error, info, warn};

use super::event::{self, EventHandler};
use super::models::{MessageEvent, MessageType, RawMessageEvent};
use crate::util::url_utils::extract_host;
use crate::util::message_store::MessageStore;
use std::env;
use tokio::sync::Mutex as TokioMutex;
use std::sync::Arc;

/// BotAdapter connects to the QQ bot server via WebSocket and processes events
pub struct BotAdapter {
    url: String,
    token: String,
    event_handlers: HashMap<MessageType, EventHandler>,
    message_store: Arc<TokioMutex<MessageStore>>,
}

impl BotAdapter {
    /// Create a new BotAdapter with the given WebSocket URL, authentication token, and optional Redis URL
    pub async fn new(url: impl Into<String>, token: impl Into<String>, redis_url: Option<String>) -> Self {
        let mut event_handlers: HashMap<MessageType, EventHandler> = HashMap::new();
        event_handlers.insert(MessageType::Private, event::process_friend_message);
        event_handlers.insert(MessageType::Group, event::process_group_message);

        // Use provided redis_url, fallback to env var
        let redis_url = redis_url.or_else(|| env::var("REDIS_URL").ok());
        let message_store = Arc::new(TokioMutex::new(MessageStore::new(redis_url.as_deref()).await));

        Self {
            url: url.into(),
            token: token.into(),
            event_handlers,
            message_store,
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

        // Create the MessageEvent (messages are already deserialized in RawMessageEvent)
        let event = MessageEvent {
            message_id: raw_event.message_id,
            message_type: raw_event.message_type,
            sender: raw_event.sender.clone(),
            message_list: raw_event.message.clone(),
        };

        // Store the message in the message store (async spawn)
        let store = self.message_store.clone();
        let msg_id = raw_event.message_id.to_string();
        let msg_str = serde_json::to_string(&raw_event).unwrap_or_default();
        tokio::spawn(async move {
            let mut store = store.lock().await;
            store.store_message(&msg_id, &msg_str).await;
        });

        // Dispatch to the appropriate handler
        self.event_handlers.get(&event.message_type)
            .map(|handler| handler(&event))
            .unwrap_or_else(|| warn!("No handler registered for message type: {}", event.message_type));
    }
}
