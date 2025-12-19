# Copilot Instructions for zihuan-next_aibot-800b

> Keep this concise. Details live in code, tests, and the module notes under `.github/`.

## Architecture (big picture)
- Event-driven bot pipeline built in Rust.
    - **Config**: Centralized YAML-based configuration via `src/config.rs`. Loads from `config.yaml`, applies environment variable overrides, provides defaults (WebSocket URL: `ws://localhost:3001`).
    - **Inbound**: WebSocket to external QQ bot server using Authorization: Bearer token.
    - **Dispatch**: Route by `message_type` → handlers in `src/bot_adapter/event.rs`.
    - **Messages**: Deserialized into typed models (`PlainTextMessage`, `AtTargetMessage`, `ReplayMessage`). See `src/bot_adapter/models/`.
    - **State**: Recent raw messages cached via Redis (fallback to in-memory) for reply/context (`src/util/message_store.rs`).
    - **LLM**: `src/llm/*` provides HTTP-based chat API wrapper and agent scaffolding; integration tests are ignored by default and read `config.yaml`.
- Language division:
    - **Rust**: Primary for core business logic, bot adapters, message handling, LLM integration, configuration management.
    - **Python**: Database migrations (alembic) and data processing tasks.

## Critical workflows
- Setup:
    - `cp config.yaml.example config.yaml` (customize BOT_SERVER_URL, REDIS_*, LLM keys)
    - `docker compose -f docker/docker-compose.yaml up -d` (start Redis only)
- Development:
    - `cargo build` / `cargo run --` (invoke with `cargo run -- -l <qq_id>`)
    - `cargo test` (unit/integration tests; `cargo test -- --ignored` runs LLM integration tests)

## Project-specific conventions
- **Configuration**: Single source of truth in `config.yaml`. Loaded by `config::load_config()` in `src/main.rs`. Priority: file → env vars → defaults. Bot server URL and token guaranteed non-empty after load.
- **Redis**: Special chars in passwords percent-encoded (RFC 3986) by `config::pct_encode()`. Redis URL built by `config::build_redis_url()`.
- **Message store**: Always init early in `BotAdapter::new()`. Redis preferred; in-memory fallback logs warning. Get context with `MessageStore::get_message()` before LLM responses.
- **Deserialization**: Serde-based enums with lenient parsing—skips unsupported elements instead of failing entire event.
- **Logging**: Logs to `./logs` via `LogUtil` (`log_util` crate). Prefix message store logs with `[MessageStore]`.

## Extending the bot
- **New platform**: Add handler in `src/bot_adapter/event.rs`, register in `BotAdapter::new()`, extend `MessageType` enum in `src/bot_adapter/models/event_model.rs`.
- **Configuration changes**: Modify `Config` struct fields in `src/config.rs`, update `load_config()` priority logic if needed, document in `config.yaml.example`.
- **Message persistence**: Currently Redis cache only. Add database layer (see `database/models/message_record.py`) for long-term analytics.

## Key files reference
- Config loading: `src/config.rs` (priority chain, defaults, env overrides)
- Bot pipeline: `src/bot_adapter/adapter.rs` (WebSocket connection), `src/bot_adapter/event.rs` (dispatch)
- Message models: `src/bot_adapter/models/event_model.rs` (MessageType enum), `src/bot_adapter/models/message.rs` (message types)
- State: `src/util/message_store.rs` (Redis/memory cache)
- LLM: `src/llm/llm_api.rs`, `src/llm/agent/brain.rs` (chat API, agent tools)
- Module guides: `.github/copilot-instructions-{config,adapter,event,models,utils}.md`

