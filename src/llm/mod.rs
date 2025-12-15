pub mod agent;
pub mod api;

pub trait LLMBase {
    fn get_model_name(&self) -> &str;
    fn inference(&self, messages: Vec<api::Message>) -> String;
}

pub use api::LLMAPI;

