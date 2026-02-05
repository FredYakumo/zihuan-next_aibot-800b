# Copilot Instructions: src/node/

## Purpose
Dataflow engine providing a composable, graph-based node system for building event-driven pipelines. Nodes are typed, reusable units with typed input/output ports that can be connected into directed acyclic graphs (DAGs) and executed with automatic dependency resolution.

## Core Concepts

### Node Trait
The `Node` trait is the fundamental unit. All nodes implement:

```rust
pub trait Node: Send + Sync {
    /// Returns the type of the node (Simple or EventProducer)
    fn node_type(&self) -> NodeType { NodeType::Simple }
    
    /// Unique identifier for the node
    fn id(&self) -> &str;
    
    /// Human-readable name
    fn name(&self) -> &str;
    
    /// Optional description of what the node does
    fn description(&self) -> Option<&str> { None }
    
    /// Define input ports (name, data type, required flag)
    fn input_ports(&self) -> Vec<Port>;
    
    /// Define output ports
    fn output_ports(&self) -> Vec<Port>;
    
    /// Main execution logic: inputs → outputs
    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>>;
    
    /// [EventProducer only] Called once before update loop
    fn on_start(&mut self, _inputs: HashMap<String, DataValue>) -> Result<()> { Ok(()) }
    
    /// [EventProducer only] Called repeatedly; returns Some(outputs) until loop exits
    fn on_update(&mut self) -> Result<Option<HashMap<String, DataValue>>> { Ok(None) }
    
    /// [EventProducer only] Called after update loop
    fn on_cleanup(&mut self) -> Result<()> { Ok(()) }
    
    /// Input/output validation (auto-called during execution)
    fn validate_inputs(&self, inputs: &HashMap<String, DataValue>) -> Result<()>;
    fn validate_outputs(&self, outputs: &HashMap<String, DataValue>) -> Result<()>;
}
```

### Node Types
- **Simple**: Stateless node that executes once per input; typical for transforms, calculations, filters.
- **EventProducer**: Stateful node with lifecycle hooks (`on_start` → `on_update` loop → `on_cleanup`); used for event sources, timers, polling loops.

### Port System
Ports define the contract for input/output:

```rust
pub struct Port {
    pub name: String,              // Unique identifier in node scope
    pub data_type: DataType,       // Enforced at runtime
    pub description: Option<String>,
    pub required: bool,            // Input only; output ports always "produced"
}

// Port builder pattern:
Port::new("input_name", DataType::String)
    .with_description("Human-friendly description")
    .with_required(false)
    .optional()
```

### DataType & DataValue
Strongly typed port data:

```rust
pub enum DataType {
    String,
    Integer,
    Float,
    Boolean,
    Json,           // For complex/nested structures
    Binary,
}

// DataValue is the runtime container
pub enum DataValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Json(serde_json::Value),
    Binary(Vec<u8>),
}
```

Port names are the binding key—no explicit edges. If node A's output port `"result"` matches node B's input port `"result"`, they are connected.

### NodeGraph Execution
`NodeGraph` manages a collection of nodes and executes them as a DAG:

```rust
pub struct NodeGraph {
    pub nodes: HashMap<String, Box<dyn Node>>,
}

impl NodeGraph {
    pub fn new() -> Self;
    pub fn add_node(&mut self, node: Box<dyn Node>) -> Result<()>;
    pub fn execute(&mut self) -> Result<()>;  // Validates, topologically sorts, executes
}
```

#### Execution Flow
1. **Validation**: Build output producer map; verify no port conflicts, all required inputs bound.
2. **Topological Sort**: Compute in-degrees, use Kahn's algorithm to detect cycles and order nodes.
3. **Execution Strategy**:
   - **No EventProducers**: Simple pipeline—initialize `data_pool`, execute nodes in order, accumulate outputs.
   - **With EventProducers**: Multi-phase:
     - Execute non-reachable nodes (static setup).
     - For each EventProducer root (no EventProducer dependencies), call `on_start()`, then loop `on_update()` → run reachable nodes downstream → `on_cleanup()`.
     - EventProducers can be nested (one EventProducer feeds another).

## Built-in Utility Nodes

### ConditionalNode
Branching node:

```rust
pub struct ConditionalNode;

// Input ports:
// - "condition" (Boolean, required)
// - "true_value" (Json, optional, defaults to null)
// - "false_value" (Json, optional, defaults to null)

// Output ports:
// - "result" (Json): Selected value
// - "branch_taken" (String): "true" or "false"
```

### JsonParserNode
Parse/validate JSON:

```rust
pub struct JsonParserNode;

// Input: "json_string" (String)
// Output: "parsed_json" (Json), "is_valid" (Boolean)
```

Found in `src/node/util_nodes.rs`.

## Node Implementation Example

Minimal working node:

