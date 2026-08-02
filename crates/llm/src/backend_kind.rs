//! Backend abstraction (Stage 3-C)
//!
//! The desktop app can talk to the LLM through either of two backends:
//!
//! - [`BackendKind::Sidecar`] — HTTP round-trip to `alice-llm-server`
//!   spawned as a subprocess (`crates/llm/src/sidecar.rs`) Simple to
//!   operate, works with any OpenAI-compatible endpoint, but pays
//!   subprocess spawn + HTTP overhead per request
//! - [`BackendKind::Embedded`] — `alice-llm` linked directly as an rlib
//!   Zero-copy, no subprocess, no HTTP hop, but ties the process to a
//!   single loaded model for its lifetime (model swap requires restart)
//!
//! Both backends share the same [`InferenceParams`] shape so the caller
//! can flip [`BackendKind`] without rewriting the request path

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::backend::LlmConfig;
use crate::embedded_backend::EmbeddedBackend;

/// Which backend the app should route generation requests through
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BackendKind {
    /// HTTP round-trip to `alice-llm-server` subprocess (default;
    /// backward compat with the pre-3-C sidecar layout)
    #[default]
    Sidecar,
    /// In-process embedded inference via `alice-llm` rlib
    Embedded,
}

impl BackendKind {
    /// UI-facing label
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Sidecar => "Sidecar (HTTP)",
            Self::Embedded => "Embedded (in-process)",
        }
    }

    /// Stable string used for `profiles.backend_kind` column persistence
    /// Round-trips via [`Self::from_db_str`]
    #[must_use]
    pub const fn to_db_str(self) -> &'static str {
        match self {
            Self::Sidecar => "Sidecar",
            Self::Embedded => "Embedded",
        }
    }

    /// Inverse of [`Self::to_db_str`] Unknown / missing values fall back
    /// to the default (`Sidecar`) so pre-3-C.6 DBs read as Sidecar
    #[must_use]
    pub fn from_db_str(s: &str) -> Self {
        match s {
            "Embedded" => Self::Embedded,
            _ => Self::Sidecar,
        }
    }
}

/// UI-facing status of the [`BackendKind::Embedded`] backend Independent
/// of whether the user has selected Embedded — the model load runs in a
/// background task after selection and the UI polls this to display
/// progress
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbeddedStatus {
    /// User has not requested Embedded, or model file not present yet
    NotLoaded,
    /// Load in progress (GGUF parse + tensor dequant)
    Loading,
    /// Model loaded, ready to serve `generate` calls
    Ready,
    /// Load failed with `msg`
    Error(String),
}

impl EmbeddedStatus {
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::NotLoaded => "not loaded".to_string(),
            Self::Loading => "loading…".to_string(),
            Self::Ready => "ready".to_string(),
            Self::Error(m) => format!("error: {m}"),
        }
    }
}

/// Unified inference parameters used by both backends The sidecar path
/// forwards these into the OpenAI `chat/completions` body; the embedded
/// path passes them directly to `Llama3Model::generate`
///
/// `Clone`-not-`Copy` because the [`Self::grammar`] field is `String`
/// (owned GBNF source) The struct is still lightweight — the grammar
/// string is only a few hundred lines and cheap to `Arc<T>` upstream if
/// per-request cloning ever becomes a bottleneck
#[derive(Debug, Clone)]
pub struct InferenceParams {
    pub max_tokens: u32,
    pub temperature: f32,
    /// Top-k sampling used by [`BackendKind::Embedded`] Sidecar backend
    /// currently ignores this (the OpenAI-compatible endpoint would need
    /// a `top_k` extension that isn't wired yet)
    pub top_k: usize,
    /// Optional GBNF source (Stage 3-C.11) When `Some`, the embedded
    /// backend uses a custom decode loop that masks logits against the
    /// grammar per token, guaranteeing well-formed output The sidecar
    /// backend forwards this in the OpenAI `chat/completions` request
    /// body under the `grammar` field (accepted by `alice-llm-server`)
    /// `None` = free-form generation, backward-compatible with
    /// pre-3-C.11 behaviour
    pub grammar: Option<String>,
}

impl Default for InferenceParams {
    fn default() -> Self {
        Self {
            max_tokens: 2048,
            temperature: 0.7,
            top_k: 40,
            grammar: None,
        }
    }
}

