use crate::bot_adapter::adapter::BotAdapter;
use crate::bot_adapter::models::message::MessageProp;
use crate::bot_adapter::models::MessageEvent;
use crate::error::Result;
use crate::llm::{Message, SystemMessage};
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;

/// Build system message based on bot profile and event context
pub fn build_system_message(bot_adapter: &BotAdapter, event: &MessageEvent, persona: &str) -> Message {
    let bot_profile = bot_adapter.get_bot_profile();

    if let Some(profile) = bot_profile {
        if event.is_group_message {
            SystemMessage(format!(
                "你是\"{}\"，QQ号是\"{}\"。群\"{}\"里的一个叫\"{}\"(QQ号: \"{}\")的人给你发送了一条消息。你的性格是: {}, 你需要根据消息内容决定做出反应或者无反应，其中你做出的反应需要委派给相应的Agent智能体(通过function tools)来完成",
                profile.nickname,
                profile.qq_id,
                event.group_name.clone().unwrap_or_default(),
                if !event.sender.card.is_empty() { event.sender.card.clone() } else { event.sender.nickname.clone() },
                event.sender.user_id,
                persona
            ))
        } else {
            SystemMessage(format!(
                "你是\"{}\"，QQ号是\"{}\"。你的好友\"{}\"(QQ号: \"{}\")给你发送了一条消息。你的性格是: {}, 你需要根据消息内容决定做出反应或者无反应，其中你做出的反应需要委派给相应的Agent智能体(通过function tools)来完成",
                profile.nickname, profile.qq_id, event.sender.nickname, event.sender.user_id, persona
            ))
        }
    } else {
        SystemMessage(format!(
            "你是\"紫幻\", QQ号是\"{}\"。你的性格是: {}, 你需要根据消息内容决定做出反应或者无反应，其中你做出的反应需要委派给相应的Agent智能体(通过function tools)来完成", 
            bot_adapter.get_bot_id(),
            persona
        ))
    }
}

/// Node that converts a MessageEvent to an LLM prompt message list
/// 
/// Inputs:
///   - message_event: MessageEvent containing message data
///   - bot_adapter: BotAdapterRef for building context-aware system message
///   - persona: Optional persona/character description (default: "默认助手")
/// 
/// Outputs:
///   - messages: MessageList containing system message and user message
pub struct ExtractMessageFromEventNode {
    id: String,
    name: String,
}

impl ExtractMessageFromEventNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for ExtractMessageFromEventNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Converts MessageEvent to LLM prompt string")
    }

    node_input![
        port! { name = "message_event", ty = MessageEvent, desc = "MessageEvent containing message data" },
        port! { name = "bot_adapter", ty = BotAdapterRef, desc = "BotAdapter reference for context-aware system message", required = true },
        port! { name = "persona", ty = String, desc = "Optional persona/character description (default: 默认助手)", optional },
    ];

    node_output![
        port! { name = "messages", ty = MessageList, desc = "MessageList containing system and user messages" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();

        if let Some(DataValue::MessageEvent(event)) = inputs.get("message_event") {
            let bot_adapter_ref = inputs.get("bot_adapter")
                .and_then(|v| {
                    if let DataValue::BotAdapterRef(adapter_ref) = v {
                        Some(adapter_ref.clone())
                    } else {
                        None
                    }
                })
                .ok_or("bot_adapter input is required")?;

            // Get persona from input, use default if not provided
            let persona = inputs.get("persona")
                .and_then(|v| {
                    if let DataValue::String(s) = v {
                        if s.is_empty() { None } else { Some(s.as_str()) }
                    } else {
                        None
                    }
                })
                .unwrap_or("默认助手");

            // Lock adapter and extract all needed information at the beginning
            let adapter = bot_adapter_ref.blocking_lock();
            let system_msg = build_system_message(&adapter, event, persona);
            
            let msg_prop = MessageProp::from_messages(&event.message_list, None);

            // Build user message from incoming MessageEvent
            let mut user_text = msg_prop.content.clone().unwrap_or_default();
            if let Some(ref_cnt) = msg_prop.ref_content.as_deref() {
                if !ref_cnt.is_empty() {
                    if !user_text.is_empty() {
                        user_text.push_str("\n\n");
                    }
                    user_text.push_str("[引用内容]\n");
                    user_text.push_str(ref_cnt);
                }
            }
            if user_text.trim().is_empty() {
                user_text = "(无文本内容，可能是仅@或回复)".to_string();
            }

            let user_msg = Message::user(user_text);

            // Combine system and user messages
            let messages = vec![system_msg, user_msg];
            outputs.insert("messages".to_string(), DataValue::MessageList(messages));
        } else {
            return Err("message_event input is required and must be MessageEvent type".into());
        }
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_event_to_string_node_creation() {
        let node = ExtractMessageFromEventNode::new("msg_to_str_1", "ExtractMessageFromEvent");
        assert_eq!(node.id(), "msg_to_str_1");
        assert_eq!(node.name(), "MessageEventToString");
    }

    #[test]
    fn test_input_output_ports() {
        let node = ExtractMessageFromEventNode::new("test", "test");
        let input_ports = node.input_ports();
        let output_ports = node.output_ports();

        assert_eq!(input_ports.len(), 3);
        assert_eq!(input_ports[0].name, "message_event");
        assert_eq!(input_ports[1].name, "bot_adapter");
        assert_eq!(input_ports[2].name, "persona");

        assert_eq!(output_ports.len(), 1);
        assert_eq!(output_ports[0].name, "messages");
    }
}
