# Copilot Instructions: bot_adapter/adapter.py

## Purpose
Handles core message/event processing and platform integration (e.g., QQ, web).

## Key Patterns
- Central event loop for dispatching messages/events
- Platform-specific logic is abstracted for extensibility
- Implements hybrid RAG: fuses vector DB knowledge and chat logs for response generation
- Entry point for adding new chat platforms or tools

## How to Extend
- Add new platform: extend adapter logic and update config
- Add new event type: coordinate with event models

## Example
- See `handle_event` and platform dispatch logic for integration patterns
