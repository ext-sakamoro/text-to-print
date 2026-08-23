//! OpenAI-compatible remote LLM backend (BYO LLM, 2026-08-23)
//!
//! Sends chat-completion requests to any OpenAI-compatible HTTP endpoint
//! Preset providers: OpenAI (GPT-5 / o-series), Anthropic (Claude via
//! OpenAI-compat endpoint), Google (Gemini via OpenAI-compat endpoint),
//! and Custom (Ollama :11434/v1, LM Studio :1234/v1, other self-hosted)
//!
//! Unlike [`crate::sidecar::SidecarProcess`], this backend does NOT
//! spawn a subprocess The API key is supplied at construction time
//! (fetched from OS Keychain by the caller) and held in memory only —
//! [`OpenAiCompatConfig`] intentionally does not derive `Serialize` /
//! `Deserialize`, so an accidental persist path cannot leak the key
//!
//! Cost-guard defaults (see `[[llm-api-cost-guard]]` skill):
//! - OpenAI o-series / GPT-5: `reasoning_effort = "minimal"`
//! - Google Gemini 2.5 Pro/Flash: `reasoning_effort = "none"`
//! - Anthropic Claude: `reasoning_effort = None` (compat endpoint does
//!   not expose extended thinking; default is off)
//! - Custom: `reasoning_effort = None` (unknown backend; user decides)

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{info, warn};

use crate::backend_kind::InferenceParams;

/// Which OpenAI-compat provider preset this backend targets Determines
/// default endpoint / model / cost-guard `reasoning_effort` value
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpenAiCompatProvider {
    OpenAi,
    Anthropic,
    Google,
    Custom,
}

impl OpenAiCompatProvider {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::OpenAi => "OpenAI (GPT-5 / o-series)",
            Self::Anthropic => "Anthropic (Claude)",
            Self::Google => "Google (Gemini)",
            Self::Custom => "OpenAI-compat (Ollama / LM Studio / other)",
        }
    }

    #[must_use]
    pub const fn default_endpoint(self) -> &'static str {
        match self {
            Self::OpenAi => "https://api.openai.com/v1/chat/completions",
            Self::Anthropic => "https://api.anthropic.com/v1/chat/completions",
            Self::Google => {
                "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
            }
            Self::Custom => "http://localhost:11434/v1/chat/completions",
        }
    }

    /// Provider preset の default model 選定基準 (2026-08-23):
    /// - OpenAI: 最新 flagship `gpt-5` (課金 model、reasoning_effort=minimal で
    ///   silent thinking 抑制)
    /// - Anthropic: 主力 `claude-sonnet-4-5` (課金、Opus と Haiku の中間)
    /// - Google: **`gemini-2.5-flash`** — 無料枠 (Google AI Studio、~1500 req/day
    ///   目安) が最も寛容な API path β 公開時の 「無料で試せる BYO LLM」推奨
    ///   Pro 版 (`gemini-2.5-pro`) を使う user は Model 欄で自由に変更可
    /// - Custom (Ollama / LM Studio): 一般的な local 14B GGUF slug
    #[must_use]
    pub const fn default_model(self) -> &'static str {
        match self {
            Self::OpenAi => "gpt-5",
            Self::Anthropic => "claude-sonnet-4-5",
            Self::Google => "gemini-2.5-flash",
            Self::Custom => "qwen2.5-14b-instruct",
        }
    }

    /// Cost-guard default that suppresses silent thinking-mode token burn
    /// Value forwarded verbatim in the request body — provider-specific:
    /// - OpenAI o-series / GPT-5: `"minimal"`
    /// - Google Gemini 2.5: `"none"` (Gemini-specific OpenAI-compat ext)
    /// - Anthropic / Custom: `None` (compat endpoint does not surface it)
    #[must_use]
    pub const fn default_reasoning_effort(self) -> Option<&'static str> {
        match self {
            Self::OpenAi => Some("minimal"),
            Self::Google => Some("none"),
            Self::Anthropic | Self::Custom => None,
        }
    }

    /// Stable DB key round-trips via [`Self::from_db_str`]
    #[must_use]
    pub const fn to_db_str(self) -> &'static str {
        match self {
            Self::OpenAi => "OpenAi",
            Self::Anthropic => "Anthropic",
            Self::Google => "Google",
            Self::Custom => "Custom",
        }
    }

    #[must_use]
    pub fn from_db_str(s: &str) -> Self {
        match s {
            "Anthropic" => Self::Anthropic,
            "Google" => Self::Google,
            "Custom" => Self::Custom,
            _ => Self::OpenAi,
        }
    }

    /// Stable identifier used as the Keychain `account` field so each
    /// provider stores its API key under a separate slot
    #[must_use]
    pub const fn keychain_account(self) -> &'static str {
        match self {
            Self::OpenAi => "openai_api_key",
            Self::Anthropic => "anthropic_api_key",
            Self::Google => "google_api_key",
            Self::Custom => "openai_compat_custom_api_key",
        }
    }
}

