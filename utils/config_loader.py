import os
import yaml
from typing import Optional

from pydantic import BaseModel, Field
from utils.logging_config import logger


class Config(BaseModel):
    """Configuration model with explicit fields for IDE autocompletion."""

    BOT_SERVER_URL: str = Field(default="ws://localhost:3001", description="Bot server WebSocket URL")
    BOT_SERVER_TOKEN: Optional[str] = Field(default=None, description="Bot server authentication token")
    
    # Redis config fields
    REDIS_HOST: str = Field(default="127.0.0.1", description="Redis server host")
    REDIS_PORT: int = Field(default=6379, description="Redis server port")
    REDIS_DB: int = Field(default=0, description="Redis database number")
    REDIS_PASSWORD: Optional[str] = Field(default=None, description="Redis password (optional)")

    # MySQL config fields
    MYSQL_HOST: str = Field(default="127.0.0.1", description="MySQL host")
    MYSQL_PORT: int = Field(default=3306, description="MySQL port")
    MYSQL_USER: str = Field(default="zihuan_user", description="MySQL user")
    MYSQL_PASSWORD: str = Field(default="your_mysql_password", description="MySQL password")
    MYSQL_DATABASE: str = Field(default="zihuan_database", description="MySQL database name")

    class Config:
        extra = "allow"  # Allow additional fields from YAML

    @property
    def SQLALCHEMY_DATABASE_URL(self) -> str:
        # Generate SQLAlchemy URL from MYSQL_XXX fields
        return (
            f"mysql+pymysql://{self.MYSQL_USER}:{self.MYSQL_PASSWORD}" 
            f"@{self.MYSQL_HOST}:{self.MYSQL_PORT}/{self.MYSQL_DATABASE}"
        )


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