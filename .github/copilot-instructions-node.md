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

## Built-in Node Components

### Utility Nodes (`src/node/util_nodes.rs`)

#### ConditionalNode
Conditional branching based on boolean input:

```rust
pub struct ConditionalNode;

// Input ports:
// - "condition" (Boolean, required): Condition to evaluate
// - "true_value" (Json, optional): Value to output if condition is true
// - "false_value" (Json, optional): Value to output if condition is false

// Output ports:
// - "result" (Json): Selected value based on condition
// - "branch_taken" (String): "true" or "false"
```

#### JsonParserNode
Parse JSON string to structured data:

```rust
pub struct JsonParserNode;

// Input ports:
// - "json_string" (String, required): JSON string to parse

// Output ports:
// - "parsed" (Json): Parsed JSON object
// - "success" (Boolean): Whether parsing was successful
```

### LLM Integration Nodes (`src/llm/node_impl.rs`)

#### LLMNode
Wraps LLM API for text generation:

```rust
pub struct LLMNode;

// Input ports:
// - "prompt" (String, required): User prompt to send to LLM
// - "messages" (Json, optional): Full message history
// - "max_tokens" (Integer, optional): Maximum tokens in response

// Output ports:
// - "response" (String): LLM response text
// - "full_message" (Json): Complete message object from LLM
// - "token_usage" (Json): Token usage information

// Builder methods:
// - with_llm_api(LLMAPI): Configure LLM API instance
// - with_system_prompt(String): Set system prompt
```

#### AgentNode
AI Agent with tool-calling capabilities:

```rust
pub struct AgentNode;

// Input ports:
// - "task" (String, required): Task description for the agent
// - "context" (Json, optional): Additional context information
// - "tools" (Json, optional): Available tools for the agent

// Output ports:
// - "result" (String): Agent execution result
// - "tool_calls" (Json): Tools called during execution
// - "execution_log" (Json): Detailed execution log

// Constructor:
// - new(id, name, agent_type): agent_type specifies agent behavior
```

#### TextProcessorNode
Text processing operations (uppercase, lowercase, trim, reverse):

```rust
pub struct TextProcessorNode;

// Input ports:
// - "text" (String, required): Input text to process
// - "params" (Json, optional): Processing parameters

// Output ports:
// - "processed_text" (String): Processed text output
// - "metadata" (Json): Processing metadata (operation, input_length)

// Constructor:
// - new(id, name, operation): operation = "uppercase"|"lowercase"|"trim"|"reverse"
```

### Bot Adapter Nodes (`src/bot_adapter/`)

#### BotAdapterNode (`node_impl.rs`)
EventProducer node that receives messages from QQ bot server:

```rust
pub struct BotAdapterNode;  // NodeType::EventProducer

// Input ports:
// - "trigger" (Boolean, required): Trigger to start receiving messages
// - "qq_id" (String, optional): QQ ID to login

// Output ports:
// - "message" (MessageEvent): Raw message event from QQ server
// - "message_event" (MessageEvent): Same as message (alias)
// - "bot_adapter" (BotAdapterRef): Shared bot adapter handle
// - "message_type" (String): Type of the message
// - "user_id" (String): User ID who sent the message
// - "content" (String): Message content

// Lifecycle: Spawns async WebSocket client in on_start(), yields events via on_update()
```

#### MessageSenderNode (`node_impl.rs`)
Sends messages back to QQ server:

```rust
pub struct MessageSenderNode;

// Input ports:
// - "bot_adapter" (BotAdapterRef, required): Bot adapter instance
// - "user_id" (String, required): Target user ID
// - "group_id" (String, optional): Target group ID
// - "message" (String, required): Message text to send
// - "reply_to" (String, optional): Message ID to reply to

// Output ports:
// - "success" (Boolean): Whether message was sent successfully
// - "message_id" (String): ID of sent message (if successful)
```

#### MessageEventToStringNode (`message_event_to_string.rs`)
Converts MessageEvent to LLM prompt string:

```rust
pub struct MessageEventToStringNode;

// Input ports:
// - "message_event" (MessageEvent, required): MessageEvent containing message data

// Output ports:
// - "prompt" (String): Formatted LLM prompt string (includes content + quoted text)

// Note: Uses MessageProp::from_messages() to extract content and ref_content
```

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

## Node Registry (`src/node/registry.rs`)

Global registry for node type discovery and instantiation:

```rust
pub static NODE_REGISTRY: Lazy<NodeRegistry>;

// Register a node type:
NODE_REGISTRY.register(
    "type_id",           // Unique type identifier
    "Display Name",      // Human-readable name (supports Chinese)
    "Category",          // Group nodes by category ("工具", "AI", "适配器", etc.)
    "Description",       // Brief description
    factory_fn,          // Arc<dyn Fn(String, String) -> Box<dyn Node>>
)?;

// Create node instance:
let node = NODE_REGISTRY.create_node("conditional", "node_id", "My Node")?;

// Query registry:
let all_types = NODE_REGISTRY.get_all_types();  // Vec<NodeTypeMetadata>
let categories = NODE_REGISTRY.get_categories();
let ai_nodes = NODE_REGISTRY.get_types_by_category("AI");
```

**Currently Registered Node Types** (initialized in `init_node_registry()`):

