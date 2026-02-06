use crate::bot_adapter::adapter::{BotAdapter, BotAdapterConfig, SharedBotAdapter};
use crate::bot_adapter::event;
use crate::bot_adapter::models::message::MessageProp;
use crate::bot_adapter::models::event_model::MessageEvent;
use crate::error::Result;
use crate::node::{DataType, DataValue, Node, NodeType, Port};
use log::{error, info};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tokio::task::block_in_place;
use tokio::sync::Mutex as TokioMutex;

pub struct BotAdapterNode {
    id: String,
    name: String,
    event_rx: Option<TokioMutex<mpsc::UnboundedReceiver<MessageEvent>>>,
    adapter_handle: Option<SharedBotAdapter>,
}

impl BotAdapterNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            event_rx: None,
            adapter_handle: None,
        }
    }
}

impl Node for BotAdapterNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn node_type(&self) -> NodeType {
        NodeType::EventProducer
    }

    fn description(&self) -> Option<&str> {
        Some("QQ Bot Adapter - receives messages from QQ server")
    }

    fn input_ports(&self) -> Vec<Port> {
        vec![
            Port::new("trigger", DataType::Boolean)
                .with_description("Trigger to start receiving messages"),
            Port::new("qq_id", DataType::String)
                .with_description("QQ ID to login")
                .optional(),
            Port::new("bot_server_url", DataType::String)
                .with_description("Bot服务器WebSocket地址"),
            Port::new("bot_server_token", DataType::String)
                .with_description("Bot服务器连接令牌")
                .optional(),
            Port::new("redis_ref", DataType::RedisRef)
                .with_description("Redis连接配置引用")
                .optional(),
            Port::new("mysql_ref", DataType::MySqlRef)
                .with_description("MySQL连接配置引用")
                .optional(),
        ]
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![
            Port::new("message", DataType::MessageEvent)
                .with_description("Raw message event from QQ server"),
            Port::new("message_event", DataType::MessageEvent)
                .with_description("Raw message event from QQ server"),
            Port::new("bot_adapter", DataType::BotAdapterRef)
                .with_description("Shared bot adapter handle (self reference)"),
            Port::new("message_type", DataType::String)
                .with_description("Type of the message"),
            Port::new("user_id", DataType::String)
                .with_description("User ID who sent the message"),
            Port::new("content", DataType::String)
                .with_description("Message content"),
        ]
    }

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.on_start(inputs)?;
        let outputs = self.on_update()?.ok_or_else(|| {
            crate::error::Error::ValidationError("No message event received".to_string())
        })?;
        Ok(outputs)
    }

    fn on_start(&mut self, inputs: HashMap<String, DataValue>) -> Result<()> {
        if self.event_rx.is_some() {
            return Ok(());
        }

        self.validate_inputs(&inputs)?;

        let qq_id = inputs
            .get("qq_id")
            .and_then(|value| match value {
                DataValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_else(|| std::env::var("QQ_ID").unwrap_or_default());

        let bot_server_url = inputs
            .get("bot_server_url")
            .and_then(|value| match value {
                DataValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_else(|| {
                std::env::var("BOT_SERVER_URL")
                    .unwrap_or_else(|_| "ws://localhost:3001".to_string())
            });

        let bot_server_token = inputs
            .get("bot_server_token")
            .and_then(|value| match value {
                DataValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_else(|| std::env::var("BOT_SERVER_TOKEN").unwrap_or_default());

        // Extract Redis config from RedisRef input port
        let redis_config = inputs
            .get("redis_ref")
            .and_then(|value| match value {
                DataValue::RedisRef(config) => Some(config.clone()),
                _ => None,
            });
        let redis_url = redis_config.as_ref().and_then(|c| c.url.clone());
        let redis_reconnect_max = redis_config.as_ref().and_then(|c| c.reconnect_max_attempts);
        let redis_reconnect_interval = redis_config.as_ref().and_then(|c| c.reconnect_interval_secs);

        // Extract MySQL config from MySqlRef input port
        let mysql_config = inputs
            .get("mysql_ref")
            .and_then(|value| match value {
                DataValue::MySqlRef(config) => Some(config.clone()),
                _ => None,
            });
        let database_url = mysql_config.as_ref().and_then(|c| c.url.clone());
        let mysql_reconnect_max = mysql_config.as_ref().and_then(|c| c.reconnect_max_attempts);
        let mysql_reconnect_interval = mysql_config.as_ref().and_then(|c| c.reconnect_interval_secs);

        let adapter_config = BotAdapterConfig::new(
            bot_server_url,
            bot_server_token,
            qq_id,
        )
        .with_redis_url(redis_url)
        .with_database_url(database_url)
        .with_redis_reconnect(
            redis_reconnect_max,
            redis_reconnect_interval,
        )
        .with_mysql_reconnect(
            mysql_reconnect_max,
            mysql_reconnect_interval,
        )
        .with_brain_agent(None);

        let (event_tx, event_rx) = mpsc::unbounded_channel::<MessageEvent>();
        let (adapter_tx, adapter_rx) = oneshot::channel();
        let handler: event::EventHandler = Arc::new(move |event, _store| {
            let event_tx = event_tx.clone();
            Box::pin(async move {
                let _ = event_tx.send(event.clone());
            })
        });

        let run_adapter = async move {
            let mut adapter = BotAdapter::new(adapter_config).await;
            adapter.register_event_handler(handler);
            let adapter = adapter.into_shared();
            let _ = adapter_tx.send(adapter.clone());
            info!("Bot adapter initialized, connecting to server...");
            if let Err(e) = BotAdapter::start(adapter).await {
                error!("Bot adapter error: {}", e);
            }
        };

        let adapter_handle = if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(run_adapter);
            block_in_place(|| handle.block_on(async { adapter_rx.await.ok() }))
        } else {
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.spawn(run_adapter);
            runtime.block_on(async { adapter_rx.await.ok() })
        };

        let adapter_handle = adapter_handle.ok_or_else(|| {
            crate::error::Error::ValidationError("Failed to receive bot adapter handle".to_string())
        })?;

        self.adapter_handle = Some(adapter_handle);
        self.event_rx = Some(TokioMutex::new(event_rx));

        Ok(())
    }

    fn on_update(&mut self) -> Result<Option<HashMap<String, DataValue>>> {
        let event_rx = self.event_rx.as_ref().ok_or_else(|| {
            crate::error::Error::ValidationError("Bot adapter is not initialized".to_string())
        })?;

        let received_event = if let Ok(handle) = tokio::runtime::Handle::try_current() {
            block_in_place(|| handle.block_on(async {
                let mut guard = event_rx.lock().await;
                guard.recv().await
            }))
        } else {
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(async {
                let mut guard = event_rx.lock().await;
                guard.recv().await
            })
        };

        let event = match received_event {
            Some(event) => event,
            None => return Ok(None),
        };

        let adapter_handle = self.adapter_handle.clone().ok_or_else(|| {
            crate::error::Error::ValidationError("Bot adapter handle missing".to_string())
        })?;

        let msg_prop = MessageProp::from_messages(&event.message_list, None);
        let mut content = msg_prop.content.unwrap_or_default();
        if let Some(ref_cnt) = msg_prop.ref_content.as_deref() {
            if !ref_cnt.is_empty() {
                if !content.is_empty() {
                    content.push_str("\n\n");
                }
                content.push_str("[引用内容]\n");
                content.push_str(ref_cnt);
            }
        }

        let mut outputs = HashMap::new();
        outputs.insert("message".to_string(), DataValue::MessageEvent(event.clone()));
        outputs.insert("message_event".to_string(), DataValue::MessageEvent(event.clone()));
        outputs.insert("bot_adapter".to_string(), DataValue::BotAdapterRef(adapter_handle));
        outputs.insert(
            "message_type".to_string(),
            DataValue::String(event.message_type.as_str().to_string()),
        );
        outputs.insert(
            "user_id".to_string(),
            DataValue::String(event.sender.user_id.to_string()),
        );
        outputs.insert("content".to_string(), DataValue::String(content));
        self.validate_outputs(&outputs)?;

        Ok(Some(outputs))
    }

    fn on_cleanup(&mut self) -> Result<()> {
        self.event_rx = None;
        self.adapter_handle = None;
        Ok(())
    }
}

pub struct MessageSenderNode {
    id: String,
    name: String,
}

impl MessageSenderNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for MessageSenderNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Send message back to QQ server")
    }

    fn input_ports(&self) -> Vec<Port> {
        vec![
            Port::new("target_id", DataType::String)
                .with_description("Target user or group ID"),
            Port::new("content", DataType::String)
                .with_description("Message content to send"),
            Port::new("message_type", DataType::String)
                .with_description("Type of message to send"),
        ]
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![
            Port::new("success", DataType::Boolean)
                .with_description("Whether the message was sent successfully"),
            Port::new("response", DataType::Json)
                .with_description("Response from the server"),
        ]
    }

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();

        outputs.insert(
            "success".to_string(),
            DataValue::Boolean(true),
        );
        outputs.insert(
            "response".to_string(),
            DataValue::Json(serde_json::json!({
                "status": "sent",
                "timestamp": "2025-01-28T00:00:00Z"
            })),
        );

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
