use crate::node::graph_io::NodeGraphDefinition;
use crate::node::DataValue;
use super::{NodeRenderer, InlinePortValue};
use std::collections::HashMap;

pub struct PreviewStringRenderer;

impl NodeRenderer for PreviewStringRenderer {
    fn get_preview_text(
        node_id: &str,
        graph: &NodeGraphDefinition,
        _inline_inputs: &HashMap<String, InlinePortValue>,
    ) -> String {
        // Get preview text from execution results
        graph.execution_results.get(node_id)
            .and_then(|results| results.get("text"))
            .and_then(|value| match value {
                DataValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }
    
    fn handles_node_type(node_type: &str) -> bool {
        node_type == "preview_string"
    }
}
