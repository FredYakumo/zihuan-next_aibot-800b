use serde_json::{Value, json};
use std::sync::Arc;
use crate::llm::LLMBase;

pub trait FunctionTool: Send + Sync {
    fn name(&self) -> & str;
    fn description(&self) -> & str;

    /// JSON Schema-like parameters definition.
    ///
    /// Example:
    /// {"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}
    fn parameters(&self) -> Value;

    fn get_json(&self) -> Value {
        json!({
            "name": self.name(),
            "description": self.description(),
            "parameters": self.parameters(),
        })
    }

    /// Tool execute function
    fn call(&self, arguments: Value) -> Result<Value, String>;
}

#[derive(Debug)]
pub struct ToolCallsFuncSpec {
    pub name: String,
    pub arguments: Value
}

#[derive(Debug)]
pub struct ToolCalls {
    pub id: String,
    pub type_name: String,
    pub function: ToolCallsFuncSpec,
}

pub mod math;
pub mod chat_history;
pub mod nl_reply;
pub mod code_writer;

pub use math::MathTool;
pub use chat_history::ChatHistoryTool;
pub use nl_reply::NaturalLanguageReplyTool;
pub use code_writer::CodeWriterTool;

/// Returns a default set of built-in tools for the BrainAgent
pub fn default_tools(llm: Arc<dyn LLMBase + Send + Sync>) -> Vec<Arc<dyn FunctionTool>> {
    vec![
        Arc::new(ChatHistoryTool::new()),
        Arc::new(NaturalLanguageReplyTool::new(llm.clone())),
        Arc::new(CodeWriterTool::new(llm)),
        Arc::new(MathTool::new()),
    ]
}

