use serde_json::{json, Value};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use log::info;

/// NodeType enum for distinguishing node categories
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum NodeType {
    Simple,
    EventProducer,
}


#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub node_results: HashMap<String, HashMap<String, DataValue>>,
    pub error_node_id: Option<String>,
    pub error_message: Option<String>,
}

impl ExecutionResult {
    pub fn success(node_results: HashMap<String, HashMap<String, DataValue>>) -> Self {
        Self {
            node_results,
            error_node_id: None,
            error_message: None,
        }
    }

    pub fn with_error(
        node_results: HashMap<String, HashMap<String, DataValue>>,
        error_node_id: String,
        error_message: String,
    ) -> Self {
        Self {
            node_results,
            error_node_id: Some(error_node_id),
            error_message: Some(error_message),
        }
    }
}

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use crate::error::Result;

type OutputPool = HashMap<String, HashMap<String, DataValue>>;
type InputSourceMap = HashMap<String, HashMap<String, (String, String)>>;

pub mod data_value;
pub mod util_nodes;
pub mod graph_io;
pub mod registry;
pub mod database_nodes;
pub mod message_nodes;

#[allow(unused_imports)]
pub use data_value::{DataType, DataValue};
#[allow(unused_imports)]
pub use node_macros::{node_input, node_output};
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
    pub inline_values: HashMap<String, HashMap<String, DataValue>>,
    stop_flag: Arc<AtomicBool>,
    execution_callback: Option<Box<dyn Fn(&str, &HashMap<String, DataValue>, &HashMap<String, DataValue>) + Send + Sync>>,
    edges: Vec<EdgeDefinition>,
}

