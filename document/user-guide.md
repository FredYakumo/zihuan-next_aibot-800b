# User Guide

> **Target audience:** End users who want to install, configure, and run the zihuan-next bot application.

This guide explains how to get the application running on your system.

---

## Table of Contents

- [User Guide](#user-guide)
  - [Table of Contents](#table-of-contents)
  - [Installation](#installation)
    - [Option A: Use Pre-built Binaries](#option-a-use-pre-built-binaries)
    - [Option B: Build from Source](#option-b-build-from-source)
  - [Configuration](#configuration)
    - [Prerequisites](#prerequisites)
    - [Config File](#config-file)
  - [Running the Application](#running-the-application)
    - [Method 1: GUI Mode (Visual Editor)](#method-1-gui-mode-visual-editor)
    - [Method 2: Headless Mode (CLI / Production)](#method-2-headless-mode-cli--production)

---

## Installation

### Option A: Use Pre-built Binaries

1.  Download the latest release package from the repository's Releases page.
2.  Extract the archive to a folder of your choice.
3.  Ensure the folder contains the executable and the configuration files.
4.  Start the application:
    - **Windows:** Double-click `zihuan_next.exe` or run in terminal:
      ```powershell
      .\zihuan_next.exe
      ```
    - **Linux:** In terminal, run:
      ```bash
      ./zihuan_next
      ```
    - **macOS:** In terminal, run:
      ```bash
      ./zihuan_next
      ```




### Option B: Build from Source

If you are a developer or want the latest changes:

1.  **Install Rust:** Ensure you have the Rust toolchain installed (1.70+).
2.  **Clone the repository:**
    ```bash
    git clone <repository-url>
    cd zihuan-next_aibot-800b
    ```
3.  **Build the release binary:**
    ```bash
    cargo build --release
    ```
    The executable will be located in `./target/release/`.
    *   Windows: `zihuan_next.exe`
    *   Linux/macOS: `zihuan_next`

---

## Configuration

### Prerequisites

Before running, ensure optional dependencies are ready if you need them:

1.  **Redis**: For message caching (recommended for performance).
2.  **MySQL**: For long-term message persistence.
    ```bash
    # Start Redis and MySQL using Docker (easiest method)
    docker compose -f docker/docker-compose.yaml up -d
    
    # Initialize database schema (if using MySQL)
    alembic upgrade head
    ```

### Config File

The application requires a `config.yaml` file.

1.  Copy the example config:
    ```bash
    cp config.yaml.example config.yaml
    ```
2.  Edit `config.yaml` to set your specific values:
    - **BOT_SERVER_URL**: Your QQ bot's WebSocket interface.
    - **TOKEN**: Authentication token.
    - **REDIS_URL / MYSQL_URL**: Database connection strings.
    - **LLM_API_BASE**: URL for your LLM provider (e.g., OpenAI, Local LLM).

---

## Running the Application

### Method 1: GUI Mode (Visual Editor)

**Use this mode to create, edit, and test node graphs visually.**

**How to run:**
- **Windows:** Double-click `zihuan_next_aibot-800b.exe`.
- **Command Line:**
    ```bash
    ./zihuan_next_aibot-800b
    ```

**What happens:**
1.  A window opens displaying the node graph editor.
2.  You can drag nodes from the palette, connect them, and verify logic.
3.  Use "Save Graph" to export your workflow to a JSON file (e.g., `bot.json`).

### Method 2: Headless Mode (CLI / Production)

**Use this mode to run a saved bot workflow in the background.**

**How to run:**
You must provide the graph file and the `--no-gui` flag via the command line.

**Windows (PowerShell/CMD):**
```powershell
.\zihuan_next_aibot-800b.exe --graph-json bot.json --no-gui
```

**Linux/macOS:**
```bash
./zihuan_next_aibot-800b --graph-json bot.json --no-gui
```

**Common Flags:**
- `--graph-json <path>`: Path to the JSON file defining your graph.
- `--no-gui`: Disables the window interface.
- `--save-graph-json <path>`: (Optional) Save a processed/validated version of the graph on exit.

**Stopping the bot:**
Press `Ctrl+C` in the terminal to gracefully shut down the application and close connections.
