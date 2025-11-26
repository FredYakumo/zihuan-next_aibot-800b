from abc import ABC, abstractmethod
from typing import Optional
from pydantic import BaseModel

class MessageBase(BaseModel, ABC):
    @abstractmethod
    def __str__(self) -> str:
        pass

    @abstractmethod
    def get_type(self) -> str:
        pass

class PlainTextMessage(MessageBase):
    text: str
    
    def __str__(self) -> str:
        return self.text

    def get_type(self) -> str:
        return "text"

class AtTargetMessage(MessageBase):
    target_id: int
    
    def __str__(self) -> str:
        return f"@{self.target_id}"
    
    def get_type(self) -> str:
        return "at"
    
class ReplayMessage(MessageBase):
    message_id: int
    message_source: Optional[MessageBase] = None
    
    def __str__(self) -> str:
        if self.message_source:
            return f"[Replay of message ID {self.message_id}: {str(self.message_source)}]"
        else:
            return f"[Replay of message ID {self.message_id}]"
    
    def get_type(self) -> str:
        return "replay"


def convert_message_from_json(json_data: dict) -> MessageBase:
    message_type: str | None = json_data.get("type")
    message_data: dict | None = json_data.get("data")
    if message_data is None:
        raise ValueError("Message data is missing")
    if message_type == "text":
        text: str = message_data.get("text", "")
        return PlainTextMessage(text=text)
    elif message_type == "at":
        target: int | None = message_data.get("target")
        if not target:
            target: int | None = message_data.get("qq")
        return AtTargetMessage(target_id=target or 0)
    elif message_type == "replay":
        message_id: int = message_data.get("id", 0)
        return ReplayMessage(message_id=message_id)
        
    else:
        raise ValueError(f"Unsupported message type: {message_type}")