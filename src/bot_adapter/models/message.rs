use serde::{Deserialize, Serialize};
use std::fmt;

/// Base trait for all message types
pub trait MessageBase: fmt::Display + fmt::Debug + Send + Sync {
    fn get_type(&self) -> &'static str;
}

/// Enum representing all possible message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Message {
    #[serde(rename = "text")]
    PlainText(PlainTextMessage),
    #[serde(rename = "at")]
    At(AtTargetMessage),
    #[serde(rename = "reply")]
    Reply(ReplyMessage),
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Message::PlainText(msg) => write!(f, "{}", msg),
            Message::At(msg) => write!(f, "{}", msg),
            Message::Reply(msg) => write!(f, "{}", msg),
        }
    }
}

impl MessageBase for Message {
    fn get_type(&self) -> &'static str {
        match self {
            Message::PlainText(_) => "text",
            Message::At(_) => "at",
            Message::Reply(_) => "reply",
        }
    }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlainTextMessage {
    pub text: String,
}

impl fmt::Display for PlainTextMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.text)
    }
}

impl MessageBase for PlainTextMessage {
    fn get_type(&self) -> &'static str {
        "text"
    }
}

/// @ mention message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtTargetMessage {
    #[serde(alias = "qq")]
    pub target: Option<i64>,
}

impl AtTargetMessage {
    pub fn target_id(&self) -> i64 {
        self.target.unwrap_or(0)
    }
}

impl fmt::Display for AtTargetMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{}", self.target_id())
    }
}

impl MessageBase for AtTargetMessage {
    fn get_type(&self) -> &'static str {
        "at"
    }
}

/// Reply message (references another message)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyMessage {
    pub id: i64,
    #[serde(skip)]
    pub message_source: Option<Box<Message>>,
}

impl fmt::Display for ReplyMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref source) = self.message_source {
            write!(f, "[Reply of message ID {}: {}]", self.id, source)
        } else {
            write!(f, "[Reply of message ID {}]", self.id)
        }
    }
}

impl MessageBase for ReplyMessage {
    fn get_type(&self) -> &'static str {
        "reply"
    }
}

/// Raw message data structure from JSON
#[derive(Debug, Clone, Deserialize)]
pub struct RawMessageData {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub data: serde_json::Value,
}

/// Convert raw JSON message to typed Message
pub fn convert_message_from_json(raw: &RawMessageData) -> Result<Message, String> {
    match raw.msg_type.as_str() {
        "text" => {
            let text = raw.data.get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Ok(Message::PlainText(PlainTextMessage { text }))
        }
        "at" => {
            let target = raw.data.get("target")
                .or_else(|| raw.data.get("qq"))
                .and_then(|v| v.as_i64());
            Ok(Message::At(AtTargetMessage { target }))
        }
        "reply" | "replay" => {
            let id = raw.data.get("id")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            Ok(Message::Reply(ReplyMessage {
                id,
                message_source: None,
            }))
        }
        _ => Err(format!("Unsupported message type: {}", raw.msg_type)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text_message() {
        let msg = PlainTextMessage { text: "Hello".to_string() };
        assert_eq!(msg.to_string(), "Hello");
        assert_eq!(msg.get_type(), "text");
    }

    #[test]
    fn test_at_target_message() {
        let msg = AtTargetMessage { target: Some(12345) };
        assert_eq!(msg.to_string(), "@12345");
        assert_eq!(msg.get_type(), "at");
    }

    #[test]
    fn test_reply_message() {
        let msg = ReplyMessage { id: 123, message_source: None };
        assert_eq!(msg.to_string(), "[Reply of message ID 123]");
        assert_eq!(msg.get_type(), "reply");
    }
}
