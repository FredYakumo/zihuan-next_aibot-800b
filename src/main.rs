mod bot_adapter;
mod util;
mod llm;
mod config;
mod error;
mod node;
mod ui;

use log::{info, error, warn};
use log_util::log_util::LogUtil;
use lazy_static::lazy_static;
use clap::Parser;
use config::load_config;



lazy_static! {
    static ref BASE_LOG: LogUtil = LogUtil::new_with_path("zihuan_next_aibot", "logs");
}


#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long = "graph-json", value_name = "PATH", help = "节点图JSON文件路径（非GUI模式下必需）")]
    graph_json: Option<String>,

    #[arg(long = "no-gui", help = "以非GUI模式运行节点图（需要--graph-json参数）")]
    no_gui: bool,
}

#[tokio::main]
async fn main() {
    // Initialize logging using LogUtil
    LogUtil::init_with_logger(&BASE_LOG).expect("Failed to initialize logger");

    // Initialize node registry
    if let Err(e) = node::registry::init_node_registry() {
        error!("Failed to initialize node registry: {}", e);
    } else {
        info!("Node registry initialized");
    }

    // Parse command line arguments
    let args = Args::parse();

    // Non-GUI mode: requires graph JSON file
    if args.no_gui {
        let graph_path = match args.graph_json {
            Some(path) => path,
            None => {
                error!("非GUI模式必须通过 --graph-json 参数指定节点图文件");
                return;
            }
        };

        info!("加载节点图文件: {}", graph_path);
        match node::load_graph_definition_from_json(&graph_path) {
            Ok(definition) => {
                if let Err(e) = execute_node_graph(definition).await {
                    error!("节点图执行失败: {}", e);
                }
            }
            Err(err) => {
                error!("加载节点图失败: {}", err);
            }
        }
        return;
    }

    // GUI mode: load graph if provided, otherwise start with empty graph
    let mut graph = if let Some(path) = args.graph_json.as_ref() {
        match node::load_graph_definition_from_json(path) {
            Ok(graph) => Some(graph),
            Err(err) => {
                error!("加载节点图失败: {}", err);
                return;
            }
        }
    } else {
        None
    };

    if let Some(graph) = graph.as_mut() {
        node::ensure_positions(graph);
    }

    if let Err(err) = ui::node_graph_view::show_graph(graph) {
        error!("UI渲染失败: {}", err);
    }
}

/// Execute a node graph loaded from JSON definition
async fn execute_node_graph(definition: node::NodeGraphDefinition) -> Result<(), Box<dyn std::error::Error>> {
    info!("构建节点图");
    let mut graph = node::registry::build_node_graph_from_definition(&definition)?;

    // Load LLM configuration for any LLM nodes that might be in the graph
    let config = load_config();
    if config.agent_model_api.is_none() || config.agent_model_name.is_none() {
        warn!("节点图中的LLM节点可能无法正常工作：缺少 agent_model_api 或 agent_model_name 配置");
    }

    info!("执行节点图");
    graph.execute()?;
    info!("节点图执行完成");

    Ok(())
}
