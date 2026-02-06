# Copilot Instructions for zihuan-next_aibot-800b

> Keep this concise. Details live in code, tests, and the module notes under `.github/`.

## Architecture (big picture)
**Node-graph-based visual programming system** for building event-driven bot pipelines with composable dataflow components.

### Core: Node Graph System (`src/node/`)
- **Foundation**: Everything is a node. Nodes implement `Node` trait with typed input/output ports (String, Integer, Float, Boolean, Json, Binary).
- **Port-based binding**: No explicit edges—connections inferred from matching port names. Output port "result" auto-connects to any input port "result".
- **DAG execution**: `NodeGraph::execute()` validates port dependencies, detects cycles, topologically sorts, then executes nodes in order.
- **Node types**:
  - **Simple nodes**: Stateless transforms (execute once per input)
  - **EventProducer nodes**: Stateful with lifecycle hooks (`on_start` → `on_update` loop → `on_cleanup`) for event sources, timers, polling
- **Visual editing**: Slint-based GUI (`src/ui/*`) for drag-and-drop node graph creation, JSON import/export, persistent window state.

### Integration Components
Built as nodes or node-compatible wrappers:
- **Bot Adapter** (`src/bot_adapter/`): WebSocket inbound from QQ bot server → deserialize to typed messages → convert to node inputs via `message_event_to_string.rs`.
- **LLM Integration** (`src/llm/`): `LLMNode`, `AgentNode`, `TextProcessorNode` wrap HTTP chat API; `BrainAgent` orchestrates multi-tool reasoning; function tools (`MathTool`, `ChatHistoryTool`, `CodeWriterTool`) plugged as node dependencies.
- **Message Store** (`src/util/message_store.rs`): Three-tier storage (Redis cache, MySQL persistence, in-memory fallback) accessible to nodes for context retrieval.
- **Config** (`src/config.rs`): YAML-based settings (BOT_SERVER_URL, REDIS_*, MYSQL_*, LLM endpoints) loaded at startup; priority: file → env vars → defaults.

### Language Division
- **Rust**: Node graph engine, execution runtime, bot adapters, LLM/agent logic, UI, configuration, all business logic.
- **Python**: Database schema migrations (alembic) for MySQL persistence layer.

## Critical workflows
### Setup
```bash
cp config.yaml.example config.yaml  # Customize BOT_SERVER_URL, REDIS_*, MYSQL_*, LLM endpoints
docker compose -f docker/docker-compose.yaml up -d  # Start Redis (optional: MySQL for persistence)
alembic upgrade head  # Apply database migrations (if using MySQL)
```

### Node Graph Development
```bash
# Visual node editing (primary mode)
cargo run                                    # Launch GUI with empty graph
cargo run -- --graph-json example.json      # Load existing graph in GUI

# Headless/CLI mode
cargo run -- --graph-json input.json --save-graph-json output.json --no-gui  # Convert/validate graphs
cargo test                                   # Unit tests (add -- --ignored for LLM integration tests)
```

### Bot Runtime
```bash
cargo run  # In default mode (no args), starts bot adapter + WebSocket client
# Bot mode vs GUI mode determined by CLI args: no args = GUI; `--no-gui` or graph file = headless
```

### Creating Custom Nodes
1. Implement `Node` trait in new `.rs` file (define `id()`, `name()`, `input_ports()`, `output_ports()`, `execute()`)
2. For event sources: set `node_type() -> NodeType::EventProducer` and override `on_start()/on_update()/on_cleanup()`
3. Add to `NodeGraph` via `graph.add_node(Box::new(YourNode::new(...)))?`
4. Connect by matching port names—no explicit edge API
5. See `ConditionalNode` (util_nodes.rs), `LLMNode` (llm/node_impl.rs), `MessageEventToStringNode` (bot_adapter/) for examples

