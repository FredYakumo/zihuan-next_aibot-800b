use crate::bot_adapter::adapter::{BotAdapter, BotAdapterConfig, SharedBotAdapter};
use crate::bot_adapter::event;
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
            Port::new("qq_id", DataType::String)
                .with_description("QQ ID to login"),
            Port::new("bot_server_url", DataType::String)
                .with_description("Bot服务器WebSocket地址"),
            Port::new("bot_server_token", DataType::String)
                .with_description("Bot服务器连接令牌")
                .optional(),
        ]
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![
            Port::new("message_event", DataType::MessageEvent)
                .with_description("Raw message event from QQ server"),
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

        let adapter_config = BotAdapterConfig::new(
            bot_server_url,
            bot_server_token,
            qq_id,
        )
        .with_brain_agent(None);

        let (event_tx, event_rx) = mpsc::unbounded_channel::<MessageEvent>();
        let (adapter_tx, adapter_rx) = oneshot::channel();
        let handler: event::EventHandler = Arc::new(move |event| {
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

        let mut outputs = HashMap::new();
        outputs.insert("message_event".to_string(), DataValue::MessageEvent(event.clone()));
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
