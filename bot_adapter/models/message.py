from abc import ABC, abstractmethod
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


def convert_message_from_json(json_data: dict) -> MessageBase:
    message_type: str | None = json_data.get("type")
    message_data: dict | None = json_data.get("data")
    if message_data is None:
        raise ValueError("Message data is missing")
    if message_type == "text":
        text: str = message_data.get("text", "")
        return PlainTextMessage(text=text)
    elif message_type == "at":
        target: int = message_data.get("target")
        return AtTargetMessage(target_id=target)
        
    else:
        raise ValueError(f"Unsupported message type: {message_type}")