use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub endpoint: String,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:11434/v1/chat/completions".to_string(),
            model: "qwen2.5:7b".to_string(),
            max_tokens: 2048,
            temperature: 0.7,
        }
    }
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: String,
}

pub async fn generate(config: &LlmConfig, system_prompt: &str, user_prompt: &str) -> Result<String> {
    info!(model = %config.model, "LLM inference");

    let client = reqwest::Client::new();
    let request = ChatRequest {
        model: config.model.clone(),
        messages: vec![
            Message {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            Message {
                role: "user".to_string(),
                content: user_prompt.to_string(),
            },
        ],
        max_tokens: config.max_tokens,
        temperature: config.temperature,
    };

    let response = client
        .post(&config.endpoint)
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!("LLM request failed: {status} {body}");
    }

    let chat_response: ChatResponse = response.json().await?;
    let content = chat_response
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .unwrap_or_default();

    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_ollama_endpoint() {
        let config = LlmConfig::default();
        assert!(config.endpoint.contains("11434"));
        assert!(config.endpoint.contains("chat/completions"));
    }

    #[test]
    fn default_config_uses_qwen() {
        let config = LlmConfig::default();
        assert!(config.model.contains("qwen"));
    }

    #[test]
    fn config_serialization_roundtrip() {
        let config = LlmConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: LlmConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config.endpoint, deserialized.endpoint);
        assert_eq!(config.model, deserialized.model);
        assert_eq!(config.max_tokens, deserialized.max_tokens);
    }

    #[test]
    fn config_temperature_range() {
        let config = LlmConfig::default();
        assert!(config.temperature > 0.0);
        assert!(config.temperature <= 2.0);
    }
}
