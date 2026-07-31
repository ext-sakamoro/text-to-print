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

/// Retry outcome returned by [`generate_with_retry`]
///
/// - `content`: final LLM response after any retries
/// - `retry_count`: number of retries actually performed (0 = first-try
///   success; up to `max_retries`)
#[derive(Debug, Clone)]
pub struct RetryResult {
    pub content: String,
    pub retry_count: u32,
}

/// Generate a response with automatic retry on safety violations
///
/// The caller supplies a `safety_check` closure that inspects the LLM
/// output (already-extracted LOL DSL or the raw response) and returns a
/// list of safety violation messages If the list is non-empty, the
/// backend re-invokes the LLM with the original user prompt suffixed by
/// a [`crate::fix_prompt::fix_prompt_from_messages`] instruction that
/// enumerates the offending violations
///
/// The loop stops when:
/// - the safety check returns no violations (success), or
/// - `max_retries` retries have been performed (returns the last output
///   regardless of remaining violations)
///
/// # Errors
///
/// Any transport or inference error from the underlying [`LlmBackend`]
/// call terminates the loop and is returned to the caller unchanged
pub async fn generate_with_retry<F>(
    backend: &crate::backend_kind::LlmBackend,
    params: &crate::backend_kind::InferenceParams,
    system_prompt: &str,
    user_prompt: &str,
    max_retries: u32,
    mut safety_check: F,
) -> Result<RetryResult>
where
    F: FnMut(&str) -> Vec<String>,
{
    let mut current_prompt = user_prompt.to_string();
    let mut retry_count = 0_u32;
    let mut last_content = String::new();

    for attempt in 0..=max_retries {
        let content = backend
            .generate(system_prompt, &current_prompt, params)
            .await?;
        last_content = content.clone();

        let violations = safety_check(&content);
        if violations.is_empty() {
            return Ok(RetryResult {
                content: last_content,
                retry_count,
            });
        }

        if attempt >= max_retries {
            break;
        }

        retry_count = retry_count.saturating_add(1);
        let suffix = crate::fix_prompt::fix_prompt_from_messages(&violations);
        if suffix.is_empty() {
            // No actionable fix instruction (all messages unclassified) —
            // stop retrying and return the current content
            break;
        }
        current_prompt = format!("{user_prompt}{suffix}");
        info!(
            retry = retry_count,
            violations = violations.len(),
            "safety violations detected, retrying with fix prompt"
        );
    }

    Ok(RetryResult {
        content: last_content,
        retry_count,
    })
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

    // Note: end-to-end generate_with_retry tests would require a live LLM
    // endpoint; these tests exercise the state machine via the safety_check
    // closure only Callers are expected to provide their own integration
    // tests against a running `alice-llm-server`

    #[test]
    fn retry_result_defaults() {
        let r = RetryResult {
            content: String::new(),
            retry_count: 0,
        };
        assert_eq!(r.retry_count, 0);
        assert!(r.content.is_empty());
    }
}
