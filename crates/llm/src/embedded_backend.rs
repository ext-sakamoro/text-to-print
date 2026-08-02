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
use alice_llm::gpu::{GpuEngine, GpuModel, GpuModelConfig};
use alice_llm::grammar::{Fsm, parse_gbnf};
use alice_llm::llama3::{Llama3Config, Llama3Model};
use alice_llm::sampling::{advance_fsm_on_emit, mask_logits_by_grammar};
use alice_llm::{
    apply_temperature, sample_argmax, sample_with_random, softmax_inplace, top_k_filter,
};

use crate::backend_kind::{ExecutionMode, InferenceParams};

/// Unified trait so [`run_decode`] can drive either the CPU
/// (`Llama3Model`) or GPU (`GpuModel`) forward pass with the same
/// grammar mask + sampling loop
trait ModelForward {
    /// Forward one token, return the logits distribution
    fn forward_read(&mut self, token: u32) -> Vec<f32>;
    /// Clear internal KV cache (start of a new sequence)
    fn clear(&mut self);
}

impl ModelForward for Llama3Model<'_> {
    fn forward_read(&mut self, token: u32) -> Vec<f32> {
        self.forward(token)
    }
    fn clear(&mut self) {
        self.clear_cache();
    }
}

impl ModelForward for GpuModel {
    fn forward_read(&mut self, token: u32) -> Vec<f32> {
        self.forward_and_read(token)
    }
    fn clear(&mut self) {
        self.reset();
    }
}

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
    execution_mode: ExecutionMode,
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
    /// Load a GGUF model with the default [`ExecutionMode::Cpu`] mode
    ///
    /// Equivalent to `load_with_mode(path, ExecutionMode::Cpu)` — kept
    /// for source-compat with pre-3-C.12 callers
    ///
    /// # Errors
    ///
    /// See [`Self::load_with_mode`]
    pub fn load(model_path: &Path) -> Result<Self> {
        Self::load_with_mode(model_path, ExecutionMode::Cpu)
    }

    /// Load a GGUF model in the specified execution mode
    ///
    /// - `ExecutionMode::Cpu` spins up a CPU worker that owns
    ///   `Mmap` + `GgufFile` + `Llama3Model` on its stack
    /// - `ExecutionMode::Gpu` spins up a GPU worker that additionally
    ///   allocates a `GpuEngine` (wgpu adapter + device) and uploads all
    ///   weight tensors into `GpuModel` GPU buffers The `Mmap` is still
    ///   held so tokenizer + config metadata stay accessible
    ///
    /// # Errors
    ///
    /// - OS thread spawn failure (extremely rare)
    /// - IO error opening `model_path`
    /// - `Mmap::map` failure (unusual on well-known file systems)
    /// - GGUF parse failure (corrupt file / unsupported version)
    /// - Tokenizer / model construction failure (arch mismatch, missing
    ///   tensors, unsupported quant type)
    /// - GPU-only: adapter / device request failure (no adapter or
    ///   insufficient VRAM)
    pub fn load_with_mode(model_path: &Path, execution_mode: ExecutionMode) -> Result<Self> {
        tracing::info!(
            path = %model_path.display(),
            ?execution_mode,
            "mmap loading embedded GGUF model"
        );
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<Command>();
        let (init_tx, init_rx) = sync_mpsc::sync_channel::<Result<ChatTemplate>>(1);
        let mp = model_path.to_path_buf();
        let thread_name = match execution_mode {
            ExecutionMode::Cpu => "embedded-llm-worker-cpu",
            ExecutionMode::Gpu => "embedded-llm-worker-gpu",
        };

        std::thread::Builder::new()
            .name(thread_name.to_string())
            .spawn(move || match execution_mode {
                ExecutionMode::Cpu => worker_main_cpu(&mp, init_tx, cmd_rx),
                ExecutionMode::Gpu => worker_main_gpu(&mp, init_tx, cmd_rx),
            })
            .with_context(|| format!("spawn {thread_name} thread"))?;

        let chat_template = init_rx
            .recv()
            .with_context(|| "worker thread exited before init handshake")??;

        Ok(Self {
            inner: Arc::new(BackendShared {
                tx: cmd_tx,
                chat_template,
                model_path: model_path.to_path_buf(),
                execution_mode,
            }),
        })
    }

    /// Which execution mode the loaded worker is using (diagnostic)
    #[must_use]
    pub fn execution_mode(&self) -> ExecutionMode {
        self.inner.execution_mode
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
                params: params.clone(),
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

/// Load `Mmap` + parse GGUF Small helper shared between the CPU and GPU
/// worker init paths Reports errors through `init_tx` and returns `None`
/// so callers can early-return on failure
fn init_gguf(
    model_path: &Path,
    init_tx: &sync_mpsc::SyncSender<Result<ChatTemplate>>,
) -> Option<Mmap> {
    let file = match std::fs::File::open(model_path)
        .with_context(|| format!("open GGUF file at {}", model_path.display()))
    {
        Ok(f) => f,
        Err(e) => {
            let _ = init_tx.send(Err(e));
            return None;
        }
    };
    // SAFETY: the file is read-only and we hold the `Mmap` for as long
    // as the worker thread lives (no aliased mutable access can occur
    // through the mapping)
    match unsafe { Mmap::map(&file) }
        .with_context(|| format!("mmap GGUF file at {}", model_path.display()))
    {
        Ok(m) => Some(m),
        Err(e) => {
            let _ = init_tx.send(Err(e));
            None
        }
    }
}

/// CPU worker thread entry point Owns the mmap / GgufFile / Llama3Model
/// as stack locals; drops them when the command channel closes
fn worker_main_cpu(
    model_path: &Path,
    init_tx: sync_mpsc::SyncSender<Result<ChatTemplate>>,
    mut cmd_rx: mpsc::UnboundedReceiver<Command>,
) {
    let Some(mmap) = init_gguf(model_path, &init_tx) else {
        return;
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
    tracing::info!(?chat_template, backend = "cpu", "detected chat template");
    if init_tx.send(Ok(chat_template)).is_err() {
        return;
    }

    serve_commands(&mut model, &tokenizer, chat_template, &mut cmd_rx);
    tracing::info!(path = %model_path.display(), "embedded-llm-worker-cpu exiting cleanly");
}

/// GPU worker thread entry point Mirrors [`worker_main_cpu`] but wraps a
/// `GpuModel` behind `alice_llm::gpu::GpuEngine` The `GpuEngine` +
/// `GpuModel` are stack locals; dropping them releases the wgpu adapter
/// and GPU buffers cleanly
fn worker_main_gpu(
    model_path: &Path,
    init_tx: sync_mpsc::SyncSender<Result<ChatTemplate>>,
    mut cmd_rx: mpsc::UnboundedReceiver<Command>,
) {
    let Some(mmap) = init_gguf(model_path, &init_tx) else {
        return;
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
    let Some(llm_config) = Llama3Config::from_gguf(&gguf) else {
        let _ = init_tx.send(Err(anyhow!(
            "Llama3Config::from_gguf failed (missing metadata?)"
        )));
        return;
    };

    let gpu_config = gpu_config_from_llama3(&llm_config);
    let engine = GpuEngine::new();
    // `GpuModel::load` panics on unrecoverable init errors (e.g.
    // architecture not supported by the GPU code path) Catch that so
    // the caller sees `EmbeddedStatus::Error(...)` instead of the
    // worker thread crashing silently
    let model_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        GpuModel::load(engine, &gguf, gpu_config)
    }));
    let mut model = match model_result {
        Ok(m) => m,
        Err(panic) => {
            let msg = panic_message(&panic);
            let _ = init_tx.send(Err(anyhow!("GpuModel::load panicked: {msg}")));
            return;
        }
    };

    let chat_template = ChatTemplate::detect(&gguf);
    tracing::info!(?chat_template, backend = "gpu", "detected chat template");
    if init_tx.send(Ok(chat_template)).is_err() {
        return;
    }

    serve_commands(&mut model, &tokenizer, chat_template, &mut cmd_rx);
    tracing::info!(path = %model_path.display(), "embedded-llm-worker-gpu exiting cleanly");
}

