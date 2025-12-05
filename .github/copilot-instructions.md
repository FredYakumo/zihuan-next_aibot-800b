

# Copilot Instructions for zihuan-next_aibot-800b

> **Documentation Principle:** Keep this file concise. Detailed descriptions are distributed in module-specific documentation.

> **Project Management:** This project uses [uv](https://github.com/astral-sh/uv) for dependency management.


## Quick Start

```bash
# Setup
uv sync
cp config.yaml.example config.yaml  # Edit: BOT_SERVER_URL, REDIS_*, MYSQL_*
cd docker/redis && docker-compose up -d
uv run alembic upgrade head
uv run python main.py
```


## Architecture

**Hybrid RAG chatbot** with event-driven design:


## Key Conventions

### Configuration

### Message Models

### Event Processing
```python
# BotAdapter dispatch pattern
self.event_process_func = {
    "private": event.process_friend_message,
    "group": event.process_group_message
}
```

### Database

### Logging


## Module References

Detailed patterns and examples:


## Common Tasks

**Add message type**: Subclass `MessageBase` → Update `convert_message_from_json()` → Handle in events  
**Add platform**: Extend `event_process_func` → Implement handler in `event.py`  
**Modify schema**: Edit `database/models/` → `uv run alembic revision --autogenerate -m "..."` → `uv run alembic upgrade head`

# Copilot Instructions for zihuan-next_aibot-800b

> **Keep concise. For details, see module docs and code.**

## Quick Start
```bash
# Setup (Python)
uv sync
cp config.yaml.example config.yaml  # Edit: BOT_SERVER_URL, REDIS_*, MYSQL_*
cd docker/redis && docker-compose up -d
uv run alembic upgrade head
uv run python main.py
# Setup (Rust)
cargo run
```

## Architecture & Data Flow
- **Hybrid RAG chatbot**: Combines vector DB knowledge graphs with real-time chat context (see README for RAG details)
- **Event-driven**: WebSocket → BotAdapter (Python: `bot_adapter/adapter.py`, Rust: `src/bot_adapter/adapter.rs`) → MessageEvent → platform handler
- **Storage**: Redis (cache), MySQL (history)
- **Platforms**: QQ (primary), web, edge
- **Cross-language**: Python and Rust implementations mirror each other; config and event flow are analogous

## Key Conventions & Patterns
- **Config**: Python: `utils/config_loader.py` (Pydantic), Rust: `src/main.rs` loads YAML/env. DB URL auto-built from `MYSQL_*`.
- **Logging**: Python: `utils/logging_config.py`, Rust: `LogUtil` (`src/main.rs`). Priority: env → config → default.
- **Message Models**: All inherit/implement `MessageBase` (see `bot_adapter/models/message.py` and `src/bot_adapter/models/message.rs`). Add new types by subclassing/enum variant + update deserialization.
- **Event Dispatch**: Handlers mapped by message type (see `event_process_func` in Python, `event_handlers` in Rust). Add platforms by extending handler map and implementing handler.
- **Database**: Alembic loads DB URL from config. Never hardcode in `alembic.ini`. Migrate: `uv run alembic revision --autogenerate -m "..."` → `uv run alembic upgrade head`.
- **MCP-inspired tools**: Implements tool-like patterns (see README, not full MCP server).

## Common Tasks
- **Add message type**: Subclass/enum `MessageBase` → update `convert_message_from_json` → handle in event logic
- **Add platform**: Extend handler map → implement handler in event module
- **Modify schema**: Edit `database/models/` → migrate with Alembic

## Key Files & Examples
- Python: `main.py`, `bot_adapter/`, `utils/`, `database/`
- Rust: `src/main.rs`, `src/bot_adapter/`

## Tips
- For RAG, see README for dual-source retrieval logic
- For config/logging, always check env > config > default
- For cross-language, keep Python/Rust logic in sync for event flow and models
