//! In-process embedded LLM backend (Stage 3-C)
//!
//! Links `alice-llm` directly as an rlib and runs GGUF inference in the
//! same process No subprocess, no HTTP hop
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
//! ## Worker thread architecture (Stage 3-C.9)
//!
//! Loading a `Llama3Model<'a>` produces a struct that borrows from a
//! parsed `GgufFile<'a>` which borrows from mmap bytes A previous
//! iteration `Box::leak`-ed the mmap to obtain `'static` — that ruled out
//! ever dropping the model, so switching between Qwen and Bonsai at
//! runtime would leak the entire prior model into process memory
//!
//! The current design instead pins the mmap + parsed file + model inside
//! a dedicated OS thread The worker thread owns all three variables as
//! locals of its main closure; their lifetimes are naturally bounded by
//! the thread lifetime and no leak is needed The caller talks to the
//! worker via a `tokio::sync::mpsc::UnboundedSender<Command>`; per-request
//! responses come back through a `tokio::sync::oneshot` channel
//!
//! `Drop` of the last [`EmbeddedBackend`] clone closes the command
//! channel, the worker loop exits, and the thread's stack unwinds —
//! releasing mmap / GgufFile / Llama3Model in that order This is what
//! makes multi-model runtime switching in [`crate::backend_kind`]
//! straightforward: swap `EmbeddedBackend` values in an `AppState` slot
//! and the prior model's memory is reclaimed as soon as the last
//! `Clone` goes out of scope

use anyhow::{Context, Result, anyhow};
use memmap2::Mmap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc as sync_mpsc;
use tokio::sync::{mpsc, oneshot};

use alice_llm::gguf::{GgufFile, GgufTokenizer};
use alice_llm::llama3::Llama3Model;

use crate::backend_kind::InferenceParams;

/// GGUF-backed in-process LLM backend
///
/// `Clone` is a cheap `Arc` bump — all clones share the same worker
/// thread and serialise inference requests through the same command
/// channel
#[derive(Clone)]
pub struct EmbeddedBackend {
    inner: Arc<BackendShared>,
}

struct BackendShared {
    tx: mpsc::UnboundedSender<Command>,
    chat_template: ChatTemplate,
    model_path: PathBuf,
}

enum Command {
    Generate {
        system: String,
        user: String,
        params: InferenceParams,
        response: oneshot::Sender<String>,
    },
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
    /// Load a GGUF model file via mmap and spawn the worker thread that
    /// will serve inference requests
    ///
    /// The GGUF file is memory-mapped read-only inside the worker thread
    /// (kernel demand-pages weight tensors during inference, keeping the
    /// resident-set spike small at load time) The tokenizer, model, and
    /// detected chat template all become worker-thread locals; the
    /// caller's [`EmbeddedBackend`] holds only a `Send`-safe channel plus
    /// the detected [`ChatTemplate`] cache
    ///
    /// # Errors
    ///
    /// - OS thread spawn failure (extremely rare)
    /// - IO error opening `model_path`
    /// - `Mmap::map` failure (unusual on well-known file systems)
    /// - GGUF parse failure (corrupt file / unsupported version)
    /// - Tokenizer / model construction failure (arch mismatch, missing
    ///   tensors, unsupported quant type)
    pub fn load(model_path: &Path) -> Result<Self> {
        tracing::info!(path = %model_path.display(), "mmap loading embedded GGUF model");
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<Command>();
        // `init_tx` communicates the load result back to the caller
        // synchronously so `load()` returns an error if the GGUF fails to
        // parse (rather than surfacing the failure on the first
        // `generate` call, which would be much more confusing)
        let (init_tx, init_rx) = sync_mpsc::sync_channel::<Result<ChatTemplate>>(1);
        let mp = model_path.to_path_buf();

        std::thread::Builder::new()
            .name("embedded-llm-worker".to_string())
            .spawn(move || {
                worker_main(&mp, init_tx, cmd_rx);
            })
            .with_context(|| "spawn embedded-llm-worker thread")?;

        let chat_template = init_rx
            .recv()
            .with_context(|| "worker thread exited before init handshake")??;

        Ok(Self {
            inner: Arc::new(BackendShared {
                tx: cmd_tx,
                chat_template,
                model_path: model_path.to_path_buf(),
            }),
        })
    }

