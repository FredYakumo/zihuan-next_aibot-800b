use serde::{Deserialize, Serialize};
use serde::de::{self, Deserializer};
use std::fmt;

fn deserialize_i64_from_string_or_number<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(deserializer)?;
    match v {
        serde_json::Value::Number(n) => n
            .as_i64()
            .ok_or_else(|| de::Error::custom("numeric value is not an i64")),
        serde_json::Value::String(s) => s
            .parse::<i64>()
            .map_err(|e| de::Error::custom(format!("failed to parse i64 from string: {e}"))),
        other => Err(de::Error::custom(format!(
            "expected string or number for i64, got {other}" 
        ))),
    }
}

fn deserialize_option_i64_from_string_or_number<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<serde_json::Value>::deserialize(deserializer)?;
    match opt {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Number(n)) => Ok(n.as_i64()),
        Some(serde_json::Value::String(s)) => {
            let parsed = s
                .parse::<i64>()
                .map_err(|e| de::Error::custom(format!("failed to parse i64 from string: {e}")))?;
            Ok(Some(parsed))
        }
        Some(other) => Err(de::Error::custom(format!(
            "expected null/string/number for Option<i64>, got {other}"
        ))),
    }
}

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
    #[serde(rename = "reply", alias = "replay")]
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
    #[serde(default, deserialize_with = "deserialize_option_i64_from_string_or_number")]
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
    #[serde(deserialize_with = "deserialize_i64_from_string_or_number")]
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

/// Abstracts and encapsulates the raw messages received by the bot, refining them into structured fields convenient for LLM processing:
/// - `content`: The merged readable body (text/@/reply, etc.), used directly for feeding to the model
/// - `ref_content`: Contextual summary from reference/reply chains (e.g., replied content), used to supplement context
/// - `is_at_me`: Whether the message @'s the bot itself, facilitating priority/trigger judgment
/// - `at_target_list`: List of all @ targets in the message (QQ numbers, etc.), used for intent recognition and routing
pub struct MessageProp {
    pub content: Option<String>,
    pub ref_content: Option<String>,
    pub is_at_me: bool,
    pub at_target_list: Vec<i64>
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
    fn test_at_target_message_deserialize_string() {
        let v = json!({"qq": "24968"});
        let msg: AtTargetMessage = serde_json::from_value(v).unwrap();
        assert_eq!(msg.target_id(), 24968);
    }

    #[test]
    fn test_reply_message() {
        let msg = ReplyMessage { id: 123, message_source: None };
        assert_eq!(msg.to_string(), "[Reply of message ID 123]");
        assert_eq!(msg.get_type(), "reply");
    }

    #[test]
    fn test_reply_message_deserialize_string() {
        let v = json!({"id": "985732927"});
        let msg: ReplyMessage = serde_json::from_value(v).unwrap();
        assert_eq!(msg.id, 985732927);
    }

    #[test]
    fn test_message_deserialize_from_message_array_element() {
        // Matches the shape inside the top-level `message` array: {"type": "at", "data": {"qq": "..."}}
        let v = json!({"type": "at", "data": {"qq": "2496875785"}});
        let msg: Message = serde_json::from_value(v).unwrap();
        assert_eq!(msg.get_type(), "at");
        assert_eq!(msg.to_string(), "@2496875785");
    }
}
