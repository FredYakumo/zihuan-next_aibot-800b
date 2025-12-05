use tracing::info;

use super::models::MessageEvent;

/// Process private (friend) messages
pub fn process_friend_message(event: &MessageEvent) {
    let messages: Vec<String> = event.message_list.iter()
        .map(|m| m.to_string())
        .collect();
    
    info!(
        "Friend Message - Sender: {}, Message: {:?}",
        event.sender.user_id,
        messages
    );
}

/// Process group messages
pub fn process_group_message(event: &MessageEvent) {
    let messages: Vec<String> = event.message_list.iter()
        .map(|m| m.to_string())
        .collect();
    
    info!(
        "Group Message - Sender: {}, Message: {:?}",
        event.sender.user_id,
        messages
    );
}

/// Event handler type alias
pub type EventHandler = fn(&MessageEvent);