    /// One-shot chat generation The system + user messages are wrapped in
    /// the detected chat template before being handed to the model
    ///
    /// # Errors
    ///
    /// - Worker thread has died / dropped its channel (`Ok(())` normally,
    ///   this is only observed after an internal panic)
    /// - Worker never replies (also only after an internal panic)
    pub async fn generate(
        &self,
        system: &str,
        user: &str,
        params: &InferenceParams,
    ) -> Result<String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.inner
            .tx
            .send(Command::Generate {
                system: system.to_string(),
                user: user.to_string(),
                params: *params,
                response: resp_tx,
            })
            .map_err(|_| anyhow!("embedded worker thread has exited"))?;
        resp_rx
            .await
            .map_err(|_| anyhow!("embedded worker dropped the response channel"))
    }

    /// Which chat template family the loaded model uses (diagnostic /
    /// UI display) Cheap: reads the cached value from the shared struct
    #[must_use]
    pub fn chat_template(&self) -> ChatTemplate {
        self.inner.chat_template
    }

    /// Path to the GGUF file backing this worker Used by the app layer to
    /// decide whether a `ModelChoice` change requires a swap
    #[must_use]
    pub fn model_path(&self) -> &Path {
        &self.inner.model_path
    }
}

/// Worker thread entry point Owns the mmap / GgufFile / Llama3Model as
/// stack locals; drops them when the command channel closes
fn worker_main(
    model_path: &Path,
    init_tx: sync_mpsc::SyncSender<Result<ChatTemplate>>,
    mut cmd_rx: mpsc::UnboundedReceiver<Command>,
) {
    let file = match std::fs::File::open(model_path)
        .with_context(|| format!("open GGUF file at {}", model_path.display()))
    {
        Ok(f) => f,
        Err(e) => {
            let _ = init_tx.send(Err(e));
            return;
        }
    };
    // SAFETY: the file is read-only and we hold the `Mmap` for as long
    // as the worker thread lives (no aliased mutable access can occur
    // through the mapping) See module-level docs on lifetime management
    let mmap = match unsafe { Mmap::map(&file) }
        .with_context(|| format!("mmap GGUF file at {}", model_path.display()))
    {
        Ok(m) => m,
        Err(e) => {
            let _ = init_tx.send(Err(e));
            return;
        }
    };
    let bytes: &[u8] = mmap.as_ref();

    let Some(gguf) = GgufFile::parse(bytes) else {
        let _ = init_tx.send(Err(anyhow!(
            "GGUF parse failed for {} (bad magic / unsupported version)",
            model_path.display()
        )));
        return;
    };

    let Some(tokenizer) = GgufTokenizer::from_gguf(&gguf) else {
        let _ = init_tx.send(Err(anyhow!("tokenizer construction failed")));
        return;
    };

    let Some(mut model) = Llama3Model::from_gguf(&gguf) else {
        let _ = init_tx.send(Err(anyhow!(
            "Llama3Model construction failed (arch not supported?)"
        )));
        return;
    };

    let chat_template = ChatTemplate::detect(&gguf);
    tracing::info!(?chat_template, "detected chat template");
    if init_tx.send(Ok(chat_template)).is_err() {
        // Caller dropped the init receiver before we handshook — nothing
        // will consume our results Bail out to avoid a leaked worker
        return;
    }

    while let Some(cmd) = cmd_rx.blocking_recv() {
        match cmd {
            Command::Generate {
                system,
                user,
                params,
                response,
            } => {
                let prompt = format_chat(chat_template, &system, &user);
                let result = model.generate(
                    &tokenizer,
                    &prompt,
                    params.max_tokens as usize,
                    params.temperature,
                    params.top_k,
                );
                let out = strip_trailer(chat_template, &result.text);
                // Ignore send error — caller may have cancelled the await
                let _ = response.send(out);
            }
        }
    }

    tracing::info!(path = %model_path.display(), "embedded-llm-worker exiting cleanly");
    // mmap / gguf / tokenizer / model drop here as the stack unwinds
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

    #[test]
    fn load_returns_error_for_missing_file() {
        // The worker init handshake should surface a synchronous error
        // when the GGUF path doesn't exist rather than silently spawning a
        // zombie thread
        let result = EmbeddedBackend::load(Path::new("/nonexistent/model.gguf"));
        let err = match result {
            Ok(_) => panic!("expected error for missing model file"),
            Err(e) => e,
        };
        let msg = format!("{err:#}");
        assert!(
            msg.contains("open GGUF file"),
            "expected open error, got: {msg}"
        );
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
