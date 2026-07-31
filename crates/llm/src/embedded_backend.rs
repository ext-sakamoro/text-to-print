//! In-process embedded LLM backend (Stage 3-C)
//!
//! Links `alice-llm` directly as an rlib and runs GGUF inference in the
//! same process No subprocess, no HTTP hop The trade-off is that the
//! loaded model occupies process memory for the app's lifetime — swapping
//! to a different model (Qwen ↔ Bonsai) requires an app restart
//!
//! ## Chat template
//!
//! v1 only supports the **Qwen 2 / 3 / 3.5** family (`<|im_start|>` /
//! `<|im_end|>` markers) The default text-to-print model
//! (`qwen3.5-4b-q4_k_m`) is Qwen 3.5; the alternate Bonsai 27B is a
//! different family (Gemma-derived, `<start_of_turn>` markers) and is
//! currently only supported through [`BackendKind::Sidecar`]
//!
//! ## Lifetime
//!
//! `Llama3Model<'a>` borrows from the parsed `GgufFile<'a>` which in turn
//! borrows from the raw bytes To keep both alive for the process's
//! lifetime we `Box::leak` the byte buffer and the parsed file struct
//! This is intentional: the model must be loaded once and never dropped
//! (the alternative — self-referential structs — pulls in `ouroboros`
//! or unsafe pinning tricks that aren't worth the complexity for a
//! single-model desktop app)
//!
//! ## Concurrency
//!
//! Inference mutates the KV cache so the model can't be shared across
//! concurrent requests We wrap the model in `Arc<tokio::sync::Mutex<_>>`
//! and run each generation inside `tokio::task::spawn_blocking` so the
//! async runtime stays responsive during the ~10 s + inference window

use anyhow::{Context, Result, anyhow};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

use alice_llm::gguf::{GgufFile, GgufTokenizer};
use alice_llm::llama3::Llama3Model;

use crate::backend_kind::InferenceParams;

/// GGUF-backed in-process LLM backend
pub struct EmbeddedBackend {
    inner: Arc<Mutex<EmbeddedInner>>,
}

struct EmbeddedInner {
    tokenizer: GgufTokenizer,
    model: Llama3Model<'static>,
}

impl EmbeddedBackend {
    /// Load a GGUF model file into memory and prepare it for inference
    ///
    /// The entire file is read into a `Vec<u8>` and leaked so its bytes
    /// live for the app lifetime For a 2.4 GB Qwen 3.5-4B Q4_K_M model
    /// this is a ~2.4 GB one-time RAM cost; the trade-off vs mmap is
    /// simpler code and no page-fault stalls during the prefill phase
    ///
    /// # Errors
    ///
    /// - IO error reading `model_path`
    /// - GGUF parse failure (corrupt file / unsupported version)
    /// - Tokenizer / model construction failure (arch mismatch, missing
    ///   tensors, unsupported quant type)
    pub fn load(model_path: &Path) -> Result<Self> {
        tracing::info!(path = %model_path.display(), "loading embedded GGUF model");
        let bytes = std::fs::read(model_path)
            .with_context(|| format!("read GGUF file at {}", model_path.display()))?;
        let bytes_static: &'static [u8] = Box::leak(bytes.into_boxed_slice());

        let gguf = GgufFile::parse(bytes_static).ok_or_else(|| {
            anyhow!(
                "GGUF parse failed for {} (bad magic / unsupported version)",
                model_path.display()
            )
        })?;
        let gguf_static: &'static GgufFile<'static> = Box::leak(Box::new(gguf));

        let tokenizer = GgufTokenizer::from_gguf(gguf_static)
            .ok_or_else(|| anyhow!("tokenizer construction failed"))?;
        let model = Llama3Model::from_gguf(gguf_static)
            .ok_or_else(|| anyhow!("Llama3Model construction failed (arch not supported?)"))?;

        Ok(Self {
            inner: Arc::new(Mutex::new(EmbeddedInner { tokenizer, model })),
        })
    }

