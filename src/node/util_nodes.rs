use crate::error::Result;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use std::sync::RwLock;
use once_cell::sync::Lazy;

// Global context for string_data nodes to access UI input values
pub static STRING_DATA_CONTEXT: Lazy<RwLock<HashMap<String, String>>> = 
    Lazy::new(|| RwLock::new(HashMap::new()));

pub struct ConditionalNode {
    id: String,
    name: String,
}

impl ConditionalNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for ConditionalNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Conditional branching based on input condition")
    }

    node_input![
        port! { name = "condition", ty = Boolean, desc = "Condition to evaluate" },
        port! { name = "true_value", ty = Json, desc = "Value to output if condition is true" },
        port! { name = "false_value", ty = Json, desc = "Value to output if condition is false" },
    ];

    node_output![
        port! { name = "result", ty = Json, desc = "Selected value based on condition" },
        port! { name = "branch_taken", ty = String, desc = "Which branch was taken" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();

        if let Some(DataValue::Boolean(condition)) = inputs.get("condition") {
            let (result, branch) = if *condition {
                (
                    inputs.get("true_value").cloned().unwrap_or(DataValue::Json(serde_json::json!(null))),
                    "true",
                )
            } else {
                (
                    inputs.get("false_value").cloned().unwrap_or(DataValue::Json(serde_json::json!(null))),
                    "false",
                )
            };

            outputs.insert("result".to_string(), result);
            outputs.insert("branch_taken".to_string(), DataValue::String(branch.to_string()));
        }

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

pub struct JsonParserNode {
    id: String,
    name: String,
}

pub struct PreviewStringNode {
    id: String,
    name: String,
}

pub struct StringDataNode {
    id: String,
    name: String,
}

pub struct PreviewMessageListNode {
    id: String,
    name: String,
}

pub struct MessageListDataNode {
    id: String,
    name: String,
}

impl JsonParserNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl PreviewStringNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl StringDataNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl PreviewMessageListNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl MessageListDataNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for JsonParserNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Parse JSON string to structured data")
    }

    node_input![
        port! { name = "json_string", ty = String, desc = "JSON string to parse" },
    ];

    node_output![
        port! { name = "parsed", ty = Json, desc = "Parsed JSON object" },
        port! { name = "success", ty = Boolean, desc = "Whether parsing was successful" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();

        if let Some(DataValue::String(json_str)) = inputs.get("json_string") {
            match serde_json::from_str::<serde_json::Value>(json_str) {
                Ok(parsed) => {
                    outputs.insert("parsed".to_string(), DataValue::Json(parsed));
                    outputs.insert("success".to_string(), DataValue::Boolean(true));
                }
                Err(_) => {
                    outputs.insert("parsed".to_string(), DataValue::Json(serde_json::json!(null)));
                    outputs.insert("success".to_string(), DataValue::Boolean(false));
                }
            }
        }

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

impl Node for PreviewStringNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Preview input string inside the node card")
    }

    node_input![
        port! { name = "text", ty = String, desc = "Text to preview inside the node", optional },
    ];

    node_output![];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();
        if let Some(value) = inputs.get("text") {
            outputs.insert("text".to_string(), value.clone());
        }

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

impl Node for StringDataNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("String data source with UI input field")
    }

    node_input![];

    node_output![
        port! { name = "text", ty = String, desc = "Output string from UI input" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        // StringDataNode gets its value from the global context (set by UI layer before execution)
        let mut outputs = HashMap::new();
        let value = {
            let context = STRING_DATA_CONTEXT.read().unwrap();
            context.get(&self.id).cloned().unwrap_or_default()
        };
        outputs.insert("text".to_string(), DataValue::String(value));
        
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

impl Node for PreviewMessageListNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Preview MessageList inside the node card with scrollable message items")
    }

    node_input![
        port! { name = "messages", ty = MessageList, desc = "MessageList to preview inside the node", optional },
    ];

    node_output![];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();
        if let Some(value) = inputs.get("messages") {
            outputs.insert("messages".to_string(), value.clone());
        }

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

impl Node for MessageListDataNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("MessageList data source with inline UI editor")
    }

    // We intentionally keep a MessageList *input* port so inline_values can persist into the
    // graph JSON and be parsed into DataValue::MessageList by the registry.
    // The port is optional to avoid validation errors when the node is created before editing.
    node_input![
        port! { name = "messages", ty = MessageList, desc = "MessageList provided by UI inline editor", optional },
    ];

    node_output![
        port! { name = "messages", ty = MessageList, desc = "Output MessageList from UI data source" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();
        let value = match inputs.get("messages") {
            Some(DataValue::MessageList(list)) => DataValue::MessageList(list.clone()),
            _ => DataValue::MessageList(Vec::new()),
        };
        outputs.insert("messages".to_string(), value);

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
