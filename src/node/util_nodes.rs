use crate::error::Result;
use crate::node::{DataType, DataValue, Node, Port};
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

    fn input_ports(&self) -> Vec<Port> {
        vec![
            Port::new("condition", DataType::Boolean)
                .with_description("Condition to evaluate"),
            Port::new("true_value", DataType::Json)
                .with_description("Value to output if condition is true"),
            Port::new("false_value", DataType::Json)
                .with_description("Value to output if condition is false"),
        ]
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![
            Port::new("result", DataType::Json)
                .with_description("Selected value based on condition"),
            Port::new("branch_taken", DataType::String)
                .with_description("Which branch was taken"),
        ]
    }

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

    fn input_ports(&self) -> Vec<Port> {
        vec![
            Port::new("json_string", DataType::String)
                .with_description("JSON string to parse"),
        ]
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![
            Port::new("parsed", DataType::Json)
                .with_description("Parsed JSON object"),
            Port::new("success", DataType::Boolean)
                .with_description("Whether parsing was successful"),
        ]
    }

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

    fn input_ports(&self) -> Vec<Port> {
        vec![
            Port::new("text", DataType::String)
                .with_description("Text to preview inside the node")
                .optional(),
        ]
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![

        ]
    }

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

    fn input_ports(&self) -> Vec<Port> {
        vec![]
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![
            Port::new("text", DataType::String)
                .with_description("Output string from UI input"),
        ]
    }

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