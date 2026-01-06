use super::FunctionTool;
use crate::error::Result;
use crate::util::message_store::MessageStore;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use tokio::runtime::Handle;

/// Fetch chat history by sender_id and optional group_id from MessageStore.
///
/// Notes:
/// - Requires MessageStore to be provided at construction.
/// - Retrieves historical messages from MySQL via MessageStore.
/// - Uses blocking runtime to call async MessageStore methods from sync trait.
#[derive(Clone)]
pub struct ChatHistoryTool {
    message_store: Arc<TokioMutex<MessageStore>>,
}

impl ChatHistoryTool {
    pub fn new(message_store: Arc<TokioMutex<MessageStore>>) -> Self { 
        Self { message_store } 
    }
}

impl FunctionTool for ChatHistoryTool {
    fn name(&self) -> &str { "chat_history" }

    fn description(&self) -> &str {
        "Fetch chat history by sender_id and optional group_id. Returns recent messages ordered by time (newest first). Use this to understand conversation context."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "sender_id": { 
                    "type": "string", 
                    "description": "The QQ ID of the sender whose messages to fetch" 
                },
                "group_id": { 
                    "type": "string", 
                    "description": "Optional group ID to filter messages from a specific group. Omit for private chat messages." 
                },
                "limit": { 
                    "type": "integer", 
                    "description": "Number of messages to retrieve (default: 100, max: 1000)",
                    "default": 100
                }
            },
            "required": ["sender_id"],
            "additionalProperties": false
        })
    }

    fn call(&self, arguments: Value) -> Result<Value> {
        let sender_id = arguments
            .get("sender_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::string_error!("missing required parameter: sender_id"))?;

        let group_id = arguments
            .get("group_id")
            .and_then(|v| v.as_str());

        let limit = arguments
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(100) as u32;
        
        // Limit max to 1000 to prevent excessive queries
        let limit = limit.min(1000);

        // Use current runtime handle to block on async operation
        let handle = Handle::current();
        let store = self.message_store.clone();
        
        let result = handle.block_on(async move {
            let store_guard = store.lock().await;
            store_guard.get_messages_by_sender(sender_id, group_id, limit).await
        })?;

        // Format results as JSON array
        let messages: Vec<Value> = result
            .into_iter()
            .map(|record| {
                json!({
                    "message_id": record.message_id,
                    "sender_id": record.sender_id,
                    "sender_name": record.sender_name,
                    "send_time": record.send_time.format("%Y-%m-%d %H:%M:%S").to_string(),
                    "group_id": record.group_id,
                    "group_name": record.group_name,
                    "content": record.content,
                    "at_target_list": record.at_target_list,
                })
            })
            .collect();

        Ok(json!({
            "sender_id": sender_id,
            "group_id": group_id,
            "count": messages.len(),
            "messages": messages
        }))
    }
}

