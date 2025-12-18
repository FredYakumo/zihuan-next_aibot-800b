use crate::bot_adapter::models::MessageEvent;
use crate::llm::{LLMBase, InferenceParam, Message, MessageRole};
use crate::llm::agent::Agent;
use crate::llm::function_tools::{
    FunctionTool, ChatHistoryTool, NaturalLanguageReplyTool, CodeWriterTool, MathTool
};

use log::{debug, warn};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrainPlan {
	/// Do nothing.
	Ignore { reason: Option<String> },
	/// Reply with content.
	Reply { content: String },
	/// Delegate to a specific agent.
	UseAgent { agent_name: String, context: Value },
}

#[derive(Debug, Clone, PartialEq)]
pub enum BrainOutcome {
	Ignored { reason: Option<String> },
	ReplyText { content: String },
	/// Agent executed (and optionally a follow-up reply was generated).
	AgentExecuted {
		agent_name: String,
		context: Value,
		agent_output: Value,
		final_reply: Option<String>,
	},
	Error { message: String, raw: String },
}

/// Returns the default set of built-in tools for the BrainAgent
pub fn brain_agent_list(llm: Arc<dyn LLMBase + Send + Sync>) -> Vec<Arc<dyn FunctionTool>> {
    vec![
        Arc::new(ChatHistoryTool::new()),
        Arc::new(NaturalLanguageReplyTool::new(llm.clone())),
        Arc::new(CodeWriterTool::new(llm)),
        Arc::new(MathTool::new()),
    ]
}

/// Brain agent: receives events and makes a decision.
///
/// Note:
/// - We use `LLMBase::inference()` for both planning and (optionally) a second stage
///   to turn tool results into a user-facing reply.
/// - "Function tools" are implemented via a strict JSON protocol in the prompt.
pub struct BrainAgent {
	llm: Arc<dyn LLMBase + Send + Sync>,
	tools: Vec<Arc<dyn FunctionTool>>,
	system_prompt: String,
	max_tool_rounds: usize,
}

impl BrainAgent {
	pub fn new(llm: Arc<dyn LLMBase + Send + Sync>) -> Self {
		let tools = brain_agent_list(llm.clone());
		Self {
			llm: llm.clone(),
			tools,
			system_prompt: default_system_prompt(),
			max_tool_rounds: 1,
		}
	}

	/// Override the system prompt.
	pub fn with_system_prompt(mut self, system_prompt: impl Into<String>) -> Self {
		self.system_prompt = system_prompt.into();
		self
	}

	/// Control maximum tool execution rounds.
	///
	/// - 0: no tool execution, only planning
	/// - 1: one tool call max (default)
	pub fn with_max_tool_rounds(mut self, n: usize) -> Self {
		self.max_tool_rounds = n;
		self
	}

	pub fn register_tool(&mut self, tool: Arc<dyn FunctionTool>) {
		self.tools.push(tool);
	}

	pub fn list_tools(&self) -> Vec<String> {
		self.tools.iter().map(|t| t.name().to_string()).collect()
	}

	/// Make a plan from an incoming event.
	pub fn plan(&self, event: &MessageEvent) -> Result<BrainPlan, String> {
		let event_summary = format_event(event);

		let prompt = format!(
			"You will receive a chat event. Decide what to do next.\n\n\
Event:\n{event_summary}\n\n\
If you need to delegate to a specific agent, use tool-calling with agent details.\n\
If no agent is needed, return STRICT JSON only (no markdown, no extra text).\n\
It MUST match one of:\n\
1) {{\"type\":\"ignore\",\"reason\":\"...optional...\"}}\n\
2) {{\"type\":\"reply\",\"content\":\"...\"}}\n",
		);

		let messages = vec![
			Message {
				role: MessageRole::System,
				content: Some(self.system_prompt.clone()),
				tool_calls: Vec::new(),
			},
			Message {
				role: MessageRole::User,
				content: Some(prompt),
				tool_calls: Vec::new(),
			},
		];

		let tools: Option<Vec<Arc<dyn FunctionTool>>> = if !self.tools.is_empty() {
			Some(self.tools.clone())
		} else {
			None
		};

		let param = InferenceParam {
			messages,
			tools,
		};

		let resp = self
			.llm
			.inference(&param);

		// Prefer tool calls when available (interpret as agent delegation).
		if let Some(tc) = resp.tool_calls.first() {
			return Ok(BrainPlan::UseAgent {
				agent_name: tc.function.name.clone(),
				context: tc.function.arguments.clone(),
			});
		}

		let raw = resp.content.unwrap_or_default();
		debug!("[BrainAgent] raw plan: {}", raw);
		parse_json_lenient::<BrainPlan>(&raw)
			.map_err(|e| format!("failed to parse BrainPlan: {e}; raw={raw}"))
	}

