use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::error::Result;
use crate::node::{DataValue, Node, NodeGraph, Port};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NodeGraphDefinition {
    pub nodes: Vec<NodeDefinition>,
    pub edges: Vec<EdgeDefinition>,
    #[serde(skip)]
    pub execution_results: HashMap<String, HashMap<String, DataValue>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDefinition {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub node_type: String,
    pub input_ports: Vec<Port>,
    pub output_ports: Vec<Port>,
    pub position: Option<GraphPosition>,
    pub size: Option<GraphSize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeDefinition {
    pub from_node_id: String,
    pub from_port: String,
    pub to_node_id: String,
    pub to_port: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphPosition {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphSize {
    pub width: f32,
    pub height: f32,
}

pub fn load_graph_definition_from_json(path: impl AsRef<Path>) -> Result<NodeGraphDefinition> {
    let content = fs::read_to_string(path.as_ref())?;
    let graph: NodeGraphDefinition = serde_json::from_str(&content)?;
    Ok(graph)
}

pub fn save_graph_definition_to_json(
    path: impl AsRef<Path>,
    graph: &NodeGraphDefinition,
) -> Result<()> {
    let content = serde_json::to_string_pretty(graph)?;
    fs::write(path.as_ref(), content)?;
    Ok(())
}

pub fn ensure_positions(graph: &mut NodeGraphDefinition) {
    let spacing_x = 220.0;
    let spacing_y = 140.0;
    let cols = 4usize;

    for (index, node) in graph.nodes.iter_mut().enumerate() {
        if node.position.is_none() {
            let col = (index % cols) as f32;
            let row = (index / cols) as f32;
            node.position = Some(GraphPosition {
                x: 40.0 + col * spacing_x,
                y: 40.0 + row * spacing_y,
            });
        }
    }
}

pub fn build_definition_from_graph(graph: &NodeGraph) -> NodeGraphDefinition {
    let mut nodes = Vec::with_capacity(graph.nodes.len());
    for (id, node) in &graph.nodes {
        nodes.push(node_to_definition(id, node.as_ref()));
    }

    let mut output_producers: HashMap<String, String> = HashMap::new();
    for (node_id, node) in &graph.nodes {
        for port in node.output_ports() {
            output_producers.insert(port.name, node_id.clone());
        }
    }

    let mut edges = Vec::new();
    for (node_id, node) in &graph.nodes {
        for port in node.input_ports() {
            if let Some(producer) = output_producers.get(&port.name) {
                if producer != node_id {
                    edges.push(EdgeDefinition {
                        from_node_id: producer.clone(),
                        from_port: port.name.clone(),
                        to_node_id: node_id.clone(),
                        to_port: port.name.clone(),
                    });
                }
            }
        }
    }

    NodeGraphDefinition { 
        nodes, 
        edges,
        execution_results: HashMap::new(),
    }
}

fn node_to_definition(id: &str, node: &dyn Node) -> NodeDefinition {
    NodeDefinition {
        id: id.to_string(),
        name: node.name().to_string(),
        description: node.description().map(|s| s.to_string()),
        node_type: format!("{:?}", node.node_type()),
        input_ports: node.input_ports(),
        output_ports: node.output_ports(),
        position: None,
        size: None,
    }
}

impl NodeGraphDefinition {
    pub fn from_node_graph(graph: &NodeGraph) -> Self {
        build_definition_from_graph(graph)
    }

    pub fn to_json_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}
