use crate::node::graph_io::NodeGraphDefinition;
use crate::node::DataValue;
use super::{NodeRenderer, InlinePortValue};
use std::collections::HashMap;

pub struct PreviewStringRenderer;

impl NodeRenderer for PreviewStringRenderer {
    fn get_preview_text(
        node_id: &str,
        graph: &NodeGraphDefinition,
        inline_inputs: &HashMap<String, InlinePortValue>,
    ) -> String {
        // Get preview text from execution results
        if let Some(results) = graph.execution_results.get(node_id) {
            if let Some(DataValue::String(s)) = results.get("text") {
                return s.clone();
            }
        }

        // Fallback to inline input if no execution result
        let key = super::inline_port_key(node_id, "text");
        if let Some(InlinePortValue::Text(s)) = inline_inputs.get(&key) {
            return s.clone();
        }

        String::new()
    }
    
    fn handles_node_type(node_type: &str) -> bool {
        node_type == "preview_string"
    }
}
