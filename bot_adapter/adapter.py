from utils.logging_config import logger
import websockets
import json
import event

class BotAdapter:
    def __init__(self, url: str, token: str):
        self.url = url
        self.token = token
        
        
        self.event_process_func = {
            "private": event.process_friend_message,
            "group": event.process_group_message
        }

    def start(self):
        async def connect():
            async with websockets.connect(self.url, additional_headers={"Authorization": f"Bearer {self.token}"}) as websocket:
                logger.info("Connected to the bot server.")
                while True:
                    message = await websocket.recv()
                    self.bot_event_process(message)
                    
        import asyncio
        asyncio.run(connect())
    
    def bot_event_process(self, message: str | bytes):
        logger.debug(f"Received message: {message}")
        if not isinstance(message, str):
            logger.warning("Received non-string message, ignoring.")
            return
        message_json = json.loads(message)
        if "type" not in message_json:
            logger.error("Can't not infer message type")
        if message_json["type"] in self.event_process_func:
            self.event_process_func[message_json["type"]](message_json)