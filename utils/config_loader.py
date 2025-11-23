import os
import yaml
from typing import Optional

from pydantic import BaseModel, Field
from utils.logging_config import logger


class Config(BaseModel):
    """Configuration model with explicit fields for IDE autocompletion."""
    
    BOT_SERVER_URL: str = Field(default="ws://localhost:3001", description="Bot server WebSocket URL")
    BOT_SERVER_TOKEN: Optional[str] = Field(default=None, description="Bot server authentication token")
    
    class Config:
        extra = "allow"  # Allow additional fields from YAML


class ConfigLoader:
    def __init__(self, config_path: str):
        if not os.path.exists(config_path):
            logger.warning(
                f"Config file {config_path} does not exist. Using default config."
            )
            self.config = Config()
            return
        
        with open(config_path) as file:
            logger.info(f"Loading config from {config_path}")
            yaml_data = yaml.safe_load(file) or {}
            self.config = Config(**yaml_data)
    
    def __getattr__(self, name: str):
        """Allow accessing config attributes directly from loader."""
        return getattr(self.config, name)


config = ConfigLoader("config.yaml")