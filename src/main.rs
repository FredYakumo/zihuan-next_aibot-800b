mod bot_adapter;
mod util;
mod llm;
mod config;

use log::{info, error, warn};
use log_util::log_util::LogUtil;
use lazy_static::lazy_static;
use clap::Parser;

use bot_adapter::adapter::BotAdapter;
use config::{load_config, build_redis_url, build_mysql_url};



lazy_static! {
    static ref BASE_LOG: LogUtil = LogUtil::new_with_path("zihuan_next_aibot", "logs");
}


#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short = 'l', long = "login-qq", help = "登录的QQ号（必填）")]
    qq_id: String,
}

#[tokio::main]
async fn main() {
    // Initialize logging using LogUtil
    LogUtil::init_with_logger(&BASE_LOG).expect("Failed to initialize logger");

    // Parse command line arguments
    let args = Args::parse();

    info!("zihuan_next_aibot-800b starting...");
    info!("登录的QQ号: {}", args.qq_id);

    // Load configuration from config.yaml, fallback to environment variables
    let config = load_config();

    // Build REDIS_URL from config
    let redis_url = build_redis_url(&config);

    if redis_url.is_some() {
        info!("Redis URL configured from config.yaml or environment");
    } else {
        warn!("No REDIS_URL or REDIS_HOST/PORT found in config.yaml; Redis will not be used.");
    }

    // Build DATABASE_URL (MySQL) from config
    let database_url = build_mysql_url(&config);

    if database_url.is_some() {
        info!("MySQL Database URL configured from config.yaml or environment");
    } else {
        warn!("No DATABASE_URL or MYSQL_* found in config.yaml; MySQL persistence will not be used.");
    }

    // Create and start the bot adapter
    let adapter = BotAdapter::new(
        config.bot_server_url,
        config.bot_server_token,
        redis_url,
        database_url,
        config.redis_reconnect_max_attempts,
        config.redis_reconnect_interval_secs,
        args.qq_id,
    ).await;
    info!("Bot adapter initialized, connecting to server...");
    if let Err(e) = adapter.start().await {
        error!("Bot adapter error: {}", e);
    }
}
