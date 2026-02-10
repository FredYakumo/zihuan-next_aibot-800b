pub mod preview_string;
pub mod string_data;
pub mod preview_message_list;
pub mod message_list_data;

use crate::node::graph_io::NodeGraphDefinition;
use std::collections::HashMap;
use serde_json::Value;

/// Trait for nodes with custom rendering
pub trait NodeRenderer {
    /// Get the preview text to display in the node card
    fn get_preview_text(
        node_id: &str,
        graph: &NodeGraphDefinition,
        inline_inputs: &HashMap<String, InlinePortValue>,
    ) -> String;
    
    /// Check if this renderer should be used for the given node type
    fn handles_node_type(node_type: &str) -> bool;
}

#[derive(Debug, Clone)]
pub enum InlinePortValue {
    Text(String),
    Bool(bool),
    Json(Value),
}

/// Get preview text for any node with custom rendering
pub fn get_node_preview_text(
    node_id: &str,
    node_type: &str,
    graph: &NodeGraphDefinition,
    inline_inputs: &HashMap<String, InlinePortValue>,
) -> String {
    if preview_string::PreviewStringRenderer::handles_node_type(node_type) {
        return preview_string::PreviewStringRenderer::get_preview_text(node_id, graph, inline_inputs);
    }
    
    if string_data::StringDataRenderer::handles_node_type(node_type) {
        return string_data::StringDataRenderer::get_preview_text(node_id, graph, inline_inputs);
    }
    
    if preview_message_list::PreviewMessageListRenderer::handles_node_type(node_type) {
        return preview_message_list::PreviewMessageListRenderer::get_preview_text(node_id, graph, inline_inputs);
    }

    if message_list_data::MessageListDataRenderer::handles_node_type(node_type) {
        return message_list_data::MessageListDataRenderer::get_preview_text(node_id, graph, inline_inputs);
    }
    
    String::new()
}

/// Check if a node type has custom rendering
pub fn has_custom_rendering(node_type: &str) -> bool {
    preview_string::PreviewStringRenderer::handles_node_type(node_type)
        || string_data::StringDataRenderer::handles_node_type(node_type)
        || preview_message_list::PreviewMessageListRenderer::handles_node_type(node_type)
        || message_list_data::MessageListDataRenderer::handles_node_type(node_type)
}

pub fn inline_port_key(node_id: &str, port_name: &str) -> String {
    format!("{node_id}::{port_name}")
}
