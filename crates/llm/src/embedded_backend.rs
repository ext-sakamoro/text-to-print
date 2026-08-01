//! In-process embedded LLM backend (Stage 3-C)
//!
//! Links `alice-llm` directly as an rlib and runs GGUF inference in the
//! same process No subprocess, no HTTP hop The trade-off is that the
//! loaded model occupies process memory for the app's lifetime — swapping
//! to a different model (Qwen ↔ Bonsai) requires an app restart
//!
//! ## Chat template
//!
//! Two chat template families are supported, auto-detected from the GGUF
//! metadata at load time:
//!
//! - **Qwen 2 / 3 / 3.5** — `<|im_start|>role\n...<|im_end|>\n` markers
//!   Default `qwen3.5-4b-q4_k_m` uses this
//! - **Gemma 2 / 3n / 3 / Bonsai 27B** — `<start_of_turn>role\n...
//!   <end_of_turn>\n` markers Assistant role is spelled `model` in the
//!   emitted prompt, matching upstream Gemma tokenizer conventions
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
use memmap2::Mmap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

use alice_llm::gguf::{GgufFile, GgufTokenizer};
use alice_llm::llama3::Llama3Model;

use crate::backend_kind::InferenceParams;

/// GGUF-backed in-process LLM backend
///
/// `Clone` is a cheap `Arc` bump — clones share the underlying model and
/// serialise inference requests through the same `tokio::sync::Mutex`
#[derive(Clone)]
pub struct EmbeddedBackend {
    inner: Arc<Mutex<EmbeddedInner>>,
}

struct EmbeddedInner {
    tokenizer: GgufTokenizer,
    model: Llama3Model<'static>,
    chat_template: ChatTemplate,
}

/// Prompt formatting family detected from the loaded GGUF
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatTemplate {
    /// `<|im_start|>role\n...<|im_end|>\n` used by Qwen 2 / 3 / 3.5
    Qwen2,
    /// `<start_of_turn>role\n...<end_of_turn>\n` used by Gemma 2 / 3n / 3
    /// and Bonsai 27B Assistant role is emitted as `model`
    Gemma,
}

impl ChatTemplate {
    /// Detect the chat template family from GGUF metadata Uses the
    /// authoritative `tokenizer.chat_template` string when present, else
    /// falls back to the `general.architecture` slug
    ///
    /// Unknown architectures default to [`Self::Qwen2`] so a user pointing
    /// at an unrecognised model still gets *some* prompt shape
    #[must_use]
    pub fn detect(gguf: &GgufFile<'_>) -> Self {
        if let Some(tmpl) = gguf.meta_str("tokenizer.chat_template") {
            if tmpl.contains("start_of_turn") {
                return Self::Gemma;
            }
            if tmpl.contains("im_start") {
                return Self::Qwen2;
            }
        }
        match gguf.meta_str("general.architecture") {
            Some(arch) if arch.starts_with("gemma") => Self::Gemma,
            _ => Self::Qwen2,
        }
    }
}

