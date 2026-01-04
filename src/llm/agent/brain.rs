use std::sync::Arc;

use log::info;

use crate::bot_adapter::adapter::BotAdapter;
use crate::bot_adapter::models::MessageEvent;
use crate::bot_adapter::models::message::MessageProp;
use crate::llm::agent::Agent;
use crate::llm::{InferenceParam, LLMBase, Message, SystemMessage, UserMessage};
use crate::error::Result;
use crate::llm::function_tools::{MathTool, ChatHistoryTool, NaturalLanguageReplyTool, CodeWriterTool, FunctionTool};

pub struct BrainAgent {
    llm: Arc<dyn LLMBase + Send + Sync>,
    tools: Vec<Arc<dyn FunctionTool>>,
}

impl BrainAgent {
    pub fn new(llm: Arc<dyn LLMBase + Send + Sync>, tools: Vec<Arc<dyn FunctionTool>>) -> Self {
        Self { llm, tools }
    }
}

/// Build system message based on bot profile and event context
fn build_system_message(bot_adapter: &BotAdapter, event: &MessageEvent) -> Message {
    let bot_profile = bot_adapter.get_bot_profile();
    
    if let Some(profile) = bot_profile {
        if event.is_group_message {
            SystemMessage(format!(
                "你是\"{}\"，QQ号是\"{}\"。群\"{}\"里的一个叫\"{}\"(QQ号: \"{}\")的人给你发送了一条消息。你需要根据消息内容决定做出反应或者无反应，其中你做出的反应需要委派给相应的Agent智能体(通过function tools)来完成",
                profile.nickname,
                profile.qq_id,
                event.group_name.clone().unwrap_or_default(),
                if !event.sender.card.is_empty() { event.sender.card.clone() } else { event.sender.nickname.clone() },
                event.sender.user_id
            ))
        } else {
            SystemMessage(format!(
                "你是\"{}\"，QQ号是\"{}\"。你的好友\"{}\"(QQ号: \"{}\")给你发送了一条消息。你需要根据消息内容决定做出反应或者无反应，其中你做出的反应需要委派给相应的Agent智能体(通过function tools)来完成",
                profile.nickname, profile.qq_id, event.sender.nickname, event.sender.user_id
            ))
        }
    } else {
        SystemMessage(format!(
            "你是\"紫幻\", QQ号是\"{}\"。你需要根据消息内容决定做出反应或者无反应，其中你做出的反应需要委派给相应的Agent智能体(通过function tools)来完成", 
            bot_adapter.get_bot_id()
        ))
    }
}

impl Agent for BrainAgent {
    type Output = Result<()>;

    fn on_event(&self, bot_adapter: &mut BotAdapter, event: &MessageEvent) -> Self::Output {
        let msg_prop = MessageProp::from_messages(&event.message_list, Some(bot_adapter.get_bot_id()));

        // Build system prompt with conversation context
        let system_msg = build_system_message(bot_adapter, event);

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

        let mut brain_message_list = vec![system_msg, UserMessage(user_text)];

        info!("[BrainAgent] llm [{}] inference...", self.llm.get_model_name());
        
        // Tool calling loop: continue until LLM returns a response without tool calls
        let max_iterations = 5;
        let mut iteration = 0;
        
        loop {
            iteration += 1;
            if iteration > max_iterations {
                info!("[BrainAgent] reached max iterations ({}), stopping tool calling loop", max_iterations);
                break;
            }

            let response = self.llm.inference(&InferenceParam {
                messages: &brain_message_list,
                tools: Some(&self.tools),
            });

            // If no tool calls, LLM has finished processing
            if response.tool_calls.is_empty() {
                info!("[BrainAgent] no tool calls in response, conversation complete");
                if let Some(content) = response.content {
                    info!("[BrainAgent] final response: {}", content);
                }
                break;
            }

            // Execute each tool call and collect results
            info!("[BrainAgent] processing {} tool call(s)", response.tool_calls.len());
            
            // Add assistant's response with tool calls to message history
            brain_message_list.push(response);

            // Clone tool_calls to avoid borrow checker issues when mutating message list
            let tool_calls_to_execute = brain_message_list.last().unwrap().tool_calls.clone();
            
            // Execute tools and collect their results
            for tool_call in &tool_calls_to_execute {
                info!("[BrainAgent] executing tool: {}({}) [{}]", 
                    tool_call.function.name, 
                    tool_call.function.arguments.to_string().as_str(),
                    tool_call.id);
                
                if let Some(tool) = self.tools.iter().find(|t| t.name() == tool_call.function.name) {
                    match tool.call(tool_call.function.arguments.clone()) {
                        Ok(tool_response) => {
                            info!("[BrainAgent] tool [{}] executed successfully", tool_call.function.name);
                            
                            // Add tool result as a tool message
                            let tool_msg = Message {
                                role: crate::llm::MessageRole::Tool,
                                content: Some(tool_response.to_string()),
                                tool_calls: Vec::new(),
                            };
                            brain_message_list.push(tool_msg);
                        }
                        Err(e) => {
                            info!("[BrainAgent] tool [{}] execution failed: {}", tool_call.function.name, e);
                            
                            // Add error message as tool result
                            let error_msg = Message {
                                role: crate::llm::MessageRole::Tool,
                                content: Some(format!("Error executing tool: {}", e)),
                                tool_calls: Vec::new(),
                            };
                            brain_message_list.push(error_msg);
                        }
                    }
                } else {
                    info!("[BrainAgent] tool [{}] not found", tool_call.function.name);
                    
                    // Add error message for missing tool
                    let error_msg = Message {
                        role: crate::llm::MessageRole::Tool,
                        content: Some(format!("Tool '{}' not found", tool_call.function.name)),
                        tool_calls: Vec::new(),
                    };
                    brain_message_list.push(error_msg);
                }
            }
            
            // Continue loop to get next LLM response with tool results
            info!("[BrainAgent] iteration {} complete, continuing with {} messages", 
                iteration, brain_message_list.len());
        }

        Ok(())
    }

    fn on_agent_input(&self, _input: Message) -> Self::Output {
        Ok(())
    }
    
    fn name(&self) -> &'static str {
        "BrainAgent"
    }
}