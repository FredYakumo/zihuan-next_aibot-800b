from abc import ABC, abstractmethod
from pydantic import BaseModel

class MessageBase(BaseModel, ABC):
    text: str

    def __str__(self) -> str:
        return self.text

    @abstractmethod
    def get_type(self) -> str:
        pass

class PlainTextMessage(MessageBase):
    def __str__(self) -> str:
        return self.text

    def get_type(self) -> str:
        return "text"


def convert_message_from_json(json_data: dict) -> MessageBase:
    message_type: str | None = json_data.get("type")
    message_data: dict | None = json_data.get("data")
    if message_data is None:
        raise ValueError("Message data is missing")
    if message_type == "text":
        text: str = message_data.get("text", "")
        return PlainTextMessage(text=text)
    else:
        raise ValueError(f"Unsupported message type: {message_type}")