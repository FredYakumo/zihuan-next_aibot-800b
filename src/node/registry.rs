use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use once_cell::sync::Lazy;
use crate::node::Node;
use crate::error::Result;

/// Node factory function type
pub type NodeFactory = Arc<dyn Fn(String, String) -> Box<dyn Node> + Send + Sync>;

/// Global node registry
pub struct NodeRegistry {
    factories: RwLock<HashMap<String, NodeFactory>>,
    metadata: RwLock<HashMap<String, NodeTypeMetadata>>,
}

#[derive(Debug, Clone)]
pub struct NodeTypeMetadata {
    pub type_id: String,
    pub display_name: String,
    pub category: String,
    pub description: String,
}

impl NodeRegistry {
    fn new() -> Self {
        Self {
            factories: RwLock::new(HashMap::new()),
            metadata: RwLock::new(HashMap::new()),
        }
    }

    /// Register a node type with its factory function
    pub fn register(
        &self,
        type_id: impl Into<String>,
        display_name: impl Into<String>,
        category: impl Into<String>,
        description: impl Into<String>,
        factory: NodeFactory,
    ) -> Result<()> {
        let type_id = type_id.into();
        let metadata = NodeTypeMetadata {
            type_id: type_id.clone(),
            display_name: display_name.into(),
            category: category.into(),
            description: description.into(),
        };

        self.factories.write().unwrap().insert(type_id.clone(), factory);
        self.metadata.write().unwrap().insert(type_id, metadata);
        Ok(())
    }

    /// Create a new node instance by type ID
    pub fn create_node(
        &self,
        type_id: &str,
        id: impl Into<String>,
        name: impl Into<String>,
    ) -> Result<Box<dyn Node>> {
        let factories = self.factories.read().unwrap();
        let factory = factories.get(type_id).ok_or_else(|| {
            crate::error::Error::ValidationError(format!("Node type '{}' not registered", type_id))
        })?;

        Ok(factory(id.into(), name.into()))
    }

    /// Get all registered node types
    pub fn get_all_types(&self) -> Vec<NodeTypeMetadata> {
        self.metadata.read().unwrap().values().cloned().collect()
    }

    /// Get node types by category
    pub fn get_types_by_category(&self, category: &str) -> Vec<NodeTypeMetadata> {
        self.metadata
            .read()
            .unwrap()
            .values()
            .filter(|meta| meta.category == category)
            .cloned()
            .collect()
    }

    /// Get all categories
    pub fn get_categories(&self) -> Vec<String> {
        let mut categories: Vec<_> = self
            .metadata
            .read()
            .unwrap()
            .values()
            .map(|meta| meta.category.clone())
            .collect();
        categories.sort();
        categories.dedup();
        categories
    }
}

/// Global singleton registry
pub static NODE_REGISTRY: Lazy<NodeRegistry> = Lazy::new(NodeRegistry::new);

/// Helper macro to register a node type
#[macro_export]
macro_rules! register_node {
    ($type_id:expr, $display_name:expr, $category:expr, $description:expr, $node_struct:ty) => {
        $crate::node::registry::NODE_REGISTRY
            .register(
                $type_id,
                $display_name,
                $category,
                $description,
                std::sync::Arc::new(|id: String, name: String| {
                    Box::new(<$node_struct>::new(id, name))
                }),
            )
            .unwrap();
    };
}

/// Initialize all node types in the registry
pub fn init_node_registry() -> Result<()> {
    use crate::node::util_nodes::{ConditionalNode, JsonParserNode};
    use crate::llm::node_impl::{LLMNode, AgentNode, TextProcessorNode};
    use crate::bot_adapter::node_impl::{BotAdapterNode, MessageSenderNode};
    use crate::bot_adapter::message_event_to_string::MessageEventToStringNode;
    use crate::node::database_nodes::{RedisNode, MySqlNode};

    // Utility nodes
    register_node!(
        "conditional",
        "条件分支",
        "工具",
        "根据条件选择不同的输出分支",
        ConditionalNode
    );

    register_node!(
        "json_parser",
        "JSON解析器",
        "工具",
        "将JSON字符串解析为结构化数据",
        JsonParserNode
    );

    // LLM nodes
    NODE_REGISTRY.register(
        "llm",
        "大语言模型",
        "AI",
        "调用大语言模型处理文本",
        Arc::new(|id: String, name: String| {
            Box::new(LLMNode::new(id, name))
        }),
    )?;

    NODE_REGISTRY.register(
        "agent",
        "AI Agent",
        "AI",
        "具有工具调用能力的智能代理",
        Arc::new(|id: String, name: String| {
            Box::new(AgentNode::new(id, name, "default"))
        }),
    )?;

    NODE_REGISTRY.register(
        "text_processor",
        "文本处理器",
        "工具",
        "对文本进行各种处理操作",
        Arc::new(|id: String, name: String| {
            Box::new(TextProcessorNode::new(id, name, "uppercase"))
        }),
    )?;

    // Bot adapter nodes
    register_node!(
        "bot_adapter",
        "QQ机器人适配器",
        "适配器",
        "接收来自QQ服务器的消息事件",
        BotAdapterNode
    );

    register_node!(
        "message_sender",
        "消息发送器",
        "Bot适配器",
        "向QQ服务器发送消息",
        MessageSenderNode
    );

    register_node!(
        "message_event_to_string",
        "消息转字符串",
        "Bot适配器",
        "将消息事件转换为LLM提示文本",
        MessageEventToStringNode
    );

    // Database nodes
    register_node!(
        "redis",
        "Redis连接",
        "数据库",
        "构建Redis连接配置",
        RedisNode
    );

    register_node!(
        "mysql",
        "MySQL连接",
        "数据库",
        "构建MySQL连接配置",
        MySqlNode
    );

    Ok(())
}

/// Build a NodeGraph from a NodeGraphDefinition
pub fn build_node_graph_from_definition(
    definition: &crate::node::graph_io::NodeGraphDefinition,
) -> Result<crate::node::NodeGraph> {
    let mut graph = crate::node::NodeGraph::new();

    // Create all nodes
    for node_def in &definition.nodes {
        let node = NODE_REGISTRY.create_node(
            &node_def.node_type,
            node_def.id.clone(),
            node_def.name.clone(),
        )?;
        graph.add_node(node)?;
    }

    Ok(graph)
}
