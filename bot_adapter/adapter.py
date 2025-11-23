from utils.logging_config import logger
import websockets

class BotAdapter:
    def __init__(self, url: str, token: str):
        self.url = url
        self.token = token
        
        
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
        logger.info(f"Received message: {message}")
        if not isinstance(message, str):
            logger.warning("Received non-string message, ignoring.")
            return
        # Process text message