```rust
use crate::node::{DataType, DataValue, Node, Port};
use crate::error::Result;
use std::collections::HashMap;

pub struct MyNode {
    id: String,
}

impl MyNode {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

impl Node for MyNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        "MyNode"
    }

    fn description(&self) -> Option<&str> {
        Some("Does something useful")
    }

    fn input_ports(&self) -> Vec<Port> {
        vec![
            Port::new("input_text", DataType::String)
                .with_description("Text to process"),
        ]
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![
            Port::new("output_text", DataType::String)
                .with_description("Processed text"),
        ]
    }

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let text = match inputs.get("input_text") {
            Some(DataValue::String(s)) => s.clone(),
            _ => return Err("Missing input_text".into()),
        };

        let mut outputs = HashMap::new();
        outputs.insert("output_text".to_string(), DataValue::String(text.to_uppercase()));

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
```

## Event Producer Example

Nodes that generate events over time:

```rust
pub struct PollingNode {
    id: String,
    counter: usize,
}

impl Node for PollingNode {
    fn node_type(&self) -> NodeType {
        NodeType::EventProducer
    }

    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { "Polling" }

    fn input_ports(&self) -> Vec<Port> {
        vec![Port::new("max_count", DataType::Integer)]
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![Port::new("event_id", DataType::Integer)]
    }

    fn on_start(&mut self, inputs: HashMap<String, DataValue>) -> Result<()> {
        self.validate_inputs(&inputs)?;
        self.counter = 0;
        Ok(())
    }

    fn on_update(&mut self) -> Result<Option<HashMap<String, DataValue>>> {
        let max = 5;  // Example limit
        if self.counter >= max {
            return Ok(None);  // Signal end of loop
        }

        let mut outputs = HashMap::new();
        outputs.insert("event_id".to_string(), DataValue::Integer(self.counter as i64));
        self.counter += 1;

        Ok(Some(outputs))
    }

    fn on_cleanup(&mut self) -> Result<()> {
        println!("Polling completed");
        Ok(())
    }

    // ... (standard trait methods omitted)
}
```

## Port Binding Rules

Port names are the connection mechanism. A port is "bound" if:
- Output port name matches an input port name in another node.
- Order of `add_node()` calls doesn't matter—binding is resolved at `execute()` time.

Example binding:

```rust
let mut graph = NodeGraph::new();
let node_a = MyProducerNode::new("a");    // output_ports: ["result"]
let node_b = MyConsumerNode::new("b");    // input_ports: ["result"]

graph.add_node(Box::new(node_a))?;
graph.add_node(Box::new(node_b))?;

// At execute time: "result" from node_a → node_b's "result" input
graph.execute()?;
```

## Error Handling

Nodes validate:
- **Input validation**: Type mismatches, missing required ports.
- **Output validation**: Ensure outputs match declared types.
- **Graph validation**: Duplicate port producers, unbound required inputs, cycles.

Return `Result<HashMap<String, DataValue>>` or `Result<()>` for lifecycle methods.

## Integration with LLM Agent

LLM-based nodes in `src/llm/node_impl.rs` (e.g., `LLMNode`, `AgentNode`) accept tool definitions and produce JSON outputs (chat responses, tool calls). They integrate into NodeGraphs to orchestrate multi-step reasoning pipelines.

## Testing

Simple unit test pattern:

```rust
#[test]
fn test_my_node() -> Result<()> {
    let mut node = MyNode::new("test_node");
    let mut inputs = HashMap::new();
    inputs.insert("input_text".to_string(), DataValue::String("hello".to_string()));
    
    let outputs = node.execute(inputs)?;
    assert_eq!(
        outputs.get("output_text"),
        Some(&DataValue::String("HELLO".to_string()))
    );
    Ok(())
}
```

Integration with `NodeGraph`:

```rust
#[test]
fn test_graph_execution() -> Result<()> {
    let mut graph = NodeGraph::new();
    graph.add_node(Box::new(MyNode::new("node1")))?;
    graph.add_node(Box::new(MyNode::new("node2")))?;
    graph.execute()?;
    Ok(())
}
```

## Key Files Reference

- **Core trait**: [src/node/mod.rs](src/node/mod.rs) — `Node` trait, `NodeGraph`, `Port`, `DataType`, `DataValue`
- **Data types**: [src/node/data_value.rs](src/node/data_value.rs) — `DataType` enum, `DataValue` implementation
- **Utility nodes**: [src/node/util_nodes.rs](src/node/util_nodes.rs) — `ConditionalNode`, `JsonParserNode`
- **LLM integration**: [src/llm/node_impl.rs](src/llm/node_impl.rs) — `LLMNode`, `AgentNode`, `TextProcessorNode`
- **Bot adapter node**: [src/bot_adapter/message_event_to_string.rs](src/bot_adapter/message_event_to_string.rs) — Example of converting bot events to nodes

## Extensions

To add a new node type:

1. Create a struct implementing `Node`.
2. Implement all required trait methods.
3. Register in any pipeline that needs it (e.g., add to `NodeGraph` in `src/main.rs` or in test harness).
4. For LLM-based nodes, extend `src/llm/node_impl.rs` and define tool outputs.

To modify the core execution model (e.g., parallel execution, streaming), update `NodeGraph::execute()` and lifecycle hooks while preserving the `Node` trait interface.
