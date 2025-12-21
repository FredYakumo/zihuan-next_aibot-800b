use log::{info, error, debug};
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use std::future::Future;
use std::pin::Pin;
use chrono::Local;

use super::models::MessageEvent;
use crate::{bot_adapter::adapter::SharedBotAdapter, util::message_store::{MessageRecord, MessageStore}};

/// Process private (friend) messages
pub async fn process_friend_message(bot_adapter: SharedBotAdapter, event: MessageEvent, store: Arc<TokioMutex<MessageStore>>) {
    let messages: Vec<String> = event.message_list.iter()
        .map(|m| m.to_string())
        .collect();
    
    info!(
        "[Friend Message] [Sender: {}({})] Message: {:?}",
        event.sender.nickname,
        event.sender.user_id,
        messages
    );


    let bot_id = {
        let bot_adapter_guard = bot_adapter.lock().await;
        bot_adapter_guard.get_bot_id().to_string()
    };

    // Store full message record to MySQL
    let record = MessageRecord {
        message_id: event.message_id.to_string(),
        sender_id: event.sender.user_id.to_string(),
        sender_name: event.sender.nickname.clone(),
        send_time: Local::now().naive_local(),
        group_id: None,
        group_name: None,
        content: messages.join(" "),
        at_target_list: Some(bot_id),
    };

    let store_guard = store.lock().await;
    if let Err(e) = store_guard.store_message_record(&record).await {
        error!("[Event] Failed to persist friend message record: {}", e);
    } else {
        debug!("[Event] Friend message record persisted: {}", record.message_id);
    }
}

/// Process group messages
pub async fn process_group_message(_bot_adapter: SharedBotAdapter, event: MessageEvent, store: Arc<TokioMutex<MessageStore>>) {
    let messages: Vec<String> = event.message_list.iter()
        .map(|m| m.to_string())
        .collect();
    
    info!(
        "[Group Message] [Group: {}({})] [Sender: {}({})] Message: {:?}",
        event.group_name.as_deref().unwrap_or_default(),
        event.group_id.unwrap_or_default(),
        event.sender.nickname,
        event.sender.user_id,
        messages
    );

    // Extract @mentions
    let at_target_list: Vec<String> = event.message_list.iter()
        .filter_map(|m| {
            if let crate::bot_adapter::models::message::Message::At(at_msg) = m {
                at_msg.target.map(|id| id.to_string())
            } else {
                None
            }
        })
        .collect();

    // Store full message record to MySQL
    let record = MessageRecord {
        message_id: event.message_id.to_string(),
        sender_id: event.sender.user_id.to_string(),
        sender_name: if !event.sender.card.is_empty() {
            event.sender.card.clone()
        } else {
            event.sender.nickname.clone()
        },
        send_time: Local::now().naive_local(),
        group_id: event.group_id.map(|id| id.to_string()),
        group_name: event.group_name.clone(),
        content: messages.join(" "),
        at_target_list: if at_target_list.is_empty() {
            None
        } else {
            Some(at_target_list.join(","))
        },
    };

    let store_guard = store.lock().await;
    if let Err(e) = store_guard.store_message_record(&record).await {
        error!("[Event] Failed to persist group message record: {}", e);
    } else {
        debug!("[Event] Group message record persisted: {}", record.message_id);
    }
}

/// Event handler type alias
pub type EventHandler = for<'a> fn(&'a MessageEvent, Arc<TokioMutex<MessageStore>>) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
