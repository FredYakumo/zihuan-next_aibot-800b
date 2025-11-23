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
                    logger.info(f"Received message: {message}")
                    
        import asyncio
        asyncio.run(connect())
        