# --- LOG LEVEL DISCOVERY ---
def _discover_log_level() -> int:
    """Resolve log level with the following priority:
    1) Environment variable: LOG_LEVEL
    2) config.yaml key: log_level
    3) Default: logging.DEBUG
    Returns the logging level constant (e.g., logging.INFO).
    """
    level_str = os.getenv("LOG_LEVEL")
    if not level_str:
        # Try config.yaml
        try:
            import yaml
            project_root = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
            cfg_path = os.path.join(project_root, "config.yaml")
            if os.path.exists(cfg_path):
                with open(cfg_path, "r", encoding="utf-8") as f:
                    loaded = yaml.safe_load(f)
                    if isinstance(loaded, dict):
                        raw = loaded.get("log_level")
                        if isinstance(raw, str) and raw.strip():
                            level_str = raw.strip()
        except Exception:
            pass
    if not level_str:
        return logging.DEBUG
    # Map string to logging level
    level_str = level_str.strip().upper()
    level_map = {
        "CRITICAL": logging.CRITICAL,
        "ERROR": logging.ERROR,
        "WARNING": logging.WARNING,
        "INFO": logging.INFO,
        "DEBUG": logging.DEBUG,
        "NOTSET": logging.NOTSET,
    }
    return level_map.get(level_str, logging.DEBUG)
import logging
import os
from logging.handlers import TimedRotatingFileHandler
from typing import Optional


def _discover_log_dir() -> str:
    """Resolve log directory with the following priority:
    1) Environment variables: SCORE_MIND_LOG_DIR, then LOG_DIR
    2) config.yaml key: loger_path (or logger_path fallback)
    3) Default: ./logs under project root
    """
    # 1) Environment overrides
    env_dir: Optional[str] = (
        os.getenv("ZIHUAN_LOG_DIR")
        or os.getenv("LOG_DIR")
        or os.getenv("LOGGER_PATH")
        or os.getenv("LOGER_PATH")
    )
    if env_dir:
        return env_dir

    # 2) Try read config.yaml next to project root
    try:
        from typing import Any
        from typing import Optional as _Optional

        import yaml  # local import to avoid hard dependency at import time if unused

        project_root = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
        cfg_path = os.path.join(project_root, "config.yaml")
        if os.path.exists(cfg_path):
            with open(cfg_path, "r", encoding="utf-8") as f:
                loaded: Any = yaml.safe_load(f)
                if isinstance(loaded, dict):
                    raw: _Optional[str] = loaded.get("loger_path") or loaded.get("logger_path")  # type: ignore[reportUnknownMemberType]
                    if isinstance(raw, str) and raw.strip():
                        return raw
    except Exception:
        # Swallow and fallback
        pass

    # 3) Default
    return os.path.join(
        os.path.abspath(os.path.join(os.path.dirname(__file__), "..")), "logs"
    )


logger = logging.getLogger("zihuan")
log_level = _discover_log_level()
logger.setLevel(log_level)

log_dir = _discover_log_dir()
log_dir = os.path.abspath(os.path.expanduser(log_dir))

try:
    os.makedirs(log_dir, exist_ok=True)
except Exception:
    fallback_dir = os.path.join(
        os.path.abspath(os.path.join(os.path.dirname(__file__), "..")), "logs"
    )
    os.makedirs(fallback_dir, exist_ok=True)
    log_dir = fallback_dir

log_file = os.path.join(log_dir, "zihuan.log")

file_handler: Optional[TimedRotatingFileHandler] = None
try:
    file_handler = TimedRotatingFileHandler(
        filename=log_file,
        when="midnight",
        interval=1,
        backupCount=7,
        encoding="utf-8",
    )
    file_handler.setLevel(log_level)
except Exception:
    file_handler = None

console_handler = logging.StreamHandler()
console_handler.setLevel(log_level)

formatter = logging.Formatter("%(asctime)s - %(name)s - %(levelname)s - %(message)s")

# Avoid attaching duplicate handlers if re-imported
existing_types = {type(h) for h in logger.handlers}
if file_handler and TimedRotatingFileHandler not in existing_types:
    file_handler.setFormatter(formatter)
    logger.addHandler(file_handler)

if logging.StreamHandler not in existing_types:
    console_handler.setFormatter(formatter)
    logger.addHandler(console_handler)
