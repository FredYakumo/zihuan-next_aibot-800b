use log::{info, error};
use std::sync::Arc;
use std::future::Future;
use std::pin::Pin;

use super::models::{MessageEvent, MessageType};
use crate::bot_adapter::adapter::SharedBotAdapter;

/// Process messages (both private and group)
pub async fn process_message(bot_adapter: SharedBotAdapter, event: MessageEvent) {
    let messages: Vec<String> = event.message_list.iter()
        .map(|m| m.to_string())
        .collect();
    
    // Log based on message type
    match event.message_type {
        MessageType::Private => {
            info!(
                "[Friend Message] [Sender: {}({})] Message: {:?}",
                event.sender.nickname,
                event.sender.user_id,
                messages
            );
        }
        MessageType::Group => {
            info!(
                "[Group Message] [Group: {}({})] [Sender: {}({})] Message: {:?}",
                event.group_name.as_deref().unwrap_or_default(),
                event.group_id.unwrap_or_default(),
                event.sender.nickname,
                event.sender.user_id,
                messages
            );
        }
    }

    let handlers = {
        let bot_adapter_guard = bot_adapter.lock().await;
        bot_adapter_guard.get_event_handlers()
    };

    for handler in handlers {
        (handler)(&event).await;
    }

    let brain_agent = {
        let bot_adapter_guard = bot_adapter.lock().await;
        bot_adapter_guard.get_brain_agent().cloned()
    };

    if let Some(brain) = brain_agent {
        let bot_adapter_clone = bot_adapter.clone();
        tokio::spawn(async move {
            let mut bot_adapter_guard = bot_adapter_clone.lock().await;
            if let Err(e) = brain.on_event(&mut bot_adapter_guard, &event) {
                error!("[Brain Agent] Error processing event: {}", e);
            }
        });
    }
}

/// Event handler type alias
pub type EventHandler = Arc<
    dyn for<'a> Fn(&'a MessageEvent) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>
        + Send
        + Sync,
>;
