# Copilot Instructions for zihuan-next_aibot-800b

> Keep this concise. Details live in code, tests, and the module notes under `.github/`.

## Architecture (big picture)
- Event-driven bot pipeline built in Rust.
    - Inbound: WebSocket to external QQ bot server using Authorization: Bearer token.
    - Dispatch: route by `message_type` → handlers in `src/bot_adapter/event.rs`.
    - Messages: deserialized into typed models (`PlainTextMessage`, `AtTargetMessage`, `ReplayMessage`). See `src/bot_adapter/models/`.
    - State: recent raw messages cached via Redis (fallback to in-memory) for reply/context (`src/util/message_store.rs`).
    - LLM: `src/llm/*` provides HTTP-based chat API wrapper and agent scaffolding; integration tests are ignored by default and read `config.yaml`.
- Language division:
    - **Rust**: primary language for core business logic, bot adapters, message handling, LLM integration, and model applications.
    - **Python**: used for database migrations (alembic) and data processing tasks.

## Critical workflows
- Rust
    - cp config.yaml.example config.yaml (set BOT_SERVER_URL, REDIS_*, and LLM keys if used)
    - docker compose -f docker/docker-compose.yaml up -d  # starts Redis only
    - cargo build | cargo run
    - cargo test
    - cargo test -- --ignored  # runs LLM integration tests using config.yaml fields: natural_language_model_* and agent_model_*

## Project-specific conventions
- Single source of truth: `config.yaml`. Config is loaded in `src/main.rs` and Redis URL is constructed dynamically.
- Always init the message store early (see `BotAdapter::new()` in `src/bot_adapter/adapter.rs`). Redis is preferred; memory fallback logs a warning.
- Deserialization: serde-based enums with lenient parsing that skips unsupported message elements instead of failing the whole event.
- Special chars in Redis passwords are percent-encoded (see `pct_encode()` in `src/main.rs`).
- Logging: logs to `./logs` via `LogUtil` (`log_util` crate).

## Extending the bot (pattern excerpts)
- New platform/type: add handler in `src/bot_adapter/event.rs` and register in `BotAdapter::new()`. Extend `MessageType` enum in `src/bot_adapter/models/event_model.rs`.
- Reply context: use `MessageStore::get_message()` to load the original referenced message before LLM/crafting a response.
- Message persistence: currently messages are stored in Redis cache; add database layer if needed for long-term analytics.

## References (start here)
- README.md (feature overview)
- .github/copilot-instructions-*.md (focused module notes): adapter, event, models, utils
- Key files: `src/main.rs`, `src/bot_adapter/adapter.rs`, `src/bot_adapter/event.rs`, `src/util/message_store.rs`, `src/llm/llm_api.rs`

