use std::sync::Arc;
use log::info;

use crate::bot_adapter::adapter::BotAdapter;
use crate::bot_adapter::models::MessageEvent;
use crate::bot_adapter::models::message::MessageProp;
use crate::llm::agent::{Agent, FunctionToolsAgent};
use crate::llm::{InferenceParam, LLMBase, Message, SystemMessage, UserMessage};
use crate::error::Result;
use crate::llm::function_tools::{ChatHistoryTool, NaturalLanguageReplyTool, FunctionTool};
use crate::util::message_store::MessageStore;
use tokio::sync::Mutex as TokioMutex;

/// ChatAgent: specialized agent for conversational interactions
/// 
/// This agent focuses on natural language conversation, using:
/// - ChatHistoryTool: to retrieve context from previous messages
/// - NaturalLanguageReplyTool: to generate contextual replies
pub struct ChatAgent {
    llm: Arc<dyn LLMBase + Send + Sync>,
    tools: Vec<Arc<dyn FunctionTool>>,
}

impl ChatAgent {
    pub fn new(llm: Arc<dyn LLMBase + Send + Sync>, message_store: Arc<TokioMutex<MessageStore>>) -> Self {
        let tools: Vec<Arc<dyn FunctionTool>> = vec![
            Arc::new(ChatHistoryTool::new(message_store)),
            Arc::new(NaturalLanguageReplyTool::new(llm.clone())),
        ];
        
        Self { llm, tools }
    }
}

/// Build system message for chat agent based on bot profile and event context
fn build_chat_system_message(bot_adapter: &BotAdapter, event: &MessageEvent) -> Message {
    let bot_profile = bot_adapter.get_bot_profile();
    
    if let Some(profile) = bot_profile {
        if event.is_group_message {
            SystemMessage(format!(
                "你是\"{}\"（QQ号: {}）。在群\"{}\"中，用户\"{}\"（QQ号: {}）向你发送了消息。\n\
                你的职责是进行自然对话。可以使用chat_history工具查询历史消息上下文，使用nl_reply工具生成回复。\n\
                请保持友好、有趣且符合角色设定的对话风格。",
                profile.nickname,
                profile.qq_id,
                event.group_name.clone().unwrap_or_default(),
                if !event.sender.card.is_empty() { event.sender.card.clone() } else { event.sender.nickname.clone() },
                event.sender.user_id
            ))
        } else {
            SystemMessage(format!(
                "你是\"{}\"（QQ号: {}）。你的好友\"{}\"（QQ号: {}）向你发送了消息。\n\
                你的职责是进行自然对话。可以使用chat_history工具查询历史消息上下文，使用nl_reply工具生成回复。\n\
                请保持友好、有趣且符合角色设定的对话风格。",
                profile.nickname,
                profile.qq_id,
                event.sender.nickname,
                event.sender.user_id
            ))
        }
    } else {
        SystemMessage(format!(
            "你是\"紫幻\"（QQ号: {}）。你的职责是进行自然对话。\n\
            可以使用chat_history工具查询历史消息上下文，使用nl_reply工具生成回复。\n\
            请保持友好、有趣且符合角色设定的对话风格。",
            bot_adapter.get_bot_id()
        ))
    }
}

impl Agent for ChatAgent {
    type Output = Result<String>;

