use serde::{Deserialize, Serialize};
use std::fmt;

use super::message::Message;

/// Message type enum (private or group chat)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageType {
    Private,
    Group,
}

impl fmt::Display for MessageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MessageType::Private => write!(f, "private"),
            MessageType::Group => write!(f, "group"),
        }
    }
}

impl MessageType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageType::Private => "private",
            MessageType::Group => "group",
        }
    }
}

/// Sender information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sender {
    pub user_id: i64,
    pub nickname: String,
    #[serde(default)]
    pub card: String,
    pub role: Option<String>,
}

/// Message event containing the full message information
#[derive(Debug, Clone)]
pub struct MessageEvent {
    pub message_id: i64,
    pub message_type: MessageType,
    pub sender: Sender,
    pub message_list: Vec<Message>,
}

/// Raw message event structure for deserialization
#[derive(Debug, Clone, Deserialize)]
pub struct RawMessageEvent {
    pub message_id: i64,
    pub message_type: MessageType,
    pub sender: Sender,
    #[serde(default)]
    pub message: Vec<super::message::RawMessageData>,
}
