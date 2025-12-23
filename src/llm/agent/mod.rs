pub mod brain;

/// Base trait for all event-driven agents.
///
/// An agent consumes an event and produces an output/decision.
///
use crate::{bot_adapter::{adapter::BotAdapter, models::MessageEvent}, llm::Message};

pub trait Agent: Send + Sync {
	type Output;

	fn name(&self) -> &'static str {
		"agent"
	}

	fn on_event(&self, bot_adapter: &mut BotAdapter, event: &MessageEvent) -> Self::Output;

	/// Invoke this agent using structured input (for agent-to-agent calls).
	/// Default implementation falls back to panic to surface unimplemented usage.
	fn on_agent_input(&self, input: Message) -> Self::Output;
}

pub mod chat_history_agent;
pub mod math_agent;
pub mod nl_reply_agent;
pub mod code_writer_agent;