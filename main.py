from utils.config_loader import config
from utils.logging_config import logger
from bot_adapter.adapter import BotAdapter

def main():
    adapter = BotAdapter(
        url=config.BOT_SERVER_URL,
        token=config.BOT_SERVER_TOKEN
    )
    adapter.start()

if __name__ == "__main__":
    main()