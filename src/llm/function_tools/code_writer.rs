use super::FunctionTool;
use crate::llm::{LLMBase, InferenceParam, Message, MessageRole};
use serde_json::{json, Value};
use std::sync::Arc;

/// Code writer tool: ask the LLM to produce code for a given task/spec.
///
/// Parameters:
/// - task (string, required): description of the code to write
/// - language (string, optional): preferred language (e.g., "python", "rust", "javascript")
/// - constraints (string, optional): any constraints or requirements
#[derive(Clone)]
pub struct CodeWriterTool {
    llm: Arc<dyn LLMBase + Send + Sync>,
}

impl CodeWriterTool {
    pub fn new(llm: Arc<dyn LLMBase + Send + Sync>) -> Self { Self { llm } }
}

impl FunctionTool for CodeWriterTool {
    fn name(&self) -> &str { "code_writer" }

    fn description(&self) -> &str {
        "Ask the language model to generate code according to a task description. Returns the code as text."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task": { "type": "string", "description": "Description of the code to write" },
                "language": { "type": "string", "description": "Preferred programming language" },
                "constraints": { "type": "string", "description": "Constraints/requirements (tests, style, libraries)" }
            },
            "required": ["task"],
            "additionalProperties": false
        })
    }

    fn call(&self, arguments: Value) -> Result<Value, String> {
        let task = arguments
            .get("task")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "missing required parameter: task".to_string())?;
        let language = arguments.get("language").and_then(|v| v.as_str()).unwrap_or("");
        let constraints = arguments.get("constraints").and_then(|v| v.as_str()).unwrap_or("");

        let system = "You are a senior software engineer. Generate clean, correct, and well-commented code. If possible, include a brief usage example.";
        let user_prompt = if language.is_empty() && constraints.is_empty() {
            format!("Task: {task}\nPlease provide the code.")
        } else {
            format!(
                "Task: {task}\nLanguage: {language}\nConstraints: {constraints}\nPlease provide the code.")
        };

        let messages = vec![
            Message { role: MessageRole::System, content: Some(system.to_string()), tool_calls: Vec::new() },
            Message { role: MessageRole::User, content: Some(user_prompt), tool_calls: Vec::new() },
        ];
        let param = InferenceParam { messages, tools: None };
        let resp = self.llm.inference(&param);
        let content = resp.content.unwrap_or_default();
        Ok(json!({ "code": content }))
    }
}
