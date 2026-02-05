use std::sync::Arc;

use log::info;

use crate::bot_adapter::adapter::BotAdapter;
use crate::bot_adapter::models::MessageEvent;
use crate::bot_adapter::models::message::MessageProp;
use crate::llm::agent::Agent;
use crate::llm::{InferenceParam, LLMBase, Message, UserMessage};
use crate::error::Result;
use crate::llm::function_tools::FunctionTool;

#[derive(Clone)]
pub struct BrainAgent {
    llm: Arc<dyn LLMBase + Send + Sync>,
    tools: Vec<Arc<dyn FunctionTool>>,
    persona: String,
}

impl BrainAgent {
    pub fn new(llm: Arc<dyn LLMBase + Send + Sync>, tools: Vec<Arc<dyn FunctionTool>>, persona: String) -> Self {
        Self { llm, tools, persona }
    }
}



fn should_reply(response_content: Option<&str>, msg_prop: &MessageProp, event: &MessageEvent) -> bool {
    let content = response_content.unwrap_or("").trim();
    if !content.is_empty() {
        let negative_markers = ["无反应", "不回复", "无需回复", "不用回复", "无需回应", "不需要回复"];
        if negative_markers.iter().any(|m| content.contains(m)) {
            return false;
        }

        let content_lower = content.to_lowercase();
        let positive_markers = ["回复", "回应", "答复", "reply", "respond"];
        if positive_markers.iter().any(|m| content_lower.contains(m)) {
            return true;
        }
    }

    if event.is_group_message {
        msg_prop.is_at_me
    } else {
        true
    }
}

impl Agent for BrainAgent {
    type Output = Result<()>;

    fn on_event(&self, bot_adapter: &mut BotAdapter, event: &MessageEvent) -> Self::Output {
        let msg_prop = MessageProp::from_messages(&event.message_list, Some(bot_adapter.get_bot_id()));

        // Build system prompt with conversation context
        let system_msg = crate::llm::prompt::brain::build_system_message(bot_adapter, event, self.persona.as_str());

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
            // Fallback to a generic hint when content is empty
            user_text = "(无文本内容，可能是仅@或回复)".to_string();
        }

        let _user_text_for_reply = user_text.clone();
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
                let response_content = response.content.clone();
                if let Some(content) = response_content.as_deref() {
                    info!("[BrainAgent] final response: {}", content);
                }

                if should_reply(response_content.as_deref(), &msg_prop, event) {
                    // TODO: Implement ChatAgent for direct replies
                    info!("[BrainAgent] should reply but ChatAgent not yet implemented");
                    // let chat_agent = ChatAgent::new(self.llm.clone(), bot_adapter.get_message_store(), self.persona.clone());
                    // match chat_agent.on_agent_input(bot_adapter, event, vec![UserMessage(user_text_for_reply.clone())]) {
                    //     Ok(reply) => {
                    //         if reply.trim().is_empty() {
                    //             info!("[BrainAgent] chat agent reply is empty");
                    //         } else {
                    //             info!("[BrainAgent] chat agent reply: {}", reply);
                    //         }
                    //     }
                    //     Err(e) => {
                    //         info!("[BrainAgent] chat agent failed: {}", e);
                    //     }
                    // }
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

    fn on_agent_input(&self, _adapter: &mut BotAdapter, _event: &MessageEvent, _messages: Vec<Message>) -> Self::Output {
        Ok(())
    }
    
    fn name(&self) -> &'static str {
        "BrainAgent"
    }
}

impl crate::bot_adapter::adapter::BrainAgentTrait for BrainAgent {
    fn on_event(&self, bot_adapter: &mut BotAdapter, event: &MessageEvent) -> Result<()> {
        Agent::on_event(self, bot_adapter, event)
    }

    fn name(&self) -> &'static str {
        "BrainAgent"
    }

    fn clone_box(&self) -> crate::bot_adapter::adapter::AgentBox {
        Box::new(self.clone())
    }
}