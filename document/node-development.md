# Node Development Guide

> **Prerequisites:** Familiarity with Rust traits, async/sync patterns, and the [Node Graph JSON Specification](./node-graph-json.md).

This guide covers the full lifecycle of creating, registering, and distributing custom node types in the zihuan-next node graph system.

---

## Table of Contents

- [Node Development Guide](#node-development-guide)
  - [Table of Contents](#table-of-contents)
  - [Core Concepts](#core-concepts)
    - [Node Types](#node-types)
    - [Port-based Dataflow](#port-based-dataflow)
    - [Execution Flow](#execution-flow)
  - [Node Trait Reference](#node-trait-reference)
  - [Creating a Simple Node](#creating-a-simple-node)
    - [1. Define the struct](#1-define-the-struct)
    - [2. Implement the `Node` trait](#2-implement-the-node-trait)
  - [Creating an EventProducer Node](#creating-an-eventproducer-node)
  - [Registering Your Node](#registering-your-node)
    - [Using the `register_node!` macro](#using-the-register_node-macro)
    - [Manual registration (for nodes with complex constructors)](#manual-registration-for-nodes-with-complex-constructors)
  - [Data Types and Validation](#data-types-and-validation)
    - [Available `DataType` variants](#available-datatype-variants)
    - [Creating a Port](#creating-a-port)
    - [Type Validation](#type-validation)
  - [Built-in Node Types](#built-in-node-types)
  - [Testing Your Node](#testing-your-node)
    - [Unit test example](#unit-test-example)
    - [Integration test with NodeGraph](#integration-test-with-nodegraph)
  - [Source Code References](#source-code-references)
  - [Next Steps](#next-steps)

---

## Core Concepts

### Node Types

All nodes implement the `Node` trait (`src/node/mod.rs`). There are two execution models:

| Type | Trait method | Use case |
|------|-------------|----------|
| **Simple** | `execute()` | Stateless transform — runs once per input set |
| **EventProducer** | `on_start()`, `on_update()`, `on_cleanup()` | Stateful event source — runs a lifecycle loop |

### Port-based Dataflow

- **Ports** are typed input/output channels (e.g., `String`, `MessageEvent`).
- **Edges** connect an output port to an input port with matching types.
- The engine validates types, detects cycles, topologically sorts nodes, and executes them in dependency order.

### Execution Flow

```
Simple node:     NodeGraph::execute() → node.execute(inputs) → outputs
EventProducer:   on_start(inputs) → loop { on_update() } → on_cleanup()
```

EventProducers can feed simple nodes or other EventProducers downstream.

---

## Node Trait Reference

Full trait definition from `src/node/mod.rs`:

```rust
pub trait Node: Send + Sync {
    /// Node execution model: Simple or EventProducer
    fn node_type(&self) -> NodeType { NodeType::Simple }

    /// Unique node ID (set at creation time)
    fn id(&self) -> &str;

    /// Display name shown in GUI
    fn name(&self) -> &str;

    /// Optional tooltip/documentation
    fn description(&self) -> Option<&str> { None }

    /// Define input ports with types and requirements
    fn input_ports(&self) -> Vec<Port>;

    /// Define output ports with types
    fn output_ports(&self) -> Vec<Port>;

    /// [Simple nodes] Execute logic and return outputs
    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>>;

    /// [EventProducer] Initialize with inputs before loop
    fn on_start(&mut self, _inputs: HashMap<String, DataValue>) -> Result<()> { Ok(()) }

    /// [EventProducer] Called repeatedly; return Some(outputs) or None to exit
    fn on_update(&mut self) -> Result<Option<HashMap<String, DataValue>>> { Ok(None) }

    /// [EventProducer] Cleanup after loop exits
    fn on_cleanup(&mut self) -> Result<()> { Ok(()) }

    // Validation methods (auto-generated, override if needed)
    fn validate_inputs(&self, inputs: &HashMap<String, DataValue>) -> Result<()> { /* ... */ }
    fn validate_outputs(&self, outputs: &HashMap<String, DataValue>) -> Result<()> { /* ... */ }
}
```

---

## Creating a Simple Node

A minimal node that uppercases strings.

### 1. Define the struct

```rust
// src/node/util_nodes.rs (or your module)

use crate::node::{Node, NodeType, Port, DataType, DataValue};
use crate::error::Result;
use std::collections::HashMap;

pub struct UppercaseNode {
    id: String,
    name: String,
}

impl UppercaseNode {
    pub fn new(id: String, name: String) -> Self {
        Self { id, name }
    }
}
```

### 2. Implement the `Node` trait

```rust
impl Node for UppercaseNode {
    fn node_type(&self) -> NodeType {
        NodeType::Simple
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Converts input text to uppercase")
    }

    fn input_ports(&self) -> Vec<Port> {
        vec![
            Port::new("text", DataType::String)
                .with_description("Input text")
                .with_required(true),
        ]
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![
            Port::new("result", DataType::String)
                .with_description("Uppercased text"),
        ]
    }

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        let text = match inputs.get("text") {
            Some(DataValue::String(s)) => s.to_uppercase(),
            _ => return Err(crate::error::Error::ValidationError("Missing or invalid 'text' input".into())),
        };

        let mut outputs = HashMap::new();
        outputs.insert("result".to_string(), DataValue::String(text));
        Ok(outputs)
    }
}
```

---

## Creating an EventProducer Node

A node that emits periodic messages (e.g., timer or polling).

```rust
use std::time::{Duration, Instant};

pub struct TimerNode {
    id: String,
    name: String,
    interval_secs: u64,
    max_ticks: u32,
    tick_count: u32,
    last_tick: Option<Instant>,
}

impl TimerNode {
    pub fn new(id: String, name: String) -> Self {
        Self {
            id,
            name,
            interval_secs: 5,
            max_ticks: 10,
            tick_count: 0,
            last_tick: None,
        }
    }
}

impl Node for TimerNode {
    fn node_type(&self) -> NodeType {
        NodeType::EventProducer
    }

    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { &self.name }

    fn description(&self) -> Option<&str> {
        Some("Emits events at fixed intervals")
    }

    fn input_ports(&self) -> Vec<Port> {
        vec![
            Port::new("interval_secs", DataType::Integer)
                .with_description("Interval in seconds")
                .optional(),
            Port::new("max_ticks", DataType::Integer)
                .with_description("Max number of ticks")
                .optional(),
        ]
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![
            Port::new("tick", DataType::Integer)
                .with_description("Current tick count"),
        ]
    }

    fn execute(&mut self, _inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        // Not used for EventProducers
        Ok(HashMap::new())
    }

    fn on_start(&mut self, inputs: HashMap<String, DataValue>) -> Result<()> {
        if let Some(DataValue::Integer(i)) = inputs.get("interval_secs") {
            self.interval_secs = *i as u64;
        }
        if let Some(DataValue::Integer(i)) = inputs.get("max_ticks") {
            self.max_ticks = *i as u32;
        }
        self.last_tick = Some(Instant::now());
        self.tick_count = 0;
        Ok(())
    }

    fn on_update(&mut self) -> Result<Option<HashMap<String, DataValue>>> {
        if self.tick_count >= self.max_ticks {
            return Ok(None); // Signal completion
        }

        if let Some(last) = self.last_tick {
            if last.elapsed() < Duration::from_secs(self.interval_secs) {
                std::thread::sleep(Duration::from_millis(100));
                return self.on_update(); // Retry
            }
        }

        self.tick_count += 1;
        self.last_tick = Some(Instant::now());

        let mut outputs = HashMap::new();
        outputs.insert("tick".to_string(), DataValue::Integer(self.tick_count as i64));
        Ok(Some(outputs))
    }

    fn on_cleanup(&mut self) -> Result<()> {
        log::info!("Timer node {} completed {} ticks", self.id, self.tick_count);
        Ok(())
    }
}
```

---

## Registering Your Node

All nodes must be registered in `src/node/registry.rs` → `init_node_registry()`.

### Using the `register_node!` macro

```rust
// In init_node_registry() function:

register_node!(
    "uppercase",           // type_id (used in JSON node_type field)
    "Uppercase",           // Display name
    "Utility",             // Category
    "Converts text to uppercase",  // Description
    UppercaseNode          // Your struct type
);
```

### Manual registration (for nodes with complex constructors)

```rust
NODE_REGISTRY.register(
    "timer",
    "Timer",
    "Event Sources",
    "Emits periodic events",
    Arc::new(|id: String, name: String| {
        Box::new(TimerNode::new(id, name))
    }),
)?;
```

---

## Data Types and Validation

### Available `DataType` variants

See `src/node/data_value.rs`:

| Type | Rust variant | Use case |
|------|-------------|----------|
| `String` | `DataType::String` | Text data |
| `Integer` | `DataType::Integer` | `i64` numbers |
| `Float` | `DataType::Float` | `f64` numbers |
| `Boolean` | `DataType::Boolean` | `true`/`false` |
| `Json` | `DataType::Json` | Arbitrary JSON structures |
| `Binary` | `DataType::Binary` | `Vec<u8>` blobs |
| `List(inner)` | `DataType::List(Box<DataType>)` | Homogeneous arrays |
| `MessageList` | `DataType::MessageList` | LLM chat history |
| `MessageEvent` | `DataType::MessageEvent` | Bot message events |
| `FunctionTools` | `DataType::FunctionTools` | LLM function tools |
| `BotAdapterRef` | `DataType::BotAdapterRef` | Shared bot adapter |
| `RedisRef` | `DataType::RedisRef` | Redis connection config |
| `MySqlRef` | `DataType::MySqlRef` | MySQL connection config |

### Creating a Port

```rust
Port::new("input_name", DataType::String)
    .with_description("Help text")
    .with_required(true)  // or .optional() for false
```

### Type Validation

The `Node` trait provides default validation that checks:
- Required input ports have values
- All port types match their declared types

Override `validate_inputs()` or `validate_outputs()` for custom checks:

```rust
fn validate_inputs(&self, inputs: &HashMap<String, DataValue>) -> Result<()> {
    if let Some(DataValue::Integer(n)) = inputs.get("port_number") {
        if *n < 1024 || *n > 65535 {
            return Err(Error::ValidationError("Port must be 1024-65535".into()));
        }
    }
    // Call default validation
    self.validate_inputs_default(inputs)
}
```

---

## Built-in Node Types

Registered in `src/node/registry.rs` → `init_node_registry()`:

| `node_type` | Display Name | Category | Source |
|-------------|-------------|----------|--------|
| `conditional` | Conditional Branch | Utility | `src/node/util_nodes.rs` |
| `json_parser` | JSON Parser | Utility | `src/node/util_nodes.rs` |
| `preview_string` | Preview String | Utility | `src/node/util_nodes.rs` |
| `string_data` | String Data | Utility | `src/node/util_nodes.rs` |
| `llm` | LLM | AI | `src/llm/node_impl.rs` |
| `agent` | AI Agent | AI | `src/llm/node_impl.rs` |
| `text_processor` | Text Processor | Utility | `src/llm/node_impl.rs` |
| `bot_adapter` | QQ Bot Adapter | Bot Adapter | `src/bot_adapter/node_impl.rs` |
| `message_sender` | Message Sender | Bot Adapter | `src/bot_adapter/node_impl.rs` |
| `message_event_to_string` | Message → String | Bot Adapter | `src/bot_adapter/message_event_to_string.rs` |
| `redis` | Redis Connection | Database | `src/node/database_nodes.rs` |
| `mysql` | MySQL Connection | Database | `src/node/database_nodes.rs` |
| `message_mysql_persistence` | Message MySQL Persistence | Message Store | `src/node/message_nodes.rs` |
| `message_cache` | Message Cache | Message Store | `src/node/message_nodes.rs` |

Refer to these implementations for patterns and best practices.

---

## Testing Your Node

### Unit test example

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uppercase_node() {
        let mut node = UppercaseNode::new("test".into(), "Test".into());
        
        let mut inputs = HashMap::new();
        inputs.insert("text".to_string(), DataValue::String("hello".into()));

        let outputs = node.execute(inputs).unwrap();
        
        match outputs.get("result") {
            Some(DataValue::String(s)) => assert_eq!(s, "HELLO"),
            _ => panic!("Expected String output"),
        }
    }
}
```

### Integration test with NodeGraph

```rust
#[test]
fn test_node_in_graph() {
    use crate::node::{NodeGraph, graph_io::NodeGraphDefinition};
    use crate::node::registry::build_node_graph_from_definition;

    let json = r#"
    {
      "nodes": [
        {
          "id": "n1",
          "name": "Uppercase",
          "node_type": "uppercase",
          "input_ports": [{"name": "text", "data_type": "String", "required": true}],
          "output_ports": [{"name": "result", "data_type": "String", "required": true}],
          "inline_values": {"text": "test"}
        }
      ],
      "edges": []
    }
    "#;

    let def: NodeGraphDefinition = serde_json::from_str(json).unwrap();
    let mut graph = build_node_graph_from_definition(&def).unwrap();
    graph.execute().unwrap();
}
```

---

## Source Code References

| Concept | File |
|---------|------|
| `Node` trait definition | `src/node/mod.rs` |
| `Port`, `DataType`, `DataValue` | `src/node/data_value.rs` |
| Node registry & `register_node!` macro | `src/node/registry.rs` |
| Simple node examples | `src/node/util_nodes.rs` |
| EventProducer example | `src/bot_adapter/node_impl.rs` (`BotAdapterNode`) |
| LLM-based nodes | `src/llm/node_impl.rs` |
| JSON format | [node-graph-json.md](./node-graph-json.md) |

---

## Next Steps

1. Study existing nodes in `src/node/util_nodes.rs`
2. Create your node in a new module or existing category file
3. Register it in `init_node_registry()`
4. Test with a JSON graph file
5. Use the GUI to visually connect and execute your node
