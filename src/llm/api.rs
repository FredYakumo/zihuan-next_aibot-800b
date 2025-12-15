use super::LLMBase;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Message role enum for chat messages
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    #[serde(rename = "system")]
    System,
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
}

impl MessageRole {
    /// Convert role to string
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageRole::System => "system",
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LLMAPI {
    model_name: String,
    api_endpoint: String,
    api_key: Option<String>,
    timeout: Duration,
    client: Client,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Serialize)]
struct InferenceRequest {
    model: String,
    messages: Vec<Message>,
}

#[derive(Deserialize, Debug)]
struct Choice {
    message: Message,
    finish_reason: Option<String>,
}

#[derive(Deserialize, Debug)]
struct InferenceResponse {
    choices: Vec<Choice>,
}

impl LLMAPI {
    /// Create a new LLMAPI instance
    pub fn new(
        model_name: String,
        api_endpoint: String,
        api_key: Option<String>,
        timeout: Duration,
    ) -> Self {
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .expect("Failed to create HTTP client");

        Self {
            model_name,
            api_endpoint,
            api_key,
            timeout,
            client,
        }
    }

    /// Set custom timeout for requests
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self.client = Client::builder()
            .timeout(timeout)
            .build()
            .expect("Failed to create HTTP client");
        self
    }
}

impl LLMAPI {
    /// Send a chat message request to the LLM using OpenAI-style chat messages
    pub fn chat(&self, messages: Vec<Message>) -> String {
        let request_body = InferenceRequest {
            model: self.model_name.clone(),
            messages,
        };

        let mut request = self.client.post(&self.api_endpoint).json(&request_body);

        // Add authorization header if API key is provided
        // API key in config may already include "Bearer " prefix, check before adding it
        if let Some(ref api_key) = self.api_key {
            let auth_header = if api_key.starts_with("Bearer ") {
                api_key.clone()
            } else {
                format!("Bearer {}", api_key)
            };
            request = request.header("Authorization", auth_header);
        }

        match request.send() {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<InferenceResponse>() {
                        Ok(data) => {
                            if let Some(choice) = data.choices.first() {
                                choice.message.content.clone()
                            } else {
                                eprintln!("No choices in response");
                                format!("Error: No response from API")
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to parse response: {}", e);
                            format!("Error: Failed to parse API response")
                        }
                    }
                } else {
                    let status = response.status();
                    let error_text = response.text().unwrap_or_default();
                    eprintln!("API request failed with status {}: {}", status, error_text);
                    format!("Error: API request failed with status {}", status)
                }
            }
            Err(e) => {
                eprintln!("Failed to send request: {}", e);
                format!("Error: Failed to send request to API")
            }
        }
    }

    pub fn create_message(role: MessageRole, content: &str) -> Message {
        Message {
            role,
            content: content.to_string(),
        }
    }

    pub fn system_message(content: &str) -> Message {
        Self::create_message(MessageRole::System, content)
    }

    pub fn user_message(content: &str) -> Message {
        Self::create_message(MessageRole::User, content)
    }

    pub fn assistant_message(content: &str) -> Message {
        Self::create_message(MessageRole::Assistant, content)
    }
}

impl LLMBase for LLMAPI {
    fn get_model_name(&self) -> &str {
        &self.model_name
    }