/// Best-effort string extraction from a boxed panic payload
fn panic_message(panic: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = panic.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

/// Extract the `GpuModelConfig` fields from a parsed `Llama3Config`
/// Mirrors the mapping in `alice-llm-server::main`
fn gpu_config_from_llama3(llm: &Llama3Config) -> GpuModelConfig {
    #[allow(clippy::cast_possible_truncation)]
    GpuModelConfig {
        num_layers: llm.num_layers,
        hidden_dim: llm.hidden_dim,
        intermediate_dim: llm.intermediate_dim,
        num_heads: llm.num_heads as u32,
        num_kv_heads: llm.num_kv_heads as u32,
        head_dim: llm.head_dim as u32,
        rope_theta: llm.rope_theta,
        eps: llm.norm_eps,
        max_seq_len: llm.max_seq_len,
        neox_rope: llm.use_neox_rope(),
        full_attention_interval: llm.full_attention_interval(),
        linear_num_kv_heads: llm.linear_num_kv_heads().map(|v| v as u32),
        linear_qk_head_dim: llm.linear_qk_head_dim().map(|v| v as u32),
        linear_kv_head_dim: llm.linear_kv_head_dim().map(|v| v as u32),
        linear_num_v_heads: llm.linear_num_v_heads().map(|v| v as u32),
        linear_conv_kernel_dim: llm.linear_conv_kernel_dim().map(|v| v as u32),
        attention_only_load: false,
    }
}

/// Backend-agnostic command loop shared between CPU and GPU workers
///
/// `M: ModelForward` erases the concrete `Llama3Model` / `GpuModel`
/// choice The grammar-free fast path calls `Llama3Model::generate` for
/// CPU (its stateful decode loop is heavily tuned) but reconstructs the
/// same loop through the trait for GPU / grammar-constrained paths
fn serve_commands<M: ModelForward>(
    model: &mut M,
    tokenizer: &GgufTokenizer,
    chat_template: ChatTemplate,
    cmd_rx: &mut mpsc::UnboundedReceiver<Command>,
) {
    while let Some(cmd) = cmd_rx.blocking_recv() {
        match cmd {
            Command::Generate {
                system,
                user,
                params,
                response,
            } => {
                let prompt = format_chat(chat_template, &system, &user);
                let text = if params.grammar.is_some() {
                    match run_decode(model, tokenizer, &prompt, &params) {
                        Ok(text) => text,
                        Err(e) => {
                            tracing::warn!(error = %e, "grammar-constrained decode failed");
                            String::new()
                        }
                    }
                } else {
                    match run_decode(model, tokenizer, &prompt, &params) {
                        Ok(text) => text,
                        Err(e) => {
                            tracing::warn!(error = %e, "unconstrained decode failed");
                            String::new()
                        }
                    }
                };
                let out = strip_trailer(chat_template, &text);
                // Ignore send error — caller may have cancelled the await
                let _ = response.send(out);
            }
        }
    }
}

/// Stage 3-C.11 / 3-C.12: unified autoregressive decode
///
/// Works for both CPU (`Llama3Model`) and GPU (`GpuModel`) backends via
/// the [`ModelForward`] trait When `params.grammar` is `Some`, the loop
/// masks logits per token against the GBNF FSM and stops early once the
/// grammar reaches `is_final` Otherwise the loop is equivalent to
/// `Llama3Model::generate` minus repetition-penalty defaults (grammar
/// mode doesn't need them)
///
/// The `max_depth` value is copied from the alice-lol bridge default; the
/// LOL DSL grammar can nest deeply enough that lower values would trip
/// `FsmError::RecursionOverflow`
///
/// # Errors
///
/// - GBNF parse failure (`params.grammar` set but malformed)
/// - `Fsm::start` returned an error (empty root rule etc.)
/// - `Fsm::advance` diverged from the grammar mask (indicates a
///   sampling / masking bug rather than input error)
fn run_decode<M: ModelForward>(
    model: &mut M,
    tokenizer: &GgufTokenizer,
    prompt: &str,
    params: &InferenceParams,
) -> Result<String> {
    // Optional grammar setup
    let (mut fsm, grammar_owner) = if let Some(gbnf) = params.grammar.as_deref() {
        let grammar = parse_gbnf(gbnf).with_context(|| "parse GBNF grammar")?;
        // `Fsm<'g>` borrows from `Grammar` so we bind the owner to the
        // same scope
        let boxed = Box::new(grammar);
        let boxed_ref: &'static _ = Box::leak(boxed);
        let fsm = Fsm::start(boxed_ref)
            .map_err(|e| anyhow!("Fsm::start failed: {e:?}"))?
            .with_max_depth(4096);
        (Some(fsm), Some(boxed_ref))
    } else {
        (None, None)
    };
    // Suppress unused warning — the leak is intentional and the owner
    // pointer lives with the FSM
    let _ = grammar_owner;

    let mut tokens = tokenizer.encode(prompt);
    if tokenizer.add_bos_token && (tokens.is_empty() || tokens[0] != tokenizer.bos_id) {
        tokens.insert(0, tokenizer.bos_id);
    }
    if tokens.is_empty() {
        return Err(anyhow!("empty prompt after tokenisation"));
    }

    model.clear();
    for &tok in &tokens[..tokens.len() - 1] {
        model.forward_read(tok);
    }
    let last = *tokens.last().expect("non-empty tokens");
    let mut logits = model.forward_read(last);

    let max_tokens = params.max_tokens as usize;
    let temperature = params.temperature;
    let top_k = params.top_k;

    let mut generated: Vec<u32> = Vec::with_capacity(max_tokens);
    let mut rng_state: u64 = 0x_DEAD_BEEF_CAFE_1234;
    let mut next_rand = || -> f32 {
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
        (rng_state as f32) / (u64::MAX as f32)
    };

    for _ in 0..max_tokens {
        if let Some(ref f) = fsm {
            mask_logits_by_grammar(f, tokenizer, &mut logits);
        }

        let next = if temperature < 1e-6 {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                sample_argmax(&logits) as u32
            }
        } else {
            apply_temperature(&mut logits, temperature);
            top_k_filter(&mut logits, top_k);
            softmax_inplace(&mut logits);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                sample_with_random(&logits, next_rand()) as u32
            }
        };

        if next == tokenizer.eos_id {
            break;
        }

        if let Some(ref mut f) = fsm {
            advance_fsm_on_emit(f, tokenizer, next)
                .map_err(|e| anyhow!("Fsm::advance diverged from mask at token {next}: {e:?}"))?;
        }
        generated.push(next);

        if let Some(ref f) = fsm
            && f.is_final()
        {
            break;
        }
        logits = model.forward_read(next);
    }

    Ok(tokenizer.decode(&generated))
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
            grammar: None,
        };
        let out = backend
            .generate("You are ALICE.", "Say 'sphere(1)' verbatim.", &params)
            .await
            .expect("generate");
        assert!(!out.is_empty(), "embedded generation should produce text");
    }
}