    /// One-shot chat generation The system + user messages are wrapped in
    /// the Qwen 2/3 chat template before being handed to the model
    ///
    /// The call is dispatched to a blocking thread pool worker via
    /// [`tokio::task::spawn_blocking`] so the async runtime stays free
    /// during the CPU-bound inference window
    ///
    /// # Errors
    ///
    /// - `spawn_blocking` join failure (panic in worker thread)
    /// - Model inference always returns text; there is no explicit error
    ///   path from `Llama3Model::generate` today If the model produces
    ///   no output, the caller sees an empty string
    pub async fn generate(
        &self,
        system: &str,
        user: &str,
        params: &InferenceParams,
    ) -> Result<String> {
        let prompt = format_qwen_chat(system, user);
        let inner = self.inner.clone();
        let max_tokens = params.max_tokens as usize;
        let temperature = params.temperature;
        let top_k = params.top_k;

        let raw = tokio::task::spawn_blocking(move || {
            let mut guard = inner.blocking_lock();
            let EmbeddedInner { tokenizer, model } = &mut *guard;
            model
                .generate(tokenizer, &prompt, max_tokens, temperature, top_k)
                .text
        })
        .await
        .with_context(|| "embedded inference worker panicked")?;

        Ok(strip_qwen_trailer(&raw))
    }
}

/// Format a system + user message pair into the Qwen 2/3/3.5 chat
/// template shape
fn format_qwen_chat(system: &str, user: &str) -> String {
    format!(
        "<|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{user}<|im_end|>\n<|im_start|>assistant\n"
    )
}

/// The Qwen assistant continuation ends with `<|im_end|>` which can leak
/// through when the tokenizer's `eos_id` doesn't match the im-end id
/// Trim the trailer plus any leading/trailing whitespace so callers get
/// pure assistant text
fn strip_qwen_trailer(text: &str) -> String {
    text.split("<|im_end|>")
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qwen_chat_template_contains_all_role_markers() {
        let out = format_qwen_chat("You are ALICE.", "Hello");
        assert!(out.starts_with("<|im_start|>system\n"));
        assert!(out.contains("You are ALICE.<|im_end|>"));
        assert!(out.contains("<|im_start|>user\nHello<|im_end|>"));
        assert!(out.ends_with("<|im_start|>assistant\n"));
    }

    #[test]
    fn strip_trailer_removes_im_end_and_whitespace() {
        let raw = "  sphere(20)\n<|im_end|>\nextra garbage";
        assert_eq!(strip_qwen_trailer(raw), "sphere(20)");
    }

    #[test]
    fn strip_trailer_no_marker_returns_trimmed() {
        assert_eq!(strip_qwen_trailer("  hello  "), "hello");
    }

    #[test]
    fn strip_trailer_empty_input_is_empty() {
        assert_eq!(strip_qwen_trailer(""), "");
    }

    #[test]
    fn chat_template_handles_empty_system_prompt() {
        let out = format_qwen_chat("", "Hi");
        assert!(out.contains("<|im_start|>system\n<|im_end|>"));
        assert!(out.contains("<|im_start|>user\nHi<|im_end|>"));
    }

    // NOTE: end-to-end `EmbeddedBackend::load` + `generate` requires a real
    // GGUF file on disk (~2.4 GB for Qwen 3.5-4B) The following test is
    // gated behind the `TEXT_TO_PRINT_EMBEDDED_GGUF` env var so CI stays
    // green while allowing local smoke tests when a model file is present
    #[tokio::test]
    #[ignore = "requires GGUF model on disk; set TEXT_TO_PRINT_EMBEDDED_GGUF"]
    async fn embedded_backend_end_to_end_smoke() {
        let Ok(path) = std::env::var("TEXT_TO_PRINT_EMBEDDED_GGUF") else {
            eprintln!("set TEXT_TO_PRINT_EMBEDDED_GGUF to a Qwen GGUF path");
            return;
        };
        let backend = EmbeddedBackend::load(Path::new(&path)).expect("load");
        let params = InferenceParams {
            max_tokens: 32,
            temperature: 0.1,
            top_k: 40,
        };
        let out = backend
            .generate("You are ALICE.", "Say 'sphere(1)' verbatim.", &params)
            .await
            .expect("generate");
        assert!(!out.is_empty(), "embedded generation should produce text");
    }
}
