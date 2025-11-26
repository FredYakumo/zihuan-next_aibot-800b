from bot_adapter.models.event_model import MessageEvent
from utils.logging_config import logger

def process_friend_message(event: MessageEvent):
    logger.info(f"Sender: {event.sender.user_id}, Message: {[str(e) for e in event.message_list]}")


def process_group_message(event: MessageEvent):
    logger.info(f"Sender: {event.sender.user_id}, Message: {[str(e) for e in event.message_list]}")