/// Runtime config for an OpenAI-compat remote LLM `api_key` is loaded
/// from OS Keychain at construction time and never serialized `Clone`
/// is intentional so the async generation path can hold its own copy
///
/// Intentionally does **not** derive `Serialize` / `Deserialize` — the
/// `api_key` field must never be persisted to disk via serde
#[derive(Debug, Clone)]
pub struct OpenAiCompatConfig {
    pub provider: OpenAiCompatProvider,
    pub endpoint: String,
    /// Held in memory only Never serialized to disk
    pub api_key: String,
    pub model: String,
    /// Cost-guard: `"minimal"` (OpenAI) / `"none"` (Gemini) / `None`
    /// (Anthropic / Custom) Forwarded verbatim in request body
    pub reasoning_effort: Option<String>,
}

impl OpenAiCompatConfig {
    /// Construct a config from a provider preset with default endpoint
    /// / model / reasoning_effort Caller supplies the `api_key` (typically
    /// from OS Keychain lookup)
    #[must_use]
    pub fn from_preset(provider: OpenAiCompatProvider, api_key: String) -> Self {
        Self {
            provider,
            endpoint: provider.default_endpoint().to_string(),
            api_key,
            model: provider.default_model().to_string(),
            reasoning_effort: provider.default_reasoning_effort().map(str::to_string),
        }
    }
}

/// Type-erased backend variant held inside [`crate::backend_kind::LlmBackend`]
#[derive(Debug, Clone)]
pub struct OpenAiCompatBackend {
    pub config: OpenAiCompatConfig,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    max_tokens: u32,
    temperature: f32,
    /// Provider-specific cost-guard field Omitted from the JSON body
    /// when `None` so vanilla OpenAI (non-reasoning models) is unaffected
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
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

impl OpenAiCompatBackend {
    #[must_use]
    pub const fn new(config: OpenAiCompatConfig) -> Self {
        Self { config }
    }

    /// One-shot chat generation Mirrors the shape of
    /// [`crate::backend::generate_with_grammar`] so that
    /// [`crate::backend::generate_with_retry`] can dispatch through
    /// [`crate::backend_kind::LlmBackend`] uniformly
    ///
    /// # Errors
    ///
    /// Transport failures, non-2xx HTTP status, or JSON parse errors
    /// bubble up unchanged
    pub async fn generate(
        &self,
        system: &str,
        user: &str,
        params: &InferenceParams,
    ) -> Result<String> {
        info!(
            provider = ?self.config.provider,
            endpoint = %self.config.endpoint,
            model = %self.config.model,
            max_tokens = params.max_tokens,
            temperature = params.temperature,
            reasoning_effort = ?self.config.reasoning_effort,
            system_prompt_len = system.len(),
            user_prompt_len = user.len(),
            "OpenAiCompat inference start"
        );

        // Reuse the 300s timeout from sidecar backend Remote APIs finish
        // in <30s typical, but leave slack for network / rate-limit retry
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()?;

        let request = ChatRequest {
            model: self.config.model.clone(),
            messages: vec![
                Message {
                    role: "system".to_string(),
                    content: system.to_string(),
                },
                Message {
                    role: "user".to_string(),
                    content: user.to_string(),
                },
            ],
            max_tokens: params.max_tokens,
            temperature: params.temperature,
            reasoning_effort: self.config.reasoning_effort.clone(),
        };

        let started = std::time::Instant::now();
        let response = client
            .post(&self.config.endpoint)
            .bearer_auth(&self.config.api_key)
            .json(&request)
            .send()
            .await?;
        let elapsed_ms = started.elapsed().as_millis();
        info!(
            elapsed_ms,
            status = %response.status(),
            "OpenAiCompat inference response received"
        );

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!("OpenAiCompat request failed: {status} {body}");
        }

        let chat_response: ChatResponse = response.json().await?;
        let content = chat_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();

        if content.trim().is_empty() {
            warn!("OpenAiCompat returned empty content");
        }

        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_labels_non_empty() {
        for p in [
            OpenAiCompatProvider::OpenAi,
            OpenAiCompatProvider::Anthropic,
            OpenAiCompatProvider::Google,
            OpenAiCompatProvider::Custom,
        ] {
            assert!(!p.label().is_empty());
            assert!(!p.default_endpoint().is_empty());
            assert!(!p.default_model().is_empty());
            assert!(!p.keychain_account().is_empty());
        }
    }

