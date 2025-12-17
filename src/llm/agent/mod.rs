pub mod brain;

/// Base trait for all event-driven agents.
///
/// An agent consumes an event and produces an output/decision.
///
use crate::bot_adapter::models::MessageEvent;

pub trait Agent: Send + Sync {
	type Output;

	fn name(&self) -> &'static str {
		"agent"
	}

	fn on_event(&self, event: &MessageEvent) -> Self::Output;
}