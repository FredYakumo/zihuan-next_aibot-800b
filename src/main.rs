mod bot_adapter;

use tracing::{info, error, Level};
use tracing_subscriber::FmtSubscriber;

use bot_adapter::adapter::BotAdapter;

#[tokio::main]
async fn main() {
    // Initialize logging
    let _subscriber = FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .init();

    info!("zihuan_next_aibot-800b starting...");

    // Configuration - TODO: Load from config file
    let bot_server_url = std::env::var("BOT_SERVER_URL")
        .unwrap_or_else(|_| "ws://localhost:3001".to_string());
    let bot_server_token = std::env::var("BOT_SERVER_TOKEN")
        .unwrap_or_default();

    // Create and start the bot adapter
    let adapter = BotAdapter::new(bot_server_url, bot_server_token);
    
    info!("Bot adapter initialized, connecting to server...");
    
    if let Err(e) = adapter.start().await {
        error!("Bot adapter error: {}", e);
    }
}