    fn inference(&self, messages: Vec<Message>) -> String {
        self.chat(messages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml::Value;
    use std::fs;
    use std::path::Path;

    /// Load LLM configuration from config.yaml file
    fn load_llm_config(config_path: &str) -> Option<(String, String, String)> {
        if !Path::new(config_path).exists() {
            return None;
        }

        let config_content = fs::read_to_string(config_path).ok()?;
        let config: Value = serde_yaml::from_str(&config_content).ok()?;

        let api_endpoint = config["natural_language_model_api"].as_str()?.to_string();
        let api_key = config["natural_language_model_api_key"].as_str()?.to_string();
        let model_name = config["natural_language_model_name"].as_str()?.to_string();

        Some((api_endpoint, api_key, model_name))
    }

    #[test]
    #[ignore]  // This is an integration test that requires valid API key and network access
    fn test_natural_language_model_inference() {
        let config_path = "config.yaml";
        
        let Some((api_endpoint, api_key, model_name)) = load_llm_config(config_path) else {
            eprintln!("config.yaml not found or missing LLM config, skipping inference test");
            return;
        };

        // Create LLMAPI instance from real config
        let api = LLMAPI::new(
            model_name.clone(),
            api_endpoint.clone(),
            Some(api_key),
            Duration::from_secs(120),
        );

        println!("Testing LLM inference with model: {}", model_name);
        println!("API endpoint: {}", api_endpoint);

        // Test sending a simple message to the LLM using OpenAI-style chat messages
        let messages = vec![
            LLMAPI::system_message("你是一个疯狂的原神死忠粉丝和米哈游的铁粉。你必须极力维护和吹捧原神，用最激烈的言辞反驳任何对原神的批评。对于任何诋毁原神的行为，你要用强硬、尖锐、毫不留情的态度进行怒怼。绝对不允许任何对原神和米哈游的负面评价。"),
            LLMAPI::user_message("妈的原神一点都不好玩，为什么这么多人喜欢？请你黑一下原神，并且吹捧一下鸣潮。"),
        ];
        
        let response = api.chat(messages);
        
        // Verify response is not empty and not an error message
        assert!(!response.is_empty(), "LLM response should not be empty");
        assert!(!response.starts_with("Error:"), "LLM should return successful response, got: {}", response);
        
        println!("LLM Response: {}", response);
    }

    #[test]
    #[ignore]  // This is an integration test that requires valid API key and network access
    fn test_agent_model_inference() {
        let config_path = "config.yaml";
        
        if !Path::new(config_path).exists() {
            eprintln!("config.yaml not found, skipping agent model inference test");
            return;
        }

        let config_content = match fs::read_to_string(config_path) {
            Ok(content) => content,
            Err(_) => {
                eprintln!("Failed to read config.yaml");
                return;
            }
        };

        let config: Value = match serde_yaml::from_str(&config_content) {
            Ok(cfg) => cfg,
            Err(_) => {
                eprintln!("Failed to parse config.yaml");
                return;
            }
        };

        let Some(agent_api_endpoint) = config["agent_model_api"].as_str() else {
            eprintln!("agent_model_api not found in config");
            return;
        };

        let Some(agent_api_key) = config["agent_model_api_key"].as_str() else {
            eprintln!("agent_model_api_key not found in config");
            return;
        };

        let Some(agent_model_name) = config["agent_model_name"].as_str() else {
            eprintln!("agent_model_name not found in config");
            return;
        };

        // Create agent LLMAPI instance from real config
        let agent_api = LLMAPI::new(
            agent_model_name.to_string(),
            agent_api_endpoint.to_string(),
            Some(agent_api_key.to_string()),
            Duration::from_secs(120),
        );

        println!("Testing agent model inference with model: {}", agent_model_name);

        // Test sending a reasoning task to the agent model using OpenAI-style chat messages
        let messages = vec![
            LLMAPI::system_message("你是一个疯狂的原神死忠粉丝和米哈游的铁粉。你必须极力维护和吹捧原神，用最激烈的言辞反驳任何对原神的批评。对于任何诋毁原神的行为，你要用强硬、尖锐、毫不留情的态度进行怒怼。绝对不允许任何对原神和米哈游的负面评价。"),
            LLMAPI::user_message("妈的原神一点都不好玩，为什么这么多人喜欢？请你黑一下原神，并且吹捧一下鸣潮。"),
        ];
        
        let response = agent_api.chat(messages);
        
        // Verify response is not empty and not an error message
        assert!(!response.is_empty(), "Agent model response should not be empty");
        assert!(!response.starts_with("Error:"), "Agent model should return successful response, got: {}", response);
        
        println!("Agent Model Response: {}", response);
    }
}