impl NodeGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            inline_values: HashMap::new(),
            stop_flag: Arc::new(AtomicBool::new(false)),
            execution_callback: None,
            edges: Vec::new(),
        }
    }

    pub fn set_execution_callback<F>(&mut self, callback: F)
    where
        F: Fn(&str, &HashMap<String, DataValue>, &HashMap<String, DataValue>) + Send + Sync + 'static,
    {
        self.execution_callback = Some(Box::new(callback));
    }

    pub fn set_edges(&mut self, edges: Vec<EdgeDefinition>) {
        self.edges = edges;
    }

    pub fn get_stop_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.stop_flag)
    }

    pub fn request_stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }

    pub fn reset_stop_flag(&mut self) {
        self.stop_flag.store(false, Ordering::Relaxed);
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
        if !self.edges.is_empty() {
            return self.execute_with_edges();
        }

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
                    // Check if the port has an inline value
                    let has_inline = self.inline_values
                        .get(node_id)
                        .map(|values| values.contains_key(&port.name))
                        .unwrap_or(false);
                    
                    if !has_inline {
                        return Err(crate::error::Error::ValidationError(format!(
                            "Required input port '{}' for node '{}' is not bound",
                            port.name, node_id
                        )));
                    }
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

                let inputs = Self::collect_inputs(node.as_ref(), &data_pool, &node_id, self.inline_values.get(&node_id))?;
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

            let inputs = Self::collect_inputs(node.as_ref(), &base_data_pool, node_id, self.inline_values.get(node_id))?;
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
    pub fn execute_and_capture_results(&mut self) -> ExecutionResult {
        let mut node_results: HashMap<String, HashMap<String, DataValue>> = HashMap::new();
        
        // Try to execute, if error occurs, return early with error info
        match self.execute_and_capture_results_internal(&mut node_results) {
            Ok(()) => ExecutionResult::success(node_results),
            Err(e) => {
                // Extract node ID from error if possible
                let error_msg = e.to_string();
                let error_node_id = self.extract_error_node_id(&error_msg);
                ExecutionResult::with_error(
                    node_results,
                    error_node_id.unwrap_or_else(|| "unknown".to_string()),
                    error_msg,
                )
            }
        }
    }

    fn extract_error_node_id(&self, error_msg: &str) -> Option<String> {
        // Try to find node ID in error message like "[NODE_ERROR:xxx]"
        if let Some(start) = error_msg.find("[NODE_ERROR:") {
            if let Some(end) = error_msg[start + 12..].find(']') {
                return Some(error_msg[start + 12..start + 12 + end].to_string());
            }
        }

        // Try to find node ID in error message like "Node 'xxx' ..."
        if let Some(start) = error_msg.find("Node '") {
            if let Some(end) = error_msg[start + 6..].find('\'') {
                return Some(error_msg[start + 6..start + 6 + end].to_string());
            }
        }
        None
    }

    fn execute_and_capture_results_internal(
        &mut self,
        node_results: &mut HashMap<String, HashMap<String, DataValue>>,
    ) -> Result<()> {
        if !self.edges.is_empty() {
            return self.execute_and_capture_results_with_edges(node_results);
        }
        
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
                    // Check if the port has an inline value
                    let has_inline = self.inline_values
                        .get(node_id)
                        .map(|values| values.contains_key(&port.name))
                        .unwrap_or(false);
                    
                    if !has_inline {
                        return Err(crate::error::Error::ValidationError(format!(
                            "Required input port '{}' for node '{}' is not bound",
                            port.name, node_id
                        )));
                    }
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

                let inputs = Self::collect_inputs(node.as_ref(), &data_pool, &node_id, self.inline_values.get(&node_id))?;
                
                let inputs_clone = if self.execution_callback.is_some() { Some(inputs.clone()) } else { None };

                let outputs = node.execute(inputs.clone())?;
                
                if let Some(cb) = &self.execution_callback {
                    if let Some(inp) = inputs_clone {
                        cb(&node_id, &inp, &outputs);
                    }
                }
                
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

            return Ok(());
        }

        // For event producers, we still need to execute but won't capture all results
        self.execute()?;
        
        Ok(())
    }

    fn execute_with_edges(&mut self) -> Result<()> {
        let (connected_nodes, dependents, dependencies, input_sources) = self.build_edge_maps()?;

        if connected_nodes.is_empty() {
            return Ok(());
        }

        let mut in_degree: HashMap<String, usize> = HashMap::new();
        for node_id in self.nodes.keys() {
            in_degree.insert(node_id.clone(), 0);
        }

        for (node_id, deps) in &dependencies {
            if let Some(count) = in_degree.get_mut(node_id) {
                *count += deps.len();
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

        for node_id in &connected_nodes {
            let node = self.nodes.get(node_id).ok_or_else(|| {
                crate::error::Error::ValidationError(format!(
                    "Node '{}' not found during execution",
                    node_id
                ))
            })?;

            let has_inline = self.inline_values.get(node_id);
            let input_map = input_sources.get(node_id);

            for port in node.input_ports() {
                if !port.required {
                    continue;
                }
                let has_edge = input_map
                    .and_then(|m| m.get(&port.name))
                    .is_some();
                let has_inline_value = has_inline
                    .map(|m| m.contains_key(&port.name))
                    .unwrap_or(false);
                if !has_edge && !has_inline_value {
                    return Err(crate::error::Error::ValidationError(format!(
                        "Required input port '{}' for node '{}' is not bound",
                        port.name, node_id
                    )));
                }
            }
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
            let mut data_pool: OutputPool = HashMap::new();
            for node_id in ordered {
                if !connected_nodes.contains(&node_id) {
                    continue;
                }
                let inputs = {
                    let node = self.nodes.get(&node_id).ok_or_else(|| {
                        crate::error::Error::ValidationError(format!(
                            "Node '{}' not found during execution",
                            node_id
                        ))
                    })?;
                    self.collect_inputs_with_edges(
                        node.as_ref(),
                        &data_pool,
                        &input_sources,
                        &node_id,
                        self.inline_values.get(&node_id),
                    )?
                };

                let inputs_clone = if self.execution_callback.is_some() { Some(inputs.clone()) } else { None };
                let outputs = {
                    let node = self.nodes.get_mut(&node_id).ok_or_else(|| {
                        crate::error::Error::ValidationError(format!(
                            "Node '{}' not found during execution",
                            node_id
                        ))
                    })?;
                    node.execute(inputs)?
                };

                if let Some(cb) = &self.execution_callback {
                    if let Some(inp) = inputs_clone {
                        cb(&node_id, &inp, &outputs);
                    }
                }

                self.insert_outputs(&mut data_pool, &node_id, outputs);
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

        let mut base_data_pool: OutputPool = HashMap::new();
        for node_id in &ordered {
            if !connected_nodes.contains(node_id) {
                continue;
            }
            if reachable_from_event.contains(node_id) {
                continue;
            }

            let inputs = {
                let node = self.nodes.get(node_id).ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        node_id
                    ))
                })?;
                self.collect_inputs_with_edges(
                    node.as_ref(),
                    &base_data_pool,
                    &input_sources,
                    node_id,
                    self.inline_values.get(node_id),
                )?
            };

            let outputs = {
                let node = self.nodes.get_mut(node_id).ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        node_id
                    ))
                })?;
                node.execute(inputs)?
            };
            self.insert_outputs(&mut base_data_pool, node_id, outputs);
        }

        let mut event_producer_roots: Vec<String> = event_producer_set
            .iter()
            .filter(|event_id| {
                connected_nodes.contains(*event_id)
                    && !dependencies
                        .get(*event_id)
                        .map(|deps| deps.iter().any(|dep| event_producer_set.contains(dep)))
                        .unwrap_or(false)
            })
            .cloned()
            .collect();
        event_producer_roots.sort();

        for root_id in event_producer_roots {
            self.run_event_producer_with_edges(
                &root_id,
                &base_data_pool,
                &reachable_map,
                &event_producer_set,
                &ordered,
                &connected_nodes,
                &input_sources,
            )?;
        }

        Ok(())
    }

    fn execute_and_capture_results_with_edges(
        &mut self,
        node_results: &mut HashMap<String, HashMap<String, DataValue>>,
    ) -> Result<()> {
        let (connected_nodes, dependents, dependencies, input_sources) = self.build_edge_maps()?;

        if connected_nodes.is_empty() {
            return Ok(());
        }

        let mut in_degree: HashMap<String, usize> = HashMap::new();
        for node_id in self.nodes.keys() {
            in_degree.insert(node_id.clone(), 0);
        }

        for (node_id, deps) in &dependencies {
            if let Some(count) = in_degree.get_mut(node_id) {
                *count += deps.len();
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

        for node_id in &connected_nodes {
            let node = self.nodes.get(node_id).ok_or_else(|| {
                crate::error::Error::ValidationError(format!(
                    "Node '{}' not found during execution",
                    node_id
                ))
            })?;

            let has_inline = self.inline_values.get(node_id);
            let input_map = input_sources.get(node_id);

            for port in node.input_ports() {
                if !port.required {
                    continue;
                }
                let has_edge = input_map
                    .and_then(|m| m.get(&port.name))
                    .is_some();
                let has_inline_value = has_inline
                    .map(|m| m.contains_key(&port.name))
                    .unwrap_or(false);
                if !has_edge && !has_inline_value {
                    return Err(crate::error::Error::ValidationError(format!(
                        "Required input port '{}' for node '{}' is not bound",
                        port.name, node_id
                    )));
                }
            }
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
            let mut data_pool: OutputPool = HashMap::new();
            for node_id in ordered {
                if !connected_nodes.contains(&node_id) {
                    continue;
                }
                let inputs = {
                    let node = self.nodes.get(&node_id).ok_or_else(|| {
                        crate::error::Error::ValidationError(format!(
                            "Node '{}' not found during execution",
                            node_id
                        ))
                    })?;
                    self.collect_inputs_with_edges(
                        node.as_ref(),
                        &data_pool,
                        &input_sources,
                        &node_id,
                        self.inline_values.get(&node_id),
                    )?
                };

                let inputs_clone = if self.execution_callback.is_some() { Some(inputs.clone()) } else { None };
                let outputs = {
                    let node = self.nodes.get_mut(&node_id).ok_or_else(|| {
                        crate::error::Error::ValidationError(format!(
                            "Node '{}' not found during execution",
                            node_id
                        ))
                    })?;
                    node.execute(inputs.clone())?
                };

                if let Some(cb) = &self.execution_callback {
                    if let Some(inp) = inputs_clone {
                        cb(&node_id, &inp, &outputs);
                    }
                }

                let mut result = inputs;
                result.extend(outputs.iter().map(|(k, v)| (k.clone(), v.clone())));
                node_results.insert(node_id.clone(), result);

                self.insert_outputs(&mut data_pool, &node_id, outputs);
            }

            return Ok(());
        }

        self.execute_with_edges()?;
        Ok(())
    }

    fn build_edge_maps(
        &self,
    ) -> Result<(
        HashSet<String>,
        HashMap<String, Vec<String>>,
        HashMap<String, Vec<String>>,
        InputSourceMap,
    )> {
        let mut connected_nodes: HashSet<String> = HashSet::new();
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();
        let mut dependencies: HashMap<String, Vec<String>> = HashMap::new();
        let mut input_sources: InputSourceMap = HashMap::new();

        for edge in &self.edges {
            let from_node = self.nodes.get(&edge.from_node_id).ok_or_else(|| {
                crate::error::Error::ValidationError(format!(
                    "Node '{}' not found for edge",
                    edge.from_node_id
                ))
            })?;
            let to_node = self.nodes.get(&edge.to_node_id).ok_or_else(|| {
                crate::error::Error::ValidationError(format!(
                    "Node '{}' not found for edge",
                    edge.to_node_id
                ))
            })?;

            let from_port = from_node
                .output_ports()
                .into_iter()
                .find(|p| p.name == edge.from_port)
                .ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Output port '{}' not found on node '{}'",
                        edge.from_port, edge.from_node_id
                    ))
                })?;

            let to_port = to_node
                .input_ports()
                .into_iter()
                .find(|p| p.name == edge.to_port)
                .ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Input port '{}' not found on node '{}'",
                        edge.to_port, edge.to_node_id
                    ))
                })?;

            if from_port.data_type != to_port.data_type {
                return Err(crate::error::Error::ValidationError(format!(
                    "Port type mismatch for edge {}.{} -> {}.{}",
                    edge.from_node_id, edge.from_port, edge.to_node_id, edge.to_port
                )));
            }

            connected_nodes.insert(edge.from_node_id.clone());
            connected_nodes.insert(edge.to_node_id.clone());

            dependents
                .entry(edge.from_node_id.clone())
                .or_default()
                .push(edge.to_node_id.clone());
            dependencies
                .entry(edge.to_node_id.clone())
                .or_default()
                .push(edge.from_node_id.clone());

            let entry = input_sources.entry(edge.to_node_id.clone()).or_default();
            if entry.contains_key(&edge.to_port) {
                return Err(crate::error::Error::ValidationError(format!(
                    "Input port '{}' on node '{}' has multiple connections",
                    edge.to_port, edge.to_node_id
                )));
            }
            entry.insert(
                edge.to_port.clone(),
                (edge.from_node_id.clone(), edge.from_port.clone()),
            );
        }

        Ok((connected_nodes, dependents, dependencies, input_sources))
    }

    fn collect_inputs_with_edges(
        &self,
        node: &dyn Node,
        data_pool: &OutputPool,
        input_sources: &InputSourceMap,
        node_id: &str,
        inline_values: Option<&HashMap<String, DataValue>>,
    ) -> Result<HashMap<String, DataValue>> {
        let mut inputs: HashMap<String, DataValue> = HashMap::new();
        let sources = input_sources.get(node_id);

        for port in node.input_ports() {
            if let Some(source_map) = sources.and_then(|m| m.get(&port.name)) {
                let (from_node_id, from_port) = source_map;
                if let Some(from_outputs) = data_pool.get(from_node_id) {
                    if let Some(value) = from_outputs.get(from_port) {
                        inputs.insert(port.name.clone(), value.clone());
                        continue;
                    }
                }
            }

            if let Some(value) = inline_values.and_then(|m| m.get(&port.name)) {
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

    fn insert_outputs(&self, pool: &mut OutputPool, node_id: &str, outputs: HashMap<String, DataValue>) {
        let entry = pool.entry(node_id.to_string()).or_default();
        for (key, value) in outputs {
            entry.insert(key, value);
        }
    }

    fn collect_inputs(
        node: &dyn Node,
        data_pool: &HashMap<String, DataValue>,
        node_id: &str,
        inline_values: Option<&HashMap<String, DataValue>>,
    ) -> Result<HashMap<String, DataValue>> {
        let mut inputs: HashMap<String, DataValue> = HashMap::new();
        for port in node.input_ports() {
            if let Some(value) = data_pool.get(&port.name) {
                inputs.insert(port.name.clone(), value.clone());
            } else if let Some(value) = inline_values.and_then(|m| m.get(&port.name)) {
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

    fn run_event_producer_with_edges(
        &mut self,
        node_id: &str,
        base_data_pool: &OutputPool,
        reachable_map: &HashMap<String, HashSet<String>>,
        event_producer_set: &HashSet<String>,
        ordered: &[String],
        connected_nodes: &HashSet<String>,
        input_sources: &InputSourceMap,
    ) -> Result<()> {
        let reachable = reachable_map
            .get(node_id)
            .cloned()
            .unwrap_or_default();

        {
            let inputs = {
                let node = self.nodes.get(node_id).ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        node_id
                    ))
                })?;
                self.collect_inputs_with_edges(
                    node.as_ref(),
                    base_data_pool,
                    input_sources,
                    node_id,
                    self.inline_values.get(node_id),
                )?
            };

            let node = self.nodes.get_mut(node_id).ok_or_else(|| {
                crate::error::Error::ValidationError(format!(
                    "Node '{}' not found during execution",
                    node_id
                ))
            })?;

            node.on_start(inputs).map_err(|e| {
                crate::error::Error::ValidationError(format!("[NODE_ERROR:{}] {}", node_id, e))
            })?;
        }

        loop {
            if self.stop_flag.load(Ordering::Relaxed) {
                info!("Event producer '{}' stopped by user request", node_id);
                break;
            }

            let outputs = {
                let node = self.nodes.get_mut(node_id).ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        node_id
                    ))
                })?;

                match node.on_update().map_err(|e| {
                    crate::error::Error::ValidationError(format!("[NODE_ERROR:{}] {}", node_id, e))
                })? {
                    Some(outputs) => {
                        node.validate_outputs(&outputs)?;
                        outputs
                    }
                    None => break,
                }
            };

            if let Some(cb) = &self.execution_callback {
                cb(node_id, &HashMap::new(), &outputs);
            }

            let mut event_pool = base_data_pool.clone();
            self.insert_outputs(&mut event_pool, node_id, outputs);

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
                if !connected_nodes.contains(ordered_id) {
                    continue;
                }

                if event_producer_set.contains(ordered_id) {
                    self.run_event_producer_with_edges(
                        ordered_id,
                        &event_pool,
                        reachable_map,
                        event_producer_set,
                        ordered,
                        connected_nodes,
                        input_sources,
                    )?;
                    if let Some(skip_set) = reachable_map.get(ordered_id) {
                        skipped.extend(skip_set.iter().cloned());
                    }
                    continue;
                }

                let inputs = {
                    let node = self.nodes.get(ordered_id).ok_or_else(|| {
                        crate::error::Error::ValidationError(format!(
                            "Node '{}' not found during execution",
                            ordered_id
                        ))
                    })?;
                    self.collect_inputs_with_edges(
                        node.as_ref(),
                        &event_pool,
                        input_sources,
                        ordered_id,
                        self.inline_values.get(ordered_id),
                    )?
                };

                let inputs_clone = if self.execution_callback.is_some() { Some(inputs.clone()) } else { None };
                let outputs = {
                    let node = self.nodes.get_mut(ordered_id).ok_or_else(|| {
                        crate::error::Error::ValidationError(format!(
                            "Node '{}' not found during execution",
                            ordered_id
                        ))
                    })?;
                    node.execute(inputs).map_err(|e| {
                        crate::error::Error::ValidationError(format!("[NODE_ERROR:{}] {}", ordered_id, e))
                    })?
                };

                if let Some(cb) = &self.execution_callback {
                    if let Some(inp) = inputs_clone {
                        cb(ordered_id, &inp, &outputs);
                    }
                }

                self.insert_outputs(&mut event_pool, ordered_id, outputs);
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

            let inputs = Self::collect_inputs(node.as_ref(), base_data_pool, node_id, self.inline_values.get(node_id))?;
            node.on_start(inputs).map_err(|e| {
                crate::error::Error::ValidationError(format!("[NODE_ERROR:{}] {}", node_id, e))
            })?;
        }

        loop {
            if self.stop_flag.load(Ordering::Relaxed) {
                info!("Event producer '{}' stopped by user request", node_id);
                break;
            }

            let outputs = {
                let node = self.nodes.get_mut(node_id).ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        node_id
                    ))
                })?;

                match node.on_update().map_err(|e| {
                    crate::error::Error::ValidationError(format!("[NODE_ERROR:{}] {}", node_id, e))
                })? {
                    Some(outputs) => {
                        node.validate_outputs(&outputs)?;
                        outputs
                    }
                    None => break,
                }
            };

            if let Some(cb) = &self.execution_callback {
                cb(node_id, &HashMap::new(), &outputs);
            }

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

                let inputs = Self::collect_inputs(node.as_ref(), &event_pool, ordered_id, self.inline_values.get(ordered_id))?;
                
                let inputs_clone = if self.execution_callback.is_some() { Some(inputs.clone()) } else { None };

                let outputs = node.execute(inputs).map_err(|e| {
                    crate::error::Error::ValidationError(format!("[NODE_ERROR:{}] {}", ordered_id, e))
                })?;
                
                if let Some(cb) = &self.execution_callback {
                    if let Some(inp) = inputs_clone {
                        cb(ordered_id, &inp, &outputs);
                    }
                }

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