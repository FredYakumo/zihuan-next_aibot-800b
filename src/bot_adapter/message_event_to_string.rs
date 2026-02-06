use log::info;

use crate::bot_adapter::models::message::MessageProp;
use crate::error::Result;
use crate::node::{DataType, DataValue, Node, Port};
use std::collections::HashMap;

/// Node that converts a MessageEvent to an LLM prompt string
/// 
/// Inputs:
///   - message_event: MessageEvent containing message data
/// 
/// Outputs:
///   - prompt: String containing the formatted LLM prompt
pub struct MessageEventToStringNode {
    id: String,
    name: String,
}

impl MessageEventToStringNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for MessageEventToStringNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Converts MessageEvent to LLM prompt string")
    }

    fn input_ports(&self) -> Vec<Port> {
        vec![
            Port::new("message_event", DataType::MessageEvent)
                .with_description("MessageEvent containing message data"),
        ]
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![
            Port::new("prompt", DataType::String)
                .with_description("Formatted LLM prompt string"),
        ]
    }

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();

        if let Some(DataValue::MessageEvent(event)) = inputs.get("message_event") {
            let msg_prop = MessageProp::from_messages(&event.message_list, None);

            // Build user message from incoming MessageEvent
            let mut user_text = msg_prop.content.clone().unwrap_or_default();
            if let Some(ref_cnt) = msg_prop.ref_content.as_deref() {
                if !ref_cnt.is_empty() {
                    if !user_text.is_empty() {
                        user_text.push_str("\n\n");
                    }
                    user_text.push_str("[引用内容]\n");
                    user_text.push_str(ref_cnt);
                }
            }
            if user_text.trim().is_empty() {
                user_text = "(无文本内容，可能是仅@或回复)".to_string();
            }

            outputs.insert("prompt".to_string(), DataValue::String(user_text));
        } else {
            return Err("message_event input is required and must be MessageEvent type".into());
        }
        info!("MessageEventToStringNode generated promp2t: {}", outputs.get("prompt").map(|v| v.to_json().to_string()).unwrap_or_default());
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_event_to_string_node_creation() {
        let node = MessageEventToStringNode::new("msg_to_str_1", "MessageEventToString");
        assert_eq!(node.id(), "msg_to_str_1");
        assert_eq!(node.name(), "MessageEventToString");
    }

    #[test]
    fn test_input_output_ports() {
        let node = MessageEventToStringNode::new("test", "test");
        let input_ports = node.input_ports();
        let output_ports = node.output_ports();

        assert_eq!(input_ports.len(), 1);
        assert_eq!(input_ports[0].name, "message_event");

        assert_eq!(output_ports.len(), 1);
        assert_eq!(output_ports[0].name, "prompt");
    }
}
