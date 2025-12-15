pub mod agent;
pub mod api;

pub trait LLMBase {
    fn get_model_name(&self) -> &str;
    fn inference(&self, prompt: &str, system_prompt: &str) -> String;
}

pub use api::LLMAPI;

