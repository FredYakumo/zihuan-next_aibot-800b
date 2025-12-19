use log::{info, error, debug};
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use std::future::Future;
use std::pin::Pin;
use chrono::Local;

use super::models::MessageEvent;
use crate::util::message_store::{MessageStore, MessageRecord};

/// Process private (friend) messages
pub async fn process_friend_message(event: &MessageEvent, store: Arc<TokioMutex<MessageStore>>) {
    let messages: Vec<String> = event.message_list.iter()
        .map(|m| m.to_string())
        .collect();
    
    info!(
        "Friend Message - Sender: {}, Message: {:?}",
        event.sender.user_id,
        messages
    );

    // Store full message record to MySQL
    let record = MessageRecord {
        message_id: event.message_id.to_string(),
        sender_id: event.sender.user_id.to_string(),
        sender_name: event.sender.nickname.clone(),
        send_time: Local::now().naive_local(),
        group_id: None,
        group_name: None,
        content: messages.join(" "),
        at_target_list: String::new(),
    };

    let store_guard = store.lock().await;
    if let Err(e) = store_guard.store_message_record(&record).await {
        error!("[Event] Failed to persist friend message record: {}", e);
    } else {
        debug!("[Event] Friend message record persisted: {}", record.message_id);
    }
}

/// Process group messages
pub async fn process_group_message(event: &MessageEvent, store: Arc<TokioMutex<MessageStore>>) {
    let messages: Vec<String> = event.message_list.iter()
        .map(|m| m.to_string())
        .collect();
    
    info!(
        "Group Message - Sender: {}, Message: {:?}",
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
        group_id: None,  // TODO: Extract group_id from event if available
        group_name: None,  // TODO: Extract group_name from event if available
        content: messages.join(" "),
        at_target_list: at_target_list.join(","),
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
