use serde_json::{json, Value};
/// NodeType enum for distinguishing node categories
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum NodeType {
    Simple,
    EventProducer,
}

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use crate::error::Result;

pub mod data_value;
pub mod util_nodes;
pub mod graph_io;
pub mod registry;
pub mod database_nodes;
pub mod message_nodes;

#[allow(unused_imports)]
pub use data_value::{DataType, DataValue};
#[allow(unused_imports)]
pub use graph_io::{
    NodeGraphDefinition,
    NodeDefinition,
    EdgeDefinition,
    GraphPosition,
    load_graph_definition_from_json,
    save_graph_definition_to_json,
    ensure_positions,
};

/// Node input/output ports
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Returns the type of the node
    fn node_type(&self) -> NodeType {
        NodeType::Simple
    }
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

    /// Event producer lifecycle: called before update loop
    fn on_start(&mut self, _inputs: HashMap<String, DataValue>) -> Result<()> {
        Ok(())
    }

    /// Event producer lifecycle: called repeatedly to produce outputs
    fn on_update(&mut self) -> Result<Option<HashMap<String, DataValue>>> {
        Ok(None)
    }

    /// Event producer lifecycle: called after update loop exits
    fn on_cleanup(&mut self) -> Result<()> {
        Ok(())
    }

    fn to_json(&self) -> Value {
        json!({
            "id": self.id(),
            "name": self.name(),
            "description": self.description(),
            "node_type": format!("{:?}", self.node_type()),
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
        let mut dependencies: HashMap<String, Vec<String>> = HashMap::new();

        for node_id in self.nodes.keys() {
            in_degree.insert(node_id.clone(), 0);
        }

        for (node_id, node) in &self.nodes {
            for port in node.input_ports() {
                if let Some(producer) = output_producers.get(&port.name) {
                    if producer != node_id {
                        dependencies.entry(node_id.clone()).or_default().push(producer.clone());
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

        let event_producer_set: HashSet<String> = self
            .nodes
            .iter()
            .filter_map(|(id, node)| {
                if node.node_type() == NodeType::EventProducer {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();

        if event_producer_set.is_empty() {
            let mut data_pool: HashMap<String, DataValue> = HashMap::new();
            for node_id in ordered {
                let node = self.nodes.get_mut(&node_id).ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        node_id
                    ))
                })?;

                let inputs = Self::collect_inputs(node.as_ref(), &data_pool, &node_id)?;
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

            return Ok(());
        }

        let mut reachable_from_event: HashSet<String> = HashSet::new();
        let mut reachable_map: HashMap<String, HashSet<String>> = HashMap::new();
        for event_id in &event_producer_set {
            let mut visited: HashSet<String> = HashSet::new();
            let mut stack: Vec<String> = vec![event_id.clone()];
            while let Some(current) = stack.pop() {
                if !visited.insert(current.clone()) {
                    continue;
                }
                if let Some(children) = dependents.get(&current) {
                    for child in children {
                        if !visited.contains(child) {
                            stack.push(child.clone());
                        }
                    }
                }
            }
            reachable_from_event.extend(visited.iter().cloned());
            reachable_map.insert(event_id.clone(), visited);
        }

        let mut base_data_pool: HashMap<String, DataValue> = HashMap::new();
        for node_id in &ordered {
            if reachable_from_event.contains(node_id) {
                continue;
            }

            let node = self.nodes.get_mut(node_id).ok_or_else(|| {
                crate::error::Error::ValidationError(format!(
                    "Node '{}' not found during execution",
                    node_id
                ))
            })?;

            let inputs = Self::collect_inputs(node.as_ref(), &base_data_pool, node_id)?;
            let outputs = node.execute(inputs)?;
            for (key, value) in outputs {
                if base_data_pool.contains_key(&key) {
                    return Err(crate::error::Error::ValidationError(format!(
                        "Output key '{}' from node '{}' conflicts with existing data",
                        key, node_id
                    )));
                }
                base_data_pool.insert(key, value);
            }
        }

        let mut event_producer_roots: Vec<String> = event_producer_set
            .iter()
            .filter(|event_id| {
                !dependencies
                    .get(*event_id)
                    .map(|deps| deps.iter().any(|dep| event_producer_set.contains(dep)))
                    .unwrap_or(false)
            })
            .cloned()
            .collect();
        event_producer_roots.sort();

        for root_id in event_producer_roots {
            self.run_event_producer(
                &root_id,
                &base_data_pool,
                &reachable_map,
                &event_producer_set,
                &ordered,
            )?;
        }

        Ok(())
    }

    /// Execute the graph and capture results for each node
    pub fn execute_and_capture_results(&mut self) -> Result<HashMap<String, HashMap<String, DataValue>>> {
        let mut node_results: HashMap<String, HashMap<String, DataValue>> = HashMap::new();
        
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
        let mut dependencies: HashMap<String, Vec<String>> = HashMap::new();

        for node_id in self.nodes.keys() {
            in_degree.insert(node_id.clone(), 0);
        }

        for (node_id, node) in &self.nodes {
            for port in node.input_ports() {
                if let Some(producer) = output_producers.get(&port.name) {
                    if producer != node_id {
                        dependencies.entry(node_id.clone()).or_default().push(producer.clone());
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

        let event_producer_set: HashSet<String> = self
            .nodes
            .iter()
            .filter_map(|(id, node)| {
                if node.node_type() == NodeType::EventProducer {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();

        if event_producer_set.is_empty() {
            let mut data_pool: HashMap<String, DataValue> = HashMap::new();
            for node_id in ordered {
                let node = self.nodes.get_mut(&node_id).ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        node_id
                    ))
                })?;

                let inputs = Self::collect_inputs(node.as_ref(), &data_pool, &node_id)?;
                let outputs = node.execute(inputs.clone())?;
                
                // Store both inputs and outputs for this node
                let mut result = inputs;
                result.extend(outputs.iter().map(|(k, v)| (k.clone(), v.clone())));
                node_results.insert(node_id.clone(), result);
                
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

            return Ok(node_results);
        }

        // For event producers, we still need to execute but won't capture all results
        self.execute()?;
        
        Ok(node_results)
    }

    fn collect_inputs(
        node: &dyn Node,
        data_pool: &HashMap<String, DataValue>,
        node_id: &str,
    ) -> Result<HashMap<String, DataValue>> {
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
        node.validate_inputs(&inputs)?;
        Ok(inputs)
    }

    fn run_event_producer(
        &mut self,
        node_id: &str,
        base_data_pool: &HashMap<String, DataValue>,
        reachable_map: &HashMap<String, HashSet<String>>,
        event_producer_set: &HashSet<String>,
        ordered: &[String],
    ) -> Result<()> {
        let reachable = reachable_map
            .get(node_id)
            .cloned()
            .unwrap_or_default();

        {
            let node = self.nodes.get_mut(node_id).ok_or_else(|| {
                crate::error::Error::ValidationError(format!(
                    "Node '{}' not found during execution",
                    node_id
                ))
            })?;

            let inputs = Self::collect_inputs(node.as_ref(), base_data_pool, node_id)?;
            node.on_start(inputs)?;
        }

        loop {
            let outputs = {
                let node = self.nodes.get_mut(node_id).ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        node_id
                    ))
                })?;

                match node.on_update()? {
                    Some(outputs) => {
                        node.validate_outputs(&outputs)?;
                        outputs
                    }
                    None => break,
                }
            };

            let mut event_pool = base_data_pool.clone();
            for (key, value) in outputs {
                event_pool.insert(key, value);
            }

            let mut skipped: HashSet<String> = HashSet::new();
            for ordered_id in ordered {
                if ordered_id == node_id {
                    continue;
                }
                if skipped.contains(ordered_id) {
                    continue;
                }
                if !reachable.contains(ordered_id) {
                    continue;
                }

                if event_producer_set.contains(ordered_id) {
                    self.run_event_producer(
                        ordered_id,
                        &event_pool,
                        reachable_map,
                        event_producer_set,
                        ordered,
                    )?;
                    if let Some(skip_set) = reachable_map.get(ordered_id) {
                        skipped.extend(skip_set.iter().cloned());
                    }
                    continue;
                }

                let node = self.nodes.get_mut(ordered_id).ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        ordered_id
                    ))
                })?;

                let inputs = Self::collect_inputs(node.as_ref(), &event_pool, ordered_id)?;
                let outputs = node.execute(inputs)?;
                for (key, value) in outputs {
                    if event_pool.contains_key(&key) {
                        return Err(crate::error::Error::ValidationError(format!(
                            "Output key '{}' from node '{}' conflicts with existing data",
                            key, ordered_id
                        )));
                    }
                    event_pool.insert(key, value);
                }
            }
        }

        let node = self.nodes.get_mut(node_id).ok_or_else(|| {
            crate::error::Error::ValidationError(format!(
                "Node '{}' not found during cleanup",
                node_id
            ))
        })?;
        node.on_cleanup()?;

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

    pub fn to_definition(&self) -> NodeGraphDefinition {
        NodeGraphDefinition::from_node_graph(self)
    }
}

impl Default for NodeGraph {
    fn default() -> Self {
        Self::new()
    }
}