# Copilot Instructions for zihuan-next_aibot-800b

> Keep this concise. Details live in code, tests, and the module notes under `.github/`.

## Architecture (big picture)
- **Event-driven bot pipeline** built in Rust with pluggable dataflow nodes.
    - **Config**: Centralized YAML-based configuration via `src/config.rs`. Loads from `config.yaml`, applies environment variable overrides, provides defaults (WebSocket URL: `ws://localhost:3001`).
    - **Inbound**: WebSocket to external QQ bot server using Authorization: Bearer token.
    - **Dispatch**: Route by `message_type` → handlers in `src/bot_adapter/event.rs`.
    - **Messages**: Deserialized into typed models (`PlainTextMessage`, `AtTargetMessage`, `ReplyMessage`). See `src/bot_adapter/models/`.
    - **State**: Three-tier message storage via `MessageStore` in `src/util/message_store.rs`:
        - **Redis**: Recent messages cache for fast reply/context lookup (cleared on startup)
        - **MySQL**: Persistent long-term storage via SQLx pool (`message_record` table, managed by Python/alembic migrations)
        - **In-memory**: Automatic fallback when Redis/MySQL unavailable
    - **LLM**: `src/llm/*` provides HTTP-based chat API wrapper, agent scaffolding, and function tools; integration tests are ignored by default and read `config.yaml`.
    - **Dataflow Nodes**: `src/node/*` defines a composable node graph system (trait `Node`, struct `NodeGraph`). Node graphs are built from port-name bindings (no explicit edges) and executed as a DAG; `NodeGraph::execute()` validates dependencies, topologically sorts, then runs nodes in order.
- Language division:
    - **Rust**: Core business logic, bot adapters, message handling, LLM integration, configuration, dataflow system.
    - **Python**: Database migrations (alembic) and data processing tasks.

## Critical workflows
- Setup:
    - `cp config.yaml.example config.yaml` (customize BOT_SERVER_URL, REDIS_*, LLM keys)
    - `docker compose -f docker/docker-compose.yaml up -d` (start Redis)
    - Optionally configure MySQL (MYSQL_HOST/PORT/USER/PASSWORD/DATABASE or DATABASE_URL) for persistent message storage
    - Run migrations: `alembic upgrade head` (if using MySQL persistence)
- Development:
    - `cargo build` / `cargo run -- -l <qq_id>` (run bot with QQ ID)
    - `cargo test` (unit/integration tests; `cargo test -- --ignored` runs LLM integration tests)
- Testing dataflow nodes: Create a `NodeGraph`, add nodes implementing `Node` trait, bind input/output port names, then call `graph.execute()` (validates port dependencies, topologically sorts, executes in order, passes outputs as inputs).

## Project-specific conventions
- **Configuration**: Single source of truth in `config.yaml`. Loaded by `config::load_config()` in `src/main.rs`. Priority: file → env vars → defaults. Bot server URL and token guaranteed non-empty after load.
- **Redis**: Special chars in passwords percent-encoded (RFC 3986) by `config::pct_encode()`. Redis URL built by `config::build_redis_url()`. Redis is flushed (`FLUSHDB`) on startup.
- **MySQL**: Database URL built by `config::build_mysql_url()`. Schema managed via Python/alembic migrations in `migrations/`. SQLx pool created on store init.
- **Message store**: Always init early in `BotAdapter::new()` with both Redis and MySQL URLs. Redis for cache, MySQL for persistence, in-memory fallback logs warning. Get context with `MessageStore::get_message()` (Redis cache) or `MessageStore::get_message_record()` (MySQL) before LLM responses. Historical queries use `MessageStore::query_messages()` (MySQL).
- **Deserialization**: Serde-based enums with lenient parsing—skips unsupported elements instead of failing entire event.
- **Logging**: Logs to `./logs` via `LogUtil` (`log_util` crate). Prefix message store logs with `[MessageStore]`.
- **Dataflow nodes**: Port names are the binding key (no explicit edges). The graph is a DAG inferred from port bindings; cycles or missing required inputs fail validation. `NodeGraph::execute()` validates, topologically sorts, and executes nodes. EventProducer nodes use `on_start`/`on_update` lifecycle hooks. See `src/node/mod.rs` (core trait), `src/node/util_nodes.rs` (ConditionalNode, JsonParserNode), `src/llm/node_impl.rs` (LLMNode, AgentNode, TextProcessorNode), `src/bot_adapter/message_event_to_string.rs` (node example).

## Extending the bot
- **New platform**: Add handler in `src/bot_adapter/event.rs`, register in `BotAdapter::new()`, extend `MessageType` enum in `src/bot_adapter/models/event_model.rs`.
- **New node type**: Implement `Node` trait (define `id()`, `name()`, `input_ports()`, `output_ports()`, `execute()`). See `ConditionalNode` in `src/node/util_nodes.rs` for minimal example.
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
- Database: `database/models/message_record.py` (SQLAlchemy model), `migrations/versions/` (alembic migrations), `alembic.ini` (migration config)
- Module guides: `.github/copilot-instructions-{config,adapter,event,models,node,utils,message-store}.md`

