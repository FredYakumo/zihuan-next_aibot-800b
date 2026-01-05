use super::FunctionTool;
use crate::llm::{LLMBase, InferenceParam, Message, MessageRole};
use crate::error::Result;
use serde_json::{json, Value};
use std::sync::Arc;

/// Natural-language reply tool: delegates to the LLM to produce a response.
///
/// Parameters:
/// - prompt (string, required): user input to respond to
/// - system (string, optional): system prompt to steer style/behavior
#[derive(Clone)]
pub struct NaturalLanguageReplyTool {
    llm: Arc<dyn LLMBase + Send + Sync>,
}

impl NaturalLanguageReplyTool {
    pub fn new(llm: Arc<dyn LLMBase + Send + Sync>) -> Self { Self { llm } }
}

impl FunctionTool for NaturalLanguageReplyTool {
    fn name(&self) -> &str { "nl_reply" }

    fn description(&self) -> &str {
        "Use the language model to craft a natural-language reply given a prompt."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": { "type": "string", "description": "User prompt to respond to" },
                "system": { "type": "string", "description": "Optional system prompt to steer tone and style" }
            },
            "required": ["prompt"],
            "additionalProperties": false
        })
    }

    fn call(&self, arguments: Value) -> Result<Value> {
        let prompt = arguments
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::string_error!("missing required parameter: prompt"))?;
        let system = arguments.get("system").and_then(|v| v.as_str()).unwrap_or("You are a helpful assistant.");

        let messages = vec![
            Message { role: MessageRole::System, content: Some(system.to_string()), tool_calls: Vec::new() },
            Message { role: MessageRole::User, content: Some(prompt.to_string()), tool_calls: Vec::new() },
        ];
        let param = InferenceParam { messages: &messages, tools: None };
        let resp = self.llm.inference(&param);
        let content = resp.content.unwrap_or_default();
        Ok(json!({ "reply": content }))
    }
}

// Agent implementation moved to llm::agent::function_tool_agents
