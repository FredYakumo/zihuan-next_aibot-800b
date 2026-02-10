use crate::node::graph_io::NodeGraphDefinition;
use super::{NodeRenderer, InlinePortValue, inline_port_key};
use std::collections::HashMap;

pub struct StringDataRenderer;

impl NodeRenderer for StringDataRenderer {
    fn get_preview_text(
        node_id: &str,
        _graph: &NodeGraphDefinition,
        inline_inputs: &HashMap<String, InlinePortValue>,
    ) -> String {
        // Get preview text from inline input (the UI text field)
        let key = inline_port_key(node_id, "text");
        match inline_inputs.get(&key) {
            Some(InlinePortValue::Text(value)) => format!("{}", value),
            Some(InlinePortValue::Bool(value)) => format!("{}", value),
            Some(InlinePortValue::Json(_)) => "(json)".to_string(),
            None => "(empty...)".to_string(),
        }
    }
    
    fn handles_node_type(node_type: &str) -> bool {
        node_type == "string_data"
    }
}
