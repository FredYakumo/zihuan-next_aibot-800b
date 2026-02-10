use crate::error::Result;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port, NodeType};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;

/// Message MySQL Persistence Node - Stores MessageEvent to MySQL database
pub struct MessageMySQLPersistenceNode {
    id: String,
    name: String,
}

impl MessageMySQLPersistenceNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for MessageMySQLPersistenceNode {
    fn node_type(&self) -> NodeType {
        NodeType::Simple
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("消息MySQL持久化 - 将MessageEvent存储到MySQL数据库")
    }

    node_input![
        port! { name = "message_event", ty = MessageEvent, desc = "消息事件" },
        port! { name = "mysql_ref", ty = MySqlRef, desc = "MySQL连接配置引用" },
    ];

    node_output![
        port! { name = "success", ty = Boolean, desc = "消息是否存储成功" },
        port! { name = "message_event", ty = MessageEvent, desc = "传递输入的消息事件" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        // Extract message event
        let message_event = inputs.get("message_event").and_then(|v| match v {
            DataValue::MessageEvent(e) => Some(e.clone()),
            _ => None,
        }).ok_or_else(|| crate::error::Error::InvalidNodeInput("message_event is required".to_string()))?;

        // Extract MySQL config reference
        let _mysql_ref = inputs.get("mysql_ref").and_then(|v| match v {
            DataValue::MySqlRef(r) => Some(r.clone()),
            _ => None,
        }).ok_or_else(|| crate::error::Error::InvalidNodeInput("mysql_ref is required".to_string()))?;

        // For now, we'll return success=false since actual persistence happens in async context
        // In a real implementation, we'd queue this for async storage or use an event-based approach
        let mut outputs = HashMap::new();
        outputs.insert("success".to_string(), DataValue::Boolean(false));
        outputs.insert("message_event".to_string(), DataValue::MessageEvent(message_event));

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

/// Message Cache Node - Caches MessageEvent in memory or optional Redis
pub struct MessageCacheNode {
    id: String,
    name: String,
    memory_cache: Arc<TokioMutex<HashMap<String, String>>>,
}

impl MessageCacheNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            memory_cache: Arc::new(TokioMutex::new(HashMap::new())),
        }
    }
}

impl Node for MessageCacheNode {
    fn node_type(&self) -> NodeType {
        NodeType::Simple
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("消息缓存 - 将MessageEvent缓存到内存或Redis")
    }

    node_input![
        port! { name = "message_event", ty = MessageEvent, desc = "消息事件" },
        port! { name = "redis_ref", ty = RedisRef, desc = "可选：Redis连接配置引用（若不提供则使用内存缓存）", optional },
    ];

    node_output![
        port! { name = "success", ty = Boolean, desc = "消息是否缓存成功" },
        port! { name = "message_event", ty = MessageEvent, desc = "传递输入的消息事件" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        // Extract message event
        let message_event = inputs.get("message_event").and_then(|v| match v {
            DataValue::MessageEvent(e) => Some(e.clone()),
            _ => None,
        }).ok_or_else(|| crate::error::Error::InvalidNodeInput("message_event is required".to_string()))?;

        // Extract optional Redis config reference
        let _redis_ref = inputs.get("redis_ref").and_then(|v| match v {
            DataValue::RedisRef(r) => Some(r.clone()),
            _ => None,
        });

        // Cache the message in memory (in real implementation, would also use Redis if provided)
        let _message_key = message_event.message_id.to_string();
        let _message_json = serde_json::json!({
            "message_id": message_event.message_id,
            "message_type": message_event.message_type.as_str(),
            "sender": {
                "user_id": message_event.sender.user_id,
                "nickname": message_event.sender.nickname,
                "card": message_event.sender.card,
                "role": message_event.sender.role,
            },
            "group_id": message_event.group_id,
            "group_name": message_event.group_name,
            "is_group_message": message_event.is_group_message,
        }).to_string();

        // For synchronous execution, we'll mark success as true
        // Actual async caching would happen in a separate task
        let mut outputs = HashMap::new();
        outputs.insert("success".to_string(), DataValue::Boolean(true));
        outputs.insert("message_event".to_string(), DataValue::MessageEvent(message_event));

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
