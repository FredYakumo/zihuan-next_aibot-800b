pub mod brain;

use std::sync::Arc;

/// Base trait for all event-driven agents.
///
/// An agent consumes an event and produces an output/decision.
///
use crate::{bot_adapter::{adapter::BotAdapter, models::MessageEvent}, llm::Message};

pub trait Agent: Send + Sync {
	type Output;

	fn name(&self) -> &'static str;

	fn on_event(&self, bot_adapter: &mut BotAdapter, event: &MessageEvent) -> Self::Output;

	/// Invoke this agent using structured input (for agent-to-agent calls).
	/// Default implementation falls back to panic to surface unimplemented usage.
	fn on_agent_input(&self, input: Message) -> Self::Output;
}

pub trait FunctionToolsAgent: Send + Sync {
    fn get_tools(&self) -> Vec<&dyn crate::llm::function_tools::FunctionTool>;
}

pub mod chat;
pub mod code_gen;