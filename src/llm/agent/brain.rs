use std::sync::Arc;

use crate::bot_adapter::adapter::BotAdapter;
use crate::bot_adapter::models::MessageEvent;
use crate::bot_adapter::models::message::MessageProp;
use crate::llm::agent::Agent;
use crate::llm::{InferenceParam, LLMBase, Message, SystemMessage, UserMessage};
use crate::error::Result;
use crate::llm::function_tools::{MathTool, ChatHistoryTool, NaturalLanguageReplyTool, CodeWriterTool, FunctionTool};

pub struct BrainAgent {
    llm: Arc<dyn LLMBase + Send + Sync>,
}

impl BrainAgent {
    pub fn new(llm: Arc<dyn LLMBase + Send + Sync>) -> Self {
        Self { llm }
    }
}

impl Agent for BrainAgent {
    type Output = Result<()>;

    fn on_event(&self, bot_adapter: &mut BotAdapter, event: &MessageEvent) -> Self::Output {
        let msg_prop = MessageProp::from_messages(&event.message_list, Some(bot_adapter.get_bot_id()));

        let bot_profile = bot_adapter.get_bot_profile();

        // Build system prompt with conversation context
        let system_msg = if let Some(profile) = bot_profile {
            if event.is_group_message {
                SystemMessage(format!(
                    "你是\"{}\"，QQ号是\"{}\"。群\"{}\"里的一个叫\"{}\"(QQ号: \"{}\")的人给你发送了一条消息。\n你需要根据消息内容决定做出反应或者无反应，其中你做出的反应需要委派给相应的Agent智能体(通过function tools)来完成",
                    profile.nickname,
                    profile.qq_id,
                    event.group_name.clone().unwrap_or_default(),
                    if !event.sender.card.is_empty() { event.sender.card.clone() } else { event.sender.nickname.clone() },
                    event.sender.user_id
                ))
            } else {
                SystemMessage(format!(
                    "你是\"{}\"，QQ号是\"{}\"。请根据用户的消息进行回复。",
                    profile.nickname, profile.qq_id
                ))
            }
        } else {
            SystemMessage(format!("你是紫幻。请根据用户的消息进行回复。", bot_adapter.get_bot_id()))
        };

        // Build user message from incoming MessageEvent
        let mut user_text = msg_prop.content.unwrap_or_default();
        if let Some(ref_cnt) = msg_prop.ref_content {
            if !ref_cnt.is_empty() {
                if !user_text.is_empty() {
                    user_text.push_str("\n\n");
                }
                user_text.push_str("[引用内容]\n");
                user_text.push_str(&ref_cnt);
            }
        }
        if user_text.trim().is_empty() {
            // Fallback to a generic hint when content is empty
            user_text = "(无文本内容，可能是仅@或回复)".to_string();
        }

        // Register function tools that the LLM can delegate to
        let tools: Vec<Arc<dyn FunctionTool>> = vec![
            Arc::new(MathTool::new()),
            Arc::new(ChatHistoryTool::new()),
            Arc::new(NaturalLanguageReplyTool::new(self.llm.clone())),
            Arc::new(CodeWriterTool::new(self.llm.clone())),
        ];

        // First round of inference with tools available
        let _response = self.llm.inference(&InferenceParam {
            messages: vec![system_msg, UserMessage(user_text)],
            tools: Some(tools),
        });

        Ok(())
    }

    fn on_agent_input(&self, _input: Message) -> Self::Output {
        Ok(())
    }
}