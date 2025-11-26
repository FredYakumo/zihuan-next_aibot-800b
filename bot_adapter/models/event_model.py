from typing import Any, Dict, List, Optional
from enum import Enum
from pydantic import BaseModel

from bot_adapter.models.message import MessageBase


class MessageType(str, Enum):
    PRIVATE = "private"
    GROUP = "group"
    
    def __str__(self) -> str:
        return self.value


class Sender(BaseModel):
    user_id: int
    nickname: str
    card: str
    role: Optional[str] = None
    


class MessageEvent(BaseModel):
    message_id: int
    message_type: MessageType
    sender: Sender
    message_list: List[MessageBase]