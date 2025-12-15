use super::LLMBase;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct LLMAPI {
    model_name: String,
    api_endpoint: String,
    api_key: Option<String>,
    timeout: Duration,
    client: Client,
}

#[derive(Serialize, Deserialize, Debug)]
struct Message {
    role: String,
    content: String,
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

impl LLMBase for LLMAPI {
    fn get_model_name(&self) -> &str {
        &self.model_name
    }

    fn inference(&self, prompt: &str, system_prompt: &str) -> String {
        let mut messages = Vec::new();
        
        // Add system message if provided
        if !system_prompt.is_empty() {
            messages.push(Message {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            });
        }
        
        // Add user message
        messages.push(Message {
            role: "user".to_string(),
            content: prompt.to_string(),
        });

        let request_body = InferenceRequest {
            model: self.model_name.clone(),
            messages,
        };

        let mut request = self.client.post(&self.api_endpoint).json(&request_body);

        // Add authorization header if API key is provided
        if let Some(ref api_key) = self.api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llmapi_creation() {
        let api = LLMAPI::new(
            "gpt-5.2".to_string(),
            "https://api.openai.com/v1/chat/completions".to_string(),
            Some("sk-114514191981".to_string()),
            Duration::from_secs(60),
        );

        assert_eq!(api.get_model_name(), "gpt-5.2");
    }

    #[test]
    fn test_llmapi_with_timeout() {
        let api = LLMAPI::new(
            "gpt-5.2".to_string(),
            "https://api.openai.com/v1/chat/completions".to_string(),
            None,
            Duration::from_secs(60),
        )
        .with_timeout(Duration::from_secs(30));

        assert_eq!(api.timeout, Duration::from_secs(30));
    }
}
