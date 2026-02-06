# Node Graph JSON Specification

> **Source of truth:** `src/node/graph_io.rs` — Rust structs are serialized/deserialized with [serde](https://serde.rs).  
> **Live example:** [`node_graph.json`](../node_graph.json) in the repository root.

This document describes the JSON format used to persist and exchange node graphs.  
The GUI loads/saves this format, and the runtime rebuilds an executable `NodeGraph` from it via `build_node_graph_from_definition()` in `src/node/registry.rs`.

---

## Overview

A node graph file is a single JSON object with two top-level arrays:

```jsonc
{
  "nodes": [ ... ],   // required — list of NodeDefinition objects
  "edges": [ ... ]    // required — list of EdgeDefinition objects (may be empty)
}
```

> **Runtime-only field:** `execution_results` exists in memory for UI display but is **never** serialized to disk.

---

## NodeDefinition

Each entry in `nodes` describes a single node:

```jsonc
{
  "id":           "node_1",                              // unique, e.g. node_1, node_2, ...
  "name":         "QQ Bot Adapter",                      // display label
  "description":  "Receives messages from QQ server",    // optional
  "node_type":    "bot_adapter",                         // registry type_id (see § Registered Node Types)
  "input_ports":  [ /* Port */ ],
  "output_ports": [ /* Port */ ],
  "position":     { "x": 40.0, "y": 40.0 },             // optional — top-left corner in canvas coords
  "size":         { "width": 200.0, "height": 120.0 },   // optional — custom size; null = auto
  "inline_values": { "port_name": <json_value> },        // optional — default values for input ports
  "has_error":    false                                   // optional — runtime error flag, safe to omit
}
```

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `id` | `string` | true | Must be unique within the graph. Convention: `node_<n>`. |
| `name` | `string` | true | Human-readable display name shown in the GUI. |
| `description` | `string` | false | Tooltip / documentation. |
| `node_type` | `string` | true | Must match a registered `type_id` in `NODE_REGISTRY`. |
| `input_ports` | `Port[]` | true | Ordered list of input ports. |
| `output_ports` | `Port[]` | true | Ordered list of output ports. |
| `position` | `{ x, y }` | false | If omitted, the GUI auto-layouts on load. |
| `size` | `{ width, height }` | false | `null` or omitted -> auto-calculated from port count. |
| `inline_values` | `object` | false | Keys are port names; values are JSON primitives (`string`, `number`, `bool`). The UI supports inline editing for `String`, `Integer`, `Float`, and `Boolean` ports. |
| `has_error` | `bool` | false | Set by the runtime when execution fails at this node. Ignored on load. |

---

## Port

Describes one input or output port on a node:

```jsonc
{
  "name":        "prompt",         // binding key — edges reference this name
  "data_type":   "String",         // see § Data Types
  "description": "Input prompt",   // optional
  "required":    true              // input ports only — true means execution fails if unconnected and no inline value
}
```

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `name` | `string` | true | Unique within the node's input or output port list. |
| `data_type` | `DataType` | true | See [Data Types](#data-types) section. |
| `description` | `string` | false | Shown as tooltip in the GUI. |
| `required` | `bool` | true | Only meaningful for input ports. If `true`, the port must receive data from an edge or an `inline_values` entry. |

---

## EdgeDefinition

Each entry in `edges` represents a directed connection from an output port to an input port:

```jsonc
{
  "from_node_id": "node_1",       // source node
  "from_port":    "message_event", // source output port name
  "to_node_id":   "node_2",       // target node
  "to_port":      "message_event"  // target input port name
}
```

**Validation rules** (enforced at runtime):
- Both referenced nodes must exist.
- `from_port` must be an output port on the source node.
- `to_port` must be an input port on the target node.
- The `data_type` of both ports must match.
- Each input port can have **at most one** incoming edge.
- The graph must be a **DAG** (no cycles).

> **Legacy mode:** When `edges` is empty, the engine falls back to implicit auto-binding — an output port named `"foo"` automatically feeds any input port also named `"foo"` on a different node.

---

## Data Types

The `data_type` field maps to the `DataType` Rust enum (defined in `src/node/data_value.rs`).

### Primitive types

| Value | Rust variant | JSON inline value |
|-------|-------------|-------------------|
| `"String"` | `DataType::String` | `"hello"` |
| `"Integer"` | `DataType::Integer` | `42` |
| `"Float"` | `DataType::Float` | `3.14` |
| `"Boolean"` | `DataType::Boolean` | `true` / `false` |
| `"Json"` | `DataType::Json` | any JSON value |
| `"Binary"` | `DataType::Binary` | *(not inlineable)* |

### Composite / domain types

| Value | Description |
|-------|-------------|
| `"MessageList"` | `Vec<Message>` — LLM chat message history |
| `"MessageEvent"` | Bot message event struct |
| `"FunctionTools"` | LLM function-calling tool definitions |
| `"BotAdapterRef"` | Shared reference to the bot adapter |
| `"RedisRef"` | Redis connection configuration |
| `"MySqlRef"` | MySQL connection configuration |
| `{ "Custom": "<name>" }` | User-defined type |

### List type

`List` wraps an inner type and serializes as a JSON object:

```json
{ "List": "String" }
```

---

---

## Complete Example

A minimal 3-node pipeline: **Bot Adapter -> Message-to-String -> Preview**

```json
{
  "nodes": [
    {
      "id": "node_1",
      "name": "QQ Bot Adapter",
      "description": "Receives messages from QQ server",
      "node_type": "bot_adapter",
      "input_ports": [
        { "name": "qq_id",            "data_type": "String", "description": "QQ ID",                    "required": true  },
        { "name": "bot_server_url",   "data_type": "String", "description": "WebSocket server address", "required": true  },
        { "name": "bot_server_token", "data_type": "String", "description": "Auth token",               "required": false }
      ],
      "output_ports": [
        { "name": "message_event", "data_type": "MessageEvent", "description": "Raw message event", "required": true }
      ],
      "position": { "x": 40.0, "y": 40.0 },
      "inline_values": {
        "qq_id": "2721394556",
        "bot_server_url": "ws://localhost:3001",
        "bot_server_token": "my_token"
      }
    },
    {
      "id": "node_2",
      "name": "Message to String",
      "node_type": "message_event_to_string",
      "input_ports": [
        { "name": "message_event", "data_type": "MessageEvent", "required": true }
      ],
      "output_ports": [
        { "name": "prompt", "data_type": "String", "required": true }
      ],
      "position": { "x": 400.0, "y": 40.0 }
    },
    {
      "id": "node_3",
      "name": "Preview",
      "node_type": "preview_string",
      "input_ports": [
        { "name": "text", "data_type": "String", "required": false }
      ],
      "output_ports": [
        { "name": "text", "data_type": "String", "required": true }
      ],
      "position": { "x": 760.0, "y": 40.0 }
    }
  ],
  "edges": [
    { "from_node_id": "node_1", "from_port": "message_event", "to_node_id": "node_2", "to_port": "message_event" },
    { "from_node_id": "node_2", "from_port": "prompt",        "to_node_id": "node_3", "to_port": "text" }
  ]
}
```

The dataflow is:

```
[Bot Adapter] --message_event--> [Message->String] --prompt/text--> [Preview]
```

---

## See Also

- **[Node Development Guide](./node-development.md)** — How to create custom nodes, register them, and extend the node type system.