## Project-specific conventions
- **Node port binding**: Port names are the only connection mechanism. Output "result" → Input "result" = auto-connected. The graph is a DAG; `NodeGraph::execute()` validates, topo-sorts, executes. No edge objects, no manual wiring API.
- **Node execution model**: Simple nodes run once per input set. EventProducer nodes have lifecycle: `on_start()` → `on_update()` loop (returns `Some(outputs)` until done, then `None`) → `on_cleanup()`. Multiple EventProducers can chain (one feeds another).
- **Data types**: Strongly typed ports (String, Integer, Float, Boolean, Json, Binary). Runtime validation ensures type safety. `DataValue` enum wraps all types.
- **Configuration**: Single source of truth in `config.yaml`. Loaded by `config::load_config()` in `src/main.rs`. Priority: file → env vars → defaults. Bot server URL and token guaranteed non-empty after load.
- **Redis**: Special chars in passwords percent-encoded (RFC 3986) by `config::pct_encode()`. Redis URL built by `config::build_redis_url()`. Redis is flushed (`FLUSHDB`) on startup.
- **MySQL**: Database URL built by `config::build_mysql_url()`. Schema managed via Python/alembic migrations in `migrations/`. SQLx pool created on store init.
- **Message store**: Always init early in `BotAdapter::new()` with both Redis and MySQL URLs. Redis for cache, MySQL for persistence, in-memory fallback logs warning. Get context with `MessageStore::get_message()` (Redis cache) or `MessageStore::get_message_record()` (MySQL) before LLM responses. Historical queries use `MessageStore::query_messages()` (MySQL).
- **Deserialization**: Serde-based enums with lenient parsing—skips unsupported elements instead of failing entire event.
- **Logging**: Logs to `./logs` via `LogUtil` (`log_util` crate). Prefix message store logs with `[MessageStore]`.
- **UI state**: Window position/size auto-saves to platform-specific config dir (Linux/macOS: `~/.config/zihuan_next/`, Windows: `%APPDATA%/zihuan_next/window_config.json`).

## Extending the bot
- **New node type**: Implement `Node` trait (define `id()`, `name()`, `input_ports()`, `output_ports()`, `execute()`). For event sources, override `node_type() -> NodeType::EventProducer` and implement `on_start()/on_update()/on_cleanup()`. See `ConditionalNode` (util_nodes.rs) for minimal example, `MessageEventToStringNode` (bot_adapter/) for event handling.
- **New platform**: Add handler in `src/bot_adapter/event.rs`, register in `BotAdapter::new()`, extend `MessageType` enum in `src/bot_adapter/models/event_model.rs`.
- **Node registry**: Add new node types to `src/node/registry.rs` for discoverability in GUI. Register with `NODE_REGISTRY.lock().unwrap().register(...)` during initialization.
- **Configuration changes**: Modify `Config` struct fields in `src/config.rs`, update `load_config()` priority logic, document in `config.yaml.example`.
- **Database schema changes**: Edit Python models in `database/models/`, generate migration: `alembic revision --autogenerate -m "description"`, apply: `alembic upgrade head`.
- **New LLM function tool**: Implement `FunctionTool` trait in `src/llm/function_tools/`, register in `src/main.rs` tools vec. See `MathTool`, `ChatHistoryTool` for examples.

## Key files reference
- Config loading: `src/config.rs` (priority chain, defaults, env overrides)
- Bot pipeline: `src/bot_adapter/adapter.rs` (WebSocket, message loop), `src/bot_adapter/event.rs` (dispatch)
- Message models: `src/bot_adapter/models/event_model.rs` (MessageType enum), `src/bot_adapter/models/message.rs` (typed messages)
- State: `src/util/message_store.rs` (Redis cache, MySQL persistence, in-memory fallback with auto-reconnect)
- Dataflow: `src/node/mod.rs` (Node trait, NodeGraph, Port, DataValue), `src/node/util_nodes.rs` (utility nodes), `src/llm/node_impl.rs` (LLM-based nodes), `src/bot_adapter/message_event_to_string.rs` (concrete node example)
- LLM: `src/llm/llm_api.rs` (HTTP client), `src/llm/agent/brain.rs` (BrainAgent with tool orchestration), `src/llm/function_tools/` (tool implementations)
- UI: `src/ui/node_graph_view.rs` (main UI logic), `src/ui/graph_window.slint` (Slint UI definition), `src/ui/window_state.rs` (persistent window config), `src/ui/selection.rs` (node selection logic)
- Database: `database/models/message_record.py` (SQLAlchemy model), `migrations/versions/` (alembic migrations), `alembic.ini` (migration config)
- Module guides: `.github/copilot-instructions-{config,adapter,event,models,node,utils,message-store}.md`
- Node graph JSON spec: `document/node-graph-json.md`

