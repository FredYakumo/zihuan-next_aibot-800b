use serde_json::{Value, json};
use crate::error::Result;

pub trait FunctionTool: Send + Sync + std::fmt::Debug {
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
    fn call(&self, arguments: Value) -> Result<Value>;
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolCallsFuncSpec {
    pub name: String,
    pub arguments: Value
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolCalls {
    pub id: String,
    pub type_name: String,
    pub function: ToolCallsFuncSpec,
}

pub mod math;
pub mod chat_history;
pub mod nl_reply;
pub mod code_writer;

#[allow(unused_imports)]
pub use math::MathTool;
#[allow(unused_imports)]
pub use chat_history::ChatHistoryTool;
#[allow(unused_imports)]
pub use nl_reply::NaturalLanguageReplyTool;
#[allow(unused_imports)]
pub use code_writer::CodeWriterTool;