    fn name(&self) -> &'static str {
        "chat_agent"
    }

    fn on_event(&self, bot_adapter: &mut BotAdapter, event: &MessageEvent) -> Self::Output {
        let msg_prop = MessageProp::from_messages(&event.message_list, Some(bot_adapter.get_bot_id()));

        // Build system prompt with conversation context
        let system_msg = build_chat_system_message(bot_adapter, event);

        // Build user message from incoming MessageEvent
        let mut user_text = msg_prop.content.unwrap_or_default();
        
        // Include referenced content if present
        if let Some(ref_cnt) = msg_prop.ref_content {
            if !ref_cnt.is_empty() {
                if !user_text.is_empty() {
                    user_text.push_str("\n\n");
                }
                user_text.push_str("[引用内容]\n");
                user_text.push_str(&ref_cnt);
            }
        }
        
        // Fallback for empty content
        if user_text.trim().is_empty() {
            user_text = "(无文本内容，可能是仅@或回复)".to_string();
        }

        let mut chat_message_list = vec![system_msg, UserMessage(user_text)];

        info!("[ChatAgent] llm [{}] inference...", self.llm.get_model_name());
        
        // Tool calling loop: continue until LLM returns a response without tool calls
        let max_iterations = 5;
        let mut iteration = 0;
        let mut final_response = String::new();
        
        loop {
            iteration += 1;
            if iteration > max_iterations {
                info!("[ChatAgent] reached max iterations ({}), stopping tool calling loop", max_iterations);
                if final_response.is_empty() {
                    final_response = "抱歉，处理超时了...".to_string();
                }
                break;
            }

            let response = self.llm.inference(&InferenceParam {
                messages: &chat_message_list,
                tools: Some(&self.tools),
            });

            // If no tool calls, LLM has finished processing
            if response.tool_calls.is_empty() {
                info!("[ChatAgent] no tool calls in response, conversation complete");
                if let Some(content) = response.content {
                    info!("[ChatAgent] final response: {}", content);
                    final_response = content;
                }
                break;
            }

            // Execute each tool call and collect results
            info!("[ChatAgent] processing {} tool call(s)", response.tool_calls.len());
            
            // Add assistant's response with tool calls to message history
            chat_message_list.push(response);

            // Clone tool_calls to avoid borrow checker issues when mutating message list
            let tool_calls_to_execute = chat_message_list.last().unwrap().tool_calls.clone();
            
            // Execute tools and collect their results
            for tool_call in &tool_calls_to_execute {
                info!("[ChatAgent] executing tool: {}({}) [{}]", 
                    tool_call.function.name, 
                    tool_call.function.arguments.to_string().as_str(),
                    tool_call.id);
                
                if let Some(tool) = self.tools.iter().find(|t| t.name() == tool_call.function.name) {
                    match tool.call(tool_call.function.arguments.clone()) {
                        Ok(tool_response) => {
                            info!("[ChatAgent] tool [{}] executed successfully", tool_call.function.name);
                            
                            // Add tool result as a tool message
                            let tool_msg = Message {
                                role: crate::llm::MessageRole::Tool,
                                content: Some(tool_response.to_string()),
                                tool_calls: Vec::new(),
                            };
                            chat_message_list.push(tool_msg);
                        }
                        Err(e) => {
                            info!("[ChatAgent] tool [{}] execution failed: {}", tool_call.function.name, e);
                            
                            // Add error message as tool result
                            let error_msg = Message {
                                role: crate::llm::MessageRole::Tool,
                                content: Some(format!("Error executing tool: {}", e)),
                                tool_calls: Vec::new(),
                            };
                            chat_message_list.push(error_msg);
                        }
                    }
                } else {
                    info!("[ChatAgent] tool [{}] not found", tool_call.function.name);
                    
                    // Add error message for missing tool
                    let error_msg = Message {
                        role: crate::llm::MessageRole::Tool,
                        content: Some(format!("Tool '{}' not found", tool_call.function.name)),
                        tool_calls: Vec::new(),
                    };
                    chat_message_list.push(error_msg);
                }
            }
            
            // Continue loop to get next LLM response with tool results
            info!("[ChatAgent] iteration {} complete, continuing with {} messages", 
                iteration, chat_message_list.len());
        }

        Ok(final_response)
    }

    fn on_agent_input(&self, messages: Vec<Message>) -> Self::Output {
        info!("[ChatAgent] processing agent input with {} message(s)", messages.len());
        
        if messages.is_empty() {
            return Ok("(无输入内容)".to_string());
        }

        let system_msg = SystemMessage(
            "你是一个聊天助手。请根据用户输入生成合适的回复。保持友好、自然的对话风格。".to_string()
        );
        
        let mut chat_message_list = vec![system_msg];
        chat_message_list.extend(messages);
        
        let max_iterations = 5;
        let mut iteration = 0;
        let mut final_response = String::new();
        
        loop {
            iteration += 1;
            if iteration > max_iterations {
                info!("[ChatAgent] agent input: reached max iterations");
                if final_response.is_empty() {
                    final_response = "处理超时".to_string();
                }
                break;
            }

            let response = self.llm.inference(&InferenceParam {
                messages: &chat_message_list,
                tools: Some(&self.tools),
            });

            if response.tool_calls.is_empty() {
                if let Some(content) = response.content {
                    final_response = content;
                }
                break;
            }

            chat_message_list.push(response);
            let tool_calls_to_execute = chat_message_list.last().unwrap().tool_calls.clone();
            
            for tool_call in &tool_calls_to_execute {
                if let Some(tool) = self.tools.iter().find(|t| t.name() == tool_call.function.name) {
                    match tool.call(tool_call.function.arguments.clone()) {
                        Ok(tool_response) => {
                            let tool_msg = Message {
                                role: crate::llm::MessageRole::Tool,
                                content: Some(tool_response.to_string()),
                                tool_calls: Vec::new(),
                            };
                            chat_message_list.push(tool_msg);
                        }
                        Err(e) => {
                            let error_msg = Message {
                                role: crate::llm::MessageRole::Tool,
                                content: Some(format!("Error: {}", e)),
                                tool_calls: Vec::new(),
                            };
                            chat_message_list.push(error_msg);
                        }
                    }
                }
            }
        }

        Ok(final_response)
    }
}

impl FunctionToolsAgent for ChatAgent {
    fn get_tools(&self) -> Vec<&dyn FunctionTool> {
        self.tools.iter().map(|t| t.as_ref()).collect()
    }
}