	/// Run the agent end-to-end: plan -> (optional tool) -> (optional final reply)
	pub fn run(&self, event: &MessageEvent) -> BrainOutcome {
		let plan = match self.plan(event) {
			Ok(p) => p,
			Err(e) => {
				warn!("[BrainAgent] planning error: {}", e);
				return BrainOutcome::Error {
					message: e,
					raw: "".to_string(),
				};
			}
		};

		match plan {
			BrainPlan::Ignore { reason } => BrainOutcome::Ignored { reason },
			BrainPlan::Reply { content } => BrainOutcome::ReplyText { content },
			BrainPlan::UseAgent { agent_name, context } => {
				if self.max_tool_rounds == 0 {
					return BrainOutcome::Error {
						message: "agent execution disabled (max_tool_rounds=0)".to_string(),
						raw: serde_json::to_string(&BrainPlan::UseAgent { agent_name, context })
							.unwrap_or_default(),
					};
				}

			let tool = self
				.tools
				.iter()
				.find(|t| t.name() == agent_name)
				.cloned();

			let tool = match tool {
				Some(t) => t,
				None => {
					return BrainOutcome::Error {
						message: format!("unknown agent: {agent_name}"),
						raw: "".to_string(),
					};
				}
			};

			let agent_context = context;
			let agent_output = match tool.call(agent_context.clone()) {
				Ok(v) => v,
				Err(e) => {
					return BrainOutcome::Error {
						message: format!("agent {agent_name} failed: {e}"),
						raw: "".to_string(),
					}
				},
			};

			// Second-stage: ask LLM to produce a user-facing reply given agent output.
			let event_summary = format_event(event);
			let prompt = format!(
				"You delegated to an agent for a chat event. Produce the final user reply.\n\n\
Event:\n{event_summary}\n\n\
Agent name: {agent_name}\n\
Agent context (JSON): {}\n\
Agent output (JSON): {}\n\n\
Return STRICT JSON only: {{\"type\":\"reply\",\"content\":\"...\"}}\n",
				serde_json::to_string_pretty(&agent_context).unwrap_or_default(),
				serde_json::to_string_pretty(&agent_output).unwrap_or_default(),
			);

			let messages = vec![
				Message {
					role: MessageRole::System,
					content: Some(self.system_prompt.clone()),
					tool_calls: Vec::new(),
				},
				Message {
					role: MessageRole::User,
					content: Some(prompt),
					tool_calls: Vec::new(),
				},
			];

			let param = InferenceParam {
				messages,
				tools: None,
			};

			let raw = self
				.llm
				.inference(&param)
				.content
				.unwrap_or_default();
				debug!("[BrainAgent] raw final: {}", raw);
				let final_reply = match parse_json_lenient::<BrainPlan>(&raw) {
					Ok(BrainPlan::Reply { content }) => Some(content),
					Ok(other) => {
						warn!("[BrainAgent] unexpected final plan: {:?}", other);
						None
					}
					Err(e) => {
						warn!("[BrainAgent] failed to parse final reply: {}", e);
						None
					}
				};

				BrainOutcome::AgentExecuted {
					agent_name,
					context: agent_context,
					agent_output,
					final_reply,
				}
			}
		}
	}
}

impl Agent for BrainAgent {
	type Output = BrainOutcome;

	fn name(&self) -> &'static str {
		"brain"
	}

	fn on_event(&self, event: &MessageEvent) -> Self::Output {
		self.run(event)
	}
}

fn default_system_prompt() -> String {
	// Tools are provided out-of-band via the LLM API (when supported), not via prompts.
	// Keep the system prompt empty by default to avoid coupling behavior to system prompts.
	"".to_string()
}

fn format_event(event: &MessageEvent) -> String {
	let messages: Vec<String> = event.message_list.iter().map(|m| m.to_string()).collect();
	format!(
		"message_id: {}\nmessage_type: {}\nsender_user_id: {}\nsender_nickname: {}\nmessage: {:?}",
		event.message_id,
		event.message_type,
		event.sender.user_id,
		event.sender.nickname,
		messages
	)
}

fn parse_json_lenient<T: for<'de> Deserialize<'de>>(raw: &str) -> Result<T, String> {
	// 1) Direct parse.
	if let Ok(v) = serde_json::from_str::<T>(raw) {
		return Ok(v);
	}

	// 2) Try to extract the first JSON object/array substring.
	let trimmed = raw.trim();
	let (start_idx, open_char) = trimmed
		.char_indices()
		.find(|(_, c)| *c == '{' || *c == '[')
		.ok_or_else(|| "no JSON object/array start found".to_string())?;
	let close_char = if open_char == '{' { '}' } else { ']' };
	let end_idx = trimmed
		.char_indices()
		.rfind(|(_, c)| *c == close_char)
		.map(|(i, _)| i)
		.ok_or_else(|| "no JSON object/array end found".to_string())?;

	if end_idx <= start_idx {
		return Err("invalid JSON bounds".to_string());
	}

	let candidate = &trimmed[start_idx..=end_idx];
	serde_json::from_str::<T>(candidate)
		.map_err(|e| format!("JSON parse failed: {e}; candidate={candidate}"))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_parse_json_lenient_direct() {
		let raw = r#"{"type":"reply","content":"hi"}"#;
		let plan: BrainPlan = parse_json_lenient(raw).unwrap();
		assert_eq!(plan, BrainPlan::Reply { content: "hi".to_string() });
	}

	#[test]
	fn test_parse_json_lenient_embedded() {
		let raw = "some text... {\"type\":\"ignore\",\"reason\":\"no trigger\"} ...tail";
		let plan: BrainPlan = parse_json_lenient(raw).unwrap();
		assert_eq!(
			plan,
			BrainPlan::Ignore {
				reason: Some("no trigger".to_string())
			}
		);
	}
}