    #[test]
    fn provider_endpoints_are_chat_completions() {
        for p in [
            OpenAiCompatProvider::OpenAi,
            OpenAiCompatProvider::Anthropic,
            OpenAiCompatProvider::Google,
            OpenAiCompatProvider::Custom,
        ] {
            assert!(
                p.default_endpoint().ends_with("/chat/completions"),
                "provider {p:?} endpoint should end with /chat/completions"
            );
        }
    }

    #[test]
    fn provider_db_roundtrip() {
        for p in [
            OpenAiCompatProvider::OpenAi,
            OpenAiCompatProvider::Anthropic,
            OpenAiCompatProvider::Google,
            OpenAiCompatProvider::Custom,
        ] {
            assert_eq!(OpenAiCompatProvider::from_db_str(p.to_db_str()), p);
        }
    }

    #[test]
    fn provider_from_db_str_unknown_falls_back_to_openai() {
        assert_eq!(
            OpenAiCompatProvider::from_db_str("garbage"),
            OpenAiCompatProvider::OpenAi
        );
        assert_eq!(
            OpenAiCompatProvider::from_db_str(""),
            OpenAiCompatProvider::OpenAi
        );
    }

    #[test]
    fn provider_keychain_accounts_are_unique() {
        use std::collections::HashSet;
        let accounts: HashSet<_> = [
            OpenAiCompatProvider::OpenAi,
            OpenAiCompatProvider::Anthropic,
            OpenAiCompatProvider::Google,
            OpenAiCompatProvider::Custom,
        ]
        .into_iter()
        .map(OpenAiCompatProvider::keychain_account)
        .collect();
        assert_eq!(accounts.len(), 4, "Keychain account slugs must be unique");
    }

    #[test]
    fn from_preset_carries_provider_defaults() {
        let cfg =
            OpenAiCompatConfig::from_preset(OpenAiCompatProvider::Anthropic, "sk-xxx".to_string());
        assert_eq!(cfg.provider, OpenAiCompatProvider::Anthropic);
        assert!(cfg.endpoint.contains("anthropic"));
        assert!(cfg.model.starts_with("claude"));
        assert!(cfg.reasoning_effort.is_none());
    }

    #[test]
    fn openai_preset_sets_reasoning_effort_minimal() {
        let cfg =
            OpenAiCompatConfig::from_preset(OpenAiCompatProvider::OpenAi, "sk-xxx".to_string());
        assert_eq!(cfg.reasoning_effort.as_deref(), Some("minimal"));
    }

    #[test]
    fn google_preset_sets_reasoning_effort_none() {
        let cfg =
            OpenAiCompatConfig::from_preset(OpenAiCompatProvider::Google, "sk-xxx".to_string());
        assert_eq!(cfg.reasoning_effort.as_deref(), Some("none"));
    }

    #[test]
    fn custom_preset_sets_reasoning_effort_none_field() {
        let cfg = OpenAiCompatConfig::from_preset(OpenAiCompatProvider::Custom, "".to_string());
        assert!(cfg.reasoning_effort.is_none());
        assert!(cfg.endpoint.starts_with("http://localhost"));
    }

    #[test]
    fn chat_request_omits_reasoning_effort_when_none() {
        let req = ChatRequest {
            model: "test-model".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: "hi".to_string(),
            }],
            max_tokens: 32,
            temperature: 0.5,
            reasoning_effort: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(
            !json.contains("reasoning_effort"),
            "reasoning_effort must be skipped when None to preserve vanilla OpenAI compat"
        );
    }

    #[test]
    fn chat_request_includes_reasoning_effort_when_set() {
        let req = ChatRequest {
            model: "gpt-5".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: "hi".to_string(),
            }],
            max_tokens: 32,
            temperature: 0.5,
            reasoning_effort: Some("minimal".to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"reasoning_effort\":\"minimal\""));
    }

    /// Compile-time check: `OpenAiCompatConfig` must NOT be Serialize
    /// / Deserialize, so the `api_key` field cannot leak to disk via
    /// serde The following line would fail to compile if either derive
    /// were added:
    ///
    /// ```compile_fail
    /// use text_to_print_llm::openai_compat_backend::OpenAiCompatConfig;
    /// fn assert_serialize<T: serde::Serialize>() {}
    /// assert_serialize::<OpenAiCompatConfig>();
    /// ```
    #[test]
    fn config_not_serialize_by_construction() {
        // The compile_fail doctest above is the real check; this runtime
        // test just documents the intent
        let cfg =
            OpenAiCompatConfig::from_preset(OpenAiCompatProvider::OpenAi, "sk-secret".to_string());
        assert_eq!(cfg.api_key, "sk-secret");
    }
}
