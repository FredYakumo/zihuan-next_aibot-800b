use crate::node::graph_io::NodeGraphDefinition;
use super::{NodeRenderer, InlinePortValue, inline_port_key};
use std::collections::HashMap;

pub struct MessageListDataRenderer;

impl NodeRenderer for MessageListDataRenderer {
    fn get_preview_text(
        node_id: &str,
        _graph: &NodeGraphDefinition,
        inline_inputs: &HashMap<String, InlinePortValue>,
    ) -> String {
        let key = inline_port_key(node_id, "messages");
        match inline_inputs.get(&key) {
            Some(InlinePortValue::Json(serde_json::Value::Array(items))) => {
                format!("messages: {}", items.len())
            }
            _ => "messages: 0".to_string(),
        }
    }

    fn handles_node_type(node_type: &str) -> bool {
        node_type == "message_list_data"
    }
}