impl From<&LlmConfig> for InferenceParams {
    fn from(cfg: &LlmConfig) -> Self {
        Self {
            max_tokens: cfg.max_tokens,
            temperature: cfg.temperature,
            top_k: 40,
            grammar: None,
        }
    }
}

/// Type-erased backend the app holds instead of one or the other
/// concrete impl `LlmBackend::Sidecar` keeps the [`LlmConfig`] so the
/// existing HTTP dispatch (`crate::backend::generate`) can be reached
/// without a second config plumb
pub enum LlmBackend {
    Sidecar(LlmConfig),
    Embedded(EmbeddedBackend),
}

impl LlmBackend {
    /// Kind discriminant for UI / logging
    #[must_use]
    pub const fn kind(&self) -> BackendKind {
        match self {
            Self::Sidecar(_) => BackendKind::Sidecar,
            Self::Embedded(_) => BackendKind::Embedded,
        }
    }

    /// One-shot chat generation The `params` argument overrides the
    /// `max_tokens` / `temperature` from any embedded [`LlmConfig`]
    ///
    /// # Errors
    ///
    /// Any transport / inference error bubbles up unchanged
    pub async fn generate(
        &self,
        system: &str,
        user: &str,
        params: &InferenceParams,
    ) -> Result<String> {
        match self {
            Self::Sidecar(cfg) => {
                // Sidecar backend uses the LlmConfig fields directly Callers
                // that want to override should clone the config and mutate
                let effective = LlmConfig {
                    max_tokens: params.max_tokens,
                    temperature: params.temperature,
                    ..cfg.clone()
                };
                crate::backend::generate_with_grammar(
                    &effective,
                    system,
                    user,
                    params.grammar.as_deref(),
                )
                .await
            }
            Self::Embedded(inner) => inner.generate(system, user, params).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_kind_is_sidecar() {
        assert_eq!(BackendKind::default(), BackendKind::Sidecar);
    }

    #[test]
    fn backend_kind_labels_are_non_empty() {
        assert!(!BackendKind::Sidecar.label().is_empty());
        assert!(!BackendKind::Embedded.label().is_empty());
    }

    #[test]
    fn backend_kind_serialization_roundtrip() {
        for kind in [BackendKind::Sidecar, BackendKind::Embedded] {
            let json = serde_json::to_string(&kind).unwrap();
            let back: BackendKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, back);
        }
    }

    #[test]
    fn inference_params_from_llm_config_copies_temp_and_max_tokens() {
        let cfg = LlmConfig {
            max_tokens: 512,
            temperature: 0.3,
            ..Default::default()
        };
        let params = InferenceParams::from(&cfg);
        assert_eq!(params.max_tokens, 512);
        assert!((params.temperature - 0.3).abs() < f32::EPSILON);
        assert_eq!(params.top_k, 40);
    }

    #[test]
    fn sidecar_variant_reports_sidecar_kind() {
        let backend = LlmBackend::Sidecar(LlmConfig::default());
        assert_eq!(backend.kind(), BackendKind::Sidecar);
    }

    #[test]
    fn to_db_str_from_db_str_roundtrip() {
        for kind in [BackendKind::Sidecar, BackendKind::Embedded] {
            assert_eq!(BackendKind::from_db_str(kind.to_db_str()), kind);
        }
    }

    #[test]
    fn from_db_str_unknown_falls_back_to_sidecar() {
        assert_eq!(BackendKind::from_db_str("garbage"), BackendKind::Sidecar);
        assert_eq!(BackendKind::from_db_str(""), BackendKind::Sidecar);
    }

    #[test]
    fn embedded_status_is_ready_only_for_ready() {
        assert!(!EmbeddedStatus::NotLoaded.is_ready());
        assert!(!EmbeddedStatus::Loading.is_ready());
        assert!(EmbeddedStatus::Ready.is_ready());
        assert!(!EmbeddedStatus::Error("x".into()).is_ready());
    }

    #[test]
    fn embedded_status_label_non_empty() {
        for s in [
            EmbeddedStatus::NotLoaded,
            EmbeddedStatus::Loading,
            EmbeddedStatus::Ready,
            EmbeddedStatus::Error("boom".into()),
        ] {
            assert!(!s.label().is_empty());
        }
    }
}
