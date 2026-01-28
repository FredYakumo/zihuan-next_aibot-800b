use serde_json::{json, Value};
use serde::Serialize;
use std::collections::HashMap;
use crate::error::Result;

pub mod data_value;
pub mod util_nodes;

pub use data_value::{DataType, DataValue};

/// Node input/output ports
#[derive(Debug, Clone, Serialize)]
pub struct Port {
    pub name: String,
    pub data_type: DataType,
    pub description: Option<String>,
    /// Whether this port is required, only for input ports
    pub required: bool,
}

impl Port {
    pub fn new(name: impl Into<String>, data_type: DataType) -> Self {
        Self {
            name: name.into(),
            data_type,
            description: None,
            required: true,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }
}

/// Node trait
pub trait Node: Send + Sync {
    fn id(&self) -> &str;


    fn name(&self) -> &str;


    fn description(&self) -> Option<&str> {
        None
    }

    fn input_ports(&self) -> Vec<Port>;

    fn output_ports(&self) -> Vec<Port>;

    /// Execute the node's main logic
    /// inputs: input port name -> data value
    /// returns: output port name -> data value
    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>>;

    fn to_json(&self) -> Value {
        json!({
            "id": self.id(),
            "name": self.name(),
            "description": self.description(),
            "input_ports": serde_json::to_value(&self.input_ports()).unwrap_or(Value::Null),
            "output_ports": serde_json::to_value(&self.output_ports()).unwrap_or(Value::Null),
        })
    }

    fn validate_inputs(&self, inputs: &HashMap<String, DataValue>) -> Result<()> {
        let input_ports = self.input_ports();
        
        for port in &input_ports {
            match inputs.get(&port.name) {
                Some(value) => {
                    // Validate data type
                    if value.data_type() != port.data_type {
                        return Err(crate::error::Error::ValidationError(format!(
                            "Input port '{}' expects type {}, got {}",
                            port.name,
                            port.data_type,
                            value.data_type()
                        )));
                    }
                }
                None => {
                    if port.required {
                        return Err(crate::error::Error::ValidationError(format!(
                            "Required input port '{}' is missing",
                            port.name
                        )));
                    }
                }
            }
        }
        
        Ok(())
    }

    fn validate_outputs(&self, outputs: &HashMap<String, DataValue>) -> Result<()> {
        let output_ports = self.output_ports();
        
        for port in &output_ports {
            if let Some(value) = outputs.get(&port.name) {
                if value.data_type() != port.data_type {
                    return Err(crate::error::Error::ValidationError(format!(
                        "Output port '{}' expects type {}, got {}",
                        port.name,
                        port.data_type,
                        value.data_type()
                    )));
                }
            }
        }
        
        Ok(())
    }
}

/// NodeGraph manages multiple nodes
pub struct NodeGraph {
    pub nodes: HashMap<String, Box<dyn Node>>,
}

impl NodeGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
        }
    }

    pub fn add_node(&mut self, node: Box<dyn Node>) -> Result<()> {
        let id = node.id().to_string();
        if self.nodes.contains_key(&id) {
            return Err(crate::error::Error::ValidationError(format!(
                "Node with id '{}' already exists",
                id
            )));
        }
        self.nodes.insert(id, node);
        Ok(())
    }

    pub fn execute(&mut self) -> Result<()> {
        let mut output_producers: HashMap<String, String> = HashMap::new();
        for (node_id, node) in &self.nodes {
            for port in node.output_ports() {
                if let Some(existing) = output_producers.insert(port.name.clone(), node_id.clone()) {
                    return Err(crate::error::Error::ValidationError(format!(
                        "Output port '{}' is produced by both '{}' and '{}'",
                        port.name, existing, node_id
                    )));
                }
            }
        }

        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();

        for node_id in self.nodes.keys() {
            in_degree.insert(node_id.clone(), 0);
        }

        for (node_id, node) in &self.nodes {
            for port in node.input_ports() {
                if let Some(producer) = output_producers.get(&port.name) {
                    if producer != node_id {
                        dependents.entry(producer.clone()).or_default().push(node_id.clone());
                        if let Some(count) = in_degree.get_mut(node_id) {
                            *count += 1;
                        }
                    }
                } else if port.required {
                    return Err(crate::error::Error::ValidationError(format!(
                        "Required input port '{}' for node '{}' is not bound",
                        port.name, node_id
                    )));
                }
            }
        }

        let mut ready: Vec<String> = in_degree
            .iter()
            .filter_map(|(id, degree)| if *degree == 0 { Some(id.clone()) } else { None })
            .collect();
        ready.sort();

        let mut ordered: Vec<String> = Vec::with_capacity(self.nodes.len());
        while !ready.is_empty() {
            let node_id = ready.remove(0);
            ordered.push(node_id.clone());

            if let Some(next_nodes) = dependents.get(&node_id) {
                for next_id in next_nodes {
                    if let Some(count) = in_degree.get_mut(next_id) {
                        *count = count.saturating_sub(1);
                        if *count == 0 {
                            ready.push(next_id.clone());
                        }
                    }
                }
                ready.sort();
            }
        }

        if ordered.len() != self.nodes.len() {
            return Err(crate::error::Error::ValidationError(
                "Cycle detected in node dependencies".to_string(),
            ));
        }

        let mut data_pool: HashMap<String, DataValue> = HashMap::new();

        for node_id in ordered {
            let node = self.nodes.get_mut(&node_id).ok_or_else(|| {
                crate::error::Error::ValidationError(format!(
                    "Node '{}' not found during execution",
                    node_id
                ))
            })?;

            let mut inputs: HashMap<String, DataValue> = HashMap::new();
            for port in node.input_ports() {
                if let Some(value) = data_pool.get(&port.name) {
                    inputs.insert(port.name.clone(), value.clone());
                } else if port.required {
                    return Err(crate::error::Error::ValidationError(format!(
                        "Required input port '{}' for node '{}' is missing",
                        port.name, node_id
                    )));
                }
            }

            let outputs = node.execute(inputs)?;
            for (key, value) in outputs {
                if data_pool.contains_key(&key) {
                    return Err(crate::error::Error::ValidationError(format!(
                        "Output key '{}' from node '{}' conflicts with existing data",
                        key, node_id
                    )));
                }
                data_pool.insert(key, value);
            }
        }

        Ok(())
    }

    pub fn to_json(&self) -> Value {
        json!({
            "nodes": self.nodes.iter().map(|(id, node)| {
                json!({
                    "id": id,
                    "node": node.to_json(),
                })
            }).collect::<Vec<_>>(),
        })
    }
}

impl Default for NodeGraph {
    fn default() -> Self {
        Self::new()
    }
}