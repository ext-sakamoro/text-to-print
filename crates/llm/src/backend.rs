use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::model::ModelChoice;

/// LLM 推論設定
///
/// `endpoint` は OpenAI 互換の chat/completions URL。デフォルトは
/// `alice-llm-server` sidecar (`http://localhost:8000/v1/chat/completions`)
/// を指す。sidecar プロセスは [`crate::sidecar::SidecarProcess::spawn`]
/// で起動する
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub endpoint: String,
    /// model 選択 (Qwen 3.5-4B Q4_K_M / Bonsai 27B Q1_0)。
    /// `model_id()` を通じて HTTP request の `model` field に載せる
    pub model_choice: ModelChoice,
    pub max_tokens: u32,
    pub temperature: f32,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:8000/v1/chat/completions".to_string(),
            model_choice: ModelChoice::default(),
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

pub async fn generate(
    config: &LlmConfig,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String> {
    let model_id = config.model_choice.model_id();
    info!(model = %model_id, endpoint = %config.endpoint, "LLM inference");

    let client = reqwest::Client::new();
    let request = ChatRequest {
        model: model_id.to_string(),
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

    let response = client.post(&config.endpoint).json(&request).send().await?;

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
    fn default_config_targets_sidecar() {
        let config = LlmConfig::default();
        assert!(config.endpoint.contains(":8000"));
        assert!(config.endpoint.contains("chat/completions"));
    }

    #[test]
    fn default_config_uses_qwen_choice() {
        let config = LlmConfig::default();
        assert_eq!(config.model_choice, ModelChoice::Qwen35_4B);
    }

    #[test]
    fn config_serialization_roundtrip() {
        let config = LlmConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: LlmConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config.endpoint, deserialized.endpoint);
        assert_eq!(config.model_choice, deserialized.model_choice);
        assert_eq!(config.max_tokens, deserialized.max_tokens);
    }

    #[test]
    fn config_temperature_range() {
        let config = LlmConfig::default();
        assert!(config.temperature > 0.0);
        assert!(config.temperature <= 2.0);
    }
}
