pub mod agent;
pub mod llm_api;
pub mod function_tools;

use crate::llm::function_tools::{FunctionTool, ToolCalls};
use std::sync::Arc;

#[derive(Debug)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Convert a MessageRole to the string expected by chat APIs
pub fn role_to_str(role: &MessageRole) -> &'static str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    }
}

/// Parse a role string from chat APIs into MessageRole
pub fn str_to_role(s: &str) -> MessageRole {
    match s {
        "system" => MessageRole::System,
        "user" => MessageRole::User,
        "tool" => MessageRole::Tool,
        _ => MessageRole::Assistant,
    }
}

pub struct Message {
    pub role: MessageRole,
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCalls>,
}

impl Message {
    /// Create a system message with the given content and no tool calls.
    pub fn system<S: Into<String>>(content: S) -> Self {
        Self {
            role: MessageRole::System,
            content: Some(content.into()),
            tool_calls: Vec::new(),
        }
    }

    /// Create a user message with the given content and no tool calls.
    pub fn user<S: Into<String>>(content: S) -> Self {
        Self {
            role: MessageRole::User,
            content: Some(content.into()),
            tool_calls: Vec::new(),
        }
    }
}

/// Shortcut to construct a system message.
pub fn SystemMessage<S: Into<String>>(content: S) -> Message {
    Message::system(content)
}

/// Shortcut to construct a user message.
pub fn UserMessage<S: Into<String>>(content: S) -> Message {
    Message::user(content)
}

pub struct InferenceParam {
    pub messages: Vec<Message>,
    pub tools: Option<Vec<Arc<dyn FunctionTool>>>,
}

pub trait LLMBase {
    fn get_model_name(&self) -> &str;

    fn inference(&self, param: &InferenceParam) -> Message;
}