impl EmbeddedBackend {
    /// Load a GGUF model file via mmap and prepare it for inference
    ///
    /// The file is memory-mapped read-only into the process address space
    /// via [`memmap2::Mmap`] so the kernel pages weight tensors in on
    /// demand For a 2.4 GB Qwen 3.5-4B Q4_K_M model this drops the load-
    /// time resident-set spike from ~2.4 GB to ~50 MB (only the parsed
    /// header + tokenizer metadata) The mmap `struct` is `Box::leak`ed to
    /// give the borrowed `GgufFile<'static>` / `Llama3Model<'static>` the
    /// lifetime they need; the mapping lives for the process lifetime
    ///
    /// # Errors
    ///
    /// - IO error opening `model_path`
    /// - `Mmap::map` failure (unusual on well-known file systems)
    /// - GGUF parse failure (corrupt file / unsupported version)
    /// - Tokenizer / model construction failure (arch mismatch, missing
    ///   tensors, unsupported quant type)
    pub fn load(model_path: &Path) -> Result<Self> {
        tracing::info!(path = %model_path.display(), "mmap loading embedded GGUF model");
        let file = std::fs::File::open(model_path)
            .with_context(|| format!("open GGUF file at {}", model_path.display()))?;
        // SAFETY: the file is read-only and we hold the `Mmap` for the
        // process lifetime (via `Box::leak` below), so no aliased mutable
        // access can occur through the mapping
        let mmap = unsafe { Mmap::map(&file) }
            .with_context(|| format!("mmap GGUF file at {}", model_path.display()))?;
        let mmap_static: &'static Mmap = Box::leak(Box::new(mmap));
        let bytes_static: &'static [u8] = mmap_static.as_ref();

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
        let chat_template = ChatTemplate::detect(gguf_static);
        tracing::info!(?chat_template, "detected chat template");

        Ok(Self {
            inner: Arc::new(Mutex::new(EmbeddedInner {
                tokenizer,
                model,
                chat_template,
            })),
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
        let inner = self.inner.clone();
        let max_tokens = params.max_tokens as usize;
        let temperature = params.temperature;
        let top_k = params.top_k;
        let system_owned = system.to_string();
        let user_owned = user.to_string();

        let (raw, template) = tokio::task::spawn_blocking(move || {
            let mut guard = inner.blocking_lock();
            let EmbeddedInner {
                tokenizer,
                model,
                chat_template,
            } = &mut *guard;
            let prompt = format_chat(*chat_template, &system_owned, &user_owned);
            let text = model
                .generate(tokenizer, &prompt, max_tokens, temperature, top_k)
                .text;
            (text, *chat_template)
        })
        .await
        .with_context(|| "embedded inference worker panicked")?;

        Ok(strip_trailer(template, &raw))
    }

    /// Which chat template family the loaded model uses (test / diagnostic)
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned — this can only happen if
    /// a previous inference call itself panicked, in which case the model
    /// state is already unrecoverable
    #[must_use]
    pub fn chat_template(&self) -> ChatTemplate {
        self.inner.blocking_lock().chat_template
    }
}

/// Format a system + user message pair according to `template`
fn format_chat(template: ChatTemplate, system: &str, user: &str) -> String {
    match template {
        ChatTemplate::Qwen2 => format_qwen_chat(system, user),
        ChatTemplate::Gemma => format_gemma_chat(system, user),
    }
}

/// Trim the template-specific end-of-turn / end-of-message marker from
/// the raw model output so callers see pure assistant text
fn strip_trailer(template: ChatTemplate, text: &str) -> String {
    match template {
        ChatTemplate::Qwen2 => strip_qwen_trailer(text),
        ChatTemplate::Gemma => strip_gemma_trailer(text),
    }
}

/// Format a system + user message pair into the Qwen 2/3/3.5 chat
/// template shape
fn format_qwen_chat(system: &str, user: &str) -> String {
    format!(
        "<|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{user}<|im_end|>\n<|im_start|>assistant\n"
    )
}

/// Format a system + user message pair into the Gemma 2/3/3n chat
/// template shape Assistant role is spelled `model` per upstream Gemma
/// convention
fn format_gemma_chat(system: &str, user: &str) -> String {
    format!(
        "<start_of_turn>system\n{system}<end_of_turn>\n<start_of_turn>user\n{user}<end_of_turn>\n<start_of_turn>model\n"
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

/// Gemma continuations terminate with `<end_of_turn>` Trim it + adjacent
/// whitespace for parity with [`strip_qwen_trailer`]
fn strip_gemma_trailer(text: &str) -> String {
    text.split("<end_of_turn>")
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
    fn gemma_chat_template_contains_all_role_markers() {
        let out = format_gemma_chat("You are ALICE.", "Hello");
        assert!(out.starts_with("<start_of_turn>system\n"));
        assert!(out.contains("You are ALICE.<end_of_turn>"));
        assert!(out.contains("<start_of_turn>user\nHello<end_of_turn>"));
        // Assistant role in Gemma is spelled `model`
        assert!(out.ends_with("<start_of_turn>model\n"));
    }

    #[test]
    fn format_chat_dispatches_on_template_variant() {
        let qwen = format_chat(ChatTemplate::Qwen2, "s", "u");
        assert!(qwen.contains("<|im_start|>"));
        let gemma = format_chat(ChatTemplate::Gemma, "s", "u");
        assert!(gemma.contains("<start_of_turn>"));
    }

    #[test]
    fn strip_qwen_trailer_removes_im_end_and_whitespace() {
        let raw = "  sphere(20)\n<|im_end|>\nextra garbage";
        assert_eq!(strip_qwen_trailer(raw), "sphere(20)");
    }

    #[test]
    fn strip_gemma_trailer_removes_end_of_turn_and_whitespace() {
        let raw = "  sphere(20)\n<end_of_turn>\nextra garbage";
        assert_eq!(strip_gemma_trailer(raw), "sphere(20)");
    }

    #[test]
    fn strip_trailer_dispatches_on_template_variant() {
        assert_eq!(strip_trailer(ChatTemplate::Qwen2, "a<|im_end|>b"), "a");
        assert_eq!(strip_trailer(ChatTemplate::Gemma, "a<end_of_turn>b"), "a");
    }

    #[test]
    fn strip_qwen_trailer_no_marker_returns_trimmed() {
        assert_eq!(strip_qwen_trailer("  hello  "), "hello");
    }

    #[test]
    fn strip_gemma_trailer_no_marker_returns_trimmed() {
        assert_eq!(strip_gemma_trailer("  hello  "), "hello");
    }

    #[test]
    fn strip_qwen_trailer_empty_input_is_empty() {
        assert_eq!(strip_qwen_trailer(""), "");
    }

    #[test]
    fn chat_template_handles_empty_system_prompt() {
        let qwen = format_qwen_chat("", "Hi");
        assert!(qwen.contains("<|im_start|>system\n<|im_end|>"));
        assert!(qwen.contains("<|im_start|>user\nHi<|im_end|>"));
        let gemma = format_gemma_chat("", "Hi");
        assert!(gemma.contains("<start_of_turn>system\n<end_of_turn>"));
        assert!(gemma.contains("<start_of_turn>user\nHi<end_of_turn>"));
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
