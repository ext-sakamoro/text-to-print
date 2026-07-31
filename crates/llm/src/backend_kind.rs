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
}

/// Unified inference parameters used by both backends The sidecar path
/// forwards these into the OpenAI `chat/completions` body; the embedded
/// path passes them directly to `Llama3Model::generate`
#[derive(Debug, Clone, Copy)]
pub struct InferenceParams {
    pub max_tokens: u32,
    pub temperature: f32,
    /// Top-k sampling used by [`BackendKind::Embedded`] Sidecar backend
    /// currently ignores this (the OpenAI-compatible endpoint would need
    /// a `top_k` extension that isn't wired yet)
    pub top_k: usize,
}

impl Default for InferenceParams {
    fn default() -> Self {
        Self {
            max_tokens: 2048,
            temperature: 0.7,
            top_k: 40,
        }
    }
}

impl From<&LlmConfig> for InferenceParams {
    fn from(cfg: &LlmConfig) -> Self {
        Self {
            max_tokens: cfg.max_tokens,
            temperature: cfg.temperature,
            top_k: 40,
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
                crate::backend::generate(&effective, system, user).await
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
}