| Type ID | Display Name | Category | Node Struct |
|---------|-------------|----------|-------------|
| `conditional` | 条件分支 | 工具 | ConditionalNode |
| `json_parser` | JSON解析器 | 工具 | JsonParserNode |
| `llm` | 大语言模型 | AI | LLMNode |
| `agent` | AI Agent | AI | AgentNode |
| `text_processor` | 文本处理器 | 工具 | TextProcessorNode |
| `bot_adapter` | QQ机器人适配器 | 适配器 | BotAdapterNode |
| `message_sender` | 消息发送器 | Bot适配器 | MessageSenderNode |
| `message_event_to_string` | 消息转字符串 | Bot适配器 | MessageEventToStringNode |

**Helper Macro for Registration**:

```rust
register_node!(
    "type_id",
    "Display Name",
    "Category",
    "Description",
    NodeStruct
);
```

## Integration Patterns

### Bot Message Processing Pipeline

Typical flow for QQ bot with LLM:

```rust
let mut graph = NodeGraph::new();

// 1. Receive messages from QQ (EventProducer)
graph.add_node(Box::new(BotAdapterNode::new("bot", "QQ Bot")));

// 2. Convert message to LLM prompt
graph.add_node(Box::new(MessageEventToStringNode::new("msg_to_str", "ToPrompt")));

// 3. Process with LLM
graph.add_node(Box::new(LLMNode::new("llm", "LLM").with_llm_api(api)));

// 4. Send response back
graph.add_node(Box::new(MessageSenderNode::new("sender", "Send Reply")));

// Port bindings (automatic by name matching):
// bot.message_event → msg_to_str.message_event
// msg_to_str.prompt → llm.prompt
// bot.bot_adapter → sender.bot_adapter
// bot.user_id → sender.user_id
// llm.response → sender.message

graph.execute()?;
```

### Multi-Step Agent Workflow

Agent with conditional branching:

```rust
// 1. Agent analyzes task
let agent = AgentNode::new("agent", "Task Agent", "analyzer");

// 2. Conditional routing based on tool calls
let conditional = ConditionalNode::new("branch", "Route");

// 3. Different text processors for different paths
let uppercase = TextProcessorNode::new("upper", "Uppercase", "uppercase");
let lowercase = TextProcessorNode::new("lower", "Lowercase", "lowercase");

// Port connections:
// agent.tool_calls → (parse to boolean) → branch.condition
// branch.result → upper.text OR lower.text
```

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

### Adding a New Node Type

1. **Create node struct** implementing `Node` trait in appropriate module:
   - Utility nodes → `src/node/util_nodes.rs`
   - LLM nodes → `src/llm/node_impl.rs`
   - Bot adapter nodes → `src/bot_adapter/node_impl.rs`
   - New category → create new file in `src/node/`

2. **Implement all required trait methods**:
   ```rust
   pub struct MyCustomNode {
       id: String,
       name: String,
       // ... custom fields
   }
   
   impl Node for MyCustomNode {
       fn id(&self) -> &str { &self.id }
       fn name(&self) -> &str { &self.name }
       fn description(&self) -> Option<&str> { Some("...") }
       fn input_ports(&self) -> Vec<Port> { vec![...] }
       fn output_ports(&self) -> Vec<Port> { vec![...] }
       fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
           // Implementation
       }
   }
   ```

3. **Register in `src/node/registry.rs`**:
   ```rust
   pub fn init_node_registry() -> Result<()> {
       // ... existing registrations
       
       register_node!(
           "my_custom",
           "My Custom Node",
           "Custom Category",
           "Node description",
           MyCustomNode
       );
       
       Ok(())
   }
   ```

4. **For EventProducer nodes**, override lifecycle methods:
   ```rust
   fn node_type(&self) -> NodeType { NodeType::EventProducer }
   fn on_start(&mut self, inputs: HashMap<String, DataValue>) -> Result<()> { /* init */ }
   fn on_update(&mut self) -> Result<Option<HashMap<String, DataValue>>> { /* loop */ }
   fn on_cleanup(&mut self) -> Result<()> { /* cleanup */ }
   ```

5. **Add to UI** (if needed): Node will automatically appear in GUI after registration.

### Modifying Core Execution

To enhance the execution model (parallel execution, streaming, etc.):

1. **Preserve `Node` trait interface** for backward compatibility
2. **Update `NodeGraph::execute()`** in `src/node/mod.rs`
3. **Modify lifecycle hooks** if needed (`on_start`/`on_update`/`on_cleanup`)
4. **Update validation logic** in `build_output_producer_map()` and topological sort
5. **Test with existing nodes** to ensure no regressions

### Custom Data Types

To add new port data types beyond String/Integer/Float/Boolean/Json/Binary:

1. **Extend `DataType` enum** in `src/node/data_value.rs`:
   ```rust
   pub enum DataType {
       // ... existing types
       MessageEvent,      // Custom type
       BotAdapterRef,     // Custom type
   }
   ```

2. **Extend `DataValue` enum**:
   ```rust
   pub enum DataValue {
       // ... existing variants
       MessageEvent(MessageEvent),
       BotAdapterRef(SharedBotAdapter),
   }
   ```

3. **Update validation logic** in `Port::validate_value()` and type conversion helpers.
