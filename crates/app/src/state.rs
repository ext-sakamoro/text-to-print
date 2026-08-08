use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use text_to_print_core::db::{Database, GenerationRow};
use text_to_print_core::pipeline::MeshStats;
use text_to_print_core::tier::Tier;
use text_to_print_llm::backend::LlmConfig;
use text_to_print_llm::backend_kind::{BackendKind, EmbeddedStatus, ExecutionMode};
use text_to_print_llm::embedded_backend::EmbeddedBackend;
use text_to_print_llm::sidecar::SidecarStatus;

/// Ordered pipeline phases surfaced to the UI progress indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerationPhase {
    Llm,
    Parse,
    Mesh,
    Safety,
    Export,
}

impl GenerationPhase {
    pub const ALL: [Self; 5] = [
        Self::Llm,
        Self::Parse,
        Self::Mesh,
        Self::Safety,
        Self::Export,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Llm => "LLM",
            Self::Parse => "parse",
            Self::Mesh => "mesh",
            Self::Safety => "safety",
            Self::Export => "export",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PhaseProgress {
    pub current: Option<GenerationPhase>,
    /// Latency of each completed phase, in the order they completed.
    pub completed: Vec<(GenerationPhase, Duration)>,
    /// Retry count for the LLM phase (surfaced by Stage 8 backend when wired).
    pub retry_count: u32,
    /// Wall-clock instant when the current generation started
    ///
    /// Populated by `prompt.rs` right before the LLM inference request is
    /// dispatched, cleared on `reset()` The UI reads this to show a running
    /// elapsed-time counter — otherwise the progress bar sits at 0% for the
    /// entire LLM phase (which dominates the wall-clock time) and users
    /// cannot tell whether the app is stuck or working
    pub generation_start: Option<std::time::Instant>,
}

impl PhaseProgress {
    pub fn reset(&mut self) {
        self.current = None;
        self.completed.clear();
        self.retry_count = 0;
        self.generation_start = None;
    }

    pub fn is_done(&self, phase: GenerationPhase) -> bool {
        self.completed.iter().any(|(p, _)| *p == phase)
    }

    pub fn latency_of(&self, phase: GenerationPhase) -> Option<Duration> {
        self.completed
            .iter()
            .find(|(p, _)| *p == phase)
            .map(|(_, d)| *d)
    }

    /// Elapsed time since `generation_start` was set — returns `None` when
    /// no generation is currently in flight
    pub fn elapsed(&self) -> Option<Duration> {
        self.generation_start.map(|t| t.elapsed())
    }
}

pub struct AppState {
    #[allow(dead_code)]
    pub data_dir: PathBuf,
    pub tier: Tier,
    pub llm_config: LlmConfig,
    pub prompt_input: String,
    pub generation_status: GenerationStatus,
    pub current_lol: Option<String>,
    /// Latest mesh built by the pipeline, ready for the on-screen preview
    ///
    /// Populated in `prompt.rs` right after `pipeline::export_mesh` returns
    /// success The `Arc` lets the mesh cross the async task boundary
    /// without copying vertices
    pub viewer_mesh: Option<std::sync::Arc<alice_sdf::mesh::Mesh>>,
    /// Monotonic version counter that increments each time `viewer_mesh` is
    /// replaced The mesh preview UI compares this against its own
    /// last-uploaded version to decide whether to re-push vertices to the
    /// GPU
    pub mesh_version: u64,
    /// General tier: 公開待ちの SDF (id, lol_source, prompt)
    pub pending_publish: Option<(String, String, String)>,
    pub history: Vec<GenerationRow>,
    pub db: Database,
    pub profile_id: String,
    pub runtime: tokio::runtime::Runtime,
    pub result_rx: mpsc::Receiver<GenerationMessage>,
    pub result_tx: mpsc::Sender<GenerationMessage>,
    pub model_progress:
        tokio::sync::watch::Receiver<text_to_print_llm::downloader::DownloadProgress>,
    pub model_ready: bool,
    pub phase_progress: PhaseProgress,
    /// Set true once egui focus has been requested for the prompt field.
    pub prompt_focused_once: bool,
    /// `alice-llm-server` sidecar プロセスの状態
    ///
    /// - `Waiting`: model DL 完了待ち
    /// - `Starting`: `alice-llm-server` を spawn 発行済、`/health` 応答待ち
    /// - `Running`: 推論 request 受付可
    /// - `Error(msg)`: バイナリ不在 / model 不在 / health timeout 等
    ///
    /// spawn task 本体は `AppState::new` の runtime 上で動作し、
    /// `SidecarProcess` を task スコープに保持することで runtime drop 時に
    /// 自動 kill される
    pub sidecar_status: tokio::sync::watch::Receiver<SidecarStatus>,
    /// Stage 3-C.6: user-selected inference backend (Sidecar HTTP or
    /// in-process Embedded) Persisted in DB (`profiles.backend_kind`)
    /// The default is `Sidecar` so existing installs keep the pre-3-C
    /// behaviour without a migration step
    pub backend_kind: BackendKind,
    /// Stage 3-C.12: whether the Embedded backend runs on CPU or GPU
    /// Only meaningful when [`Self::backend_kind`] is
    /// [`BackendKind::Embedded`] Persisted in DB
    /// (`profiles.execution_mode`) Default `Cpu`
    pub execution_mode: ExecutionMode,
    /// Loaded [`EmbeddedBackend`] wrapped for shared access The slot is
    /// `None` until the user opts into Embedded and the background load
    /// task populates it via [`AppState::switch_backend_kind`]
    pub embedded: std::sync::Arc<std::sync::Mutex<Option<EmbeddedBackend>>>,
    /// UI-facing load status Read via [`AppState::embedded_status`] The
    /// underlying `Arc<Mutex<EmbeddedStatus>>` is written by the load
    /// task in [`AppState::switch_backend_kind`]
    pub embedded_status_cell: std::sync::Arc<std::sync::Mutex<EmbeddedStatus>>,
    /// LoRA share opt-in flag (Stage 5 T5.2) When `true` (default) the
    /// LOL DSL + quality signals are queued for upload to the shared LoRA
    /// training set; when `false` the user has opted out
    pub share_lol_dsl: bool,
    /// Stage 3-C.14: when `true` (default), every generation forwards
    /// [`text_to_print_llm::grammar_lol::LOL_GBNF`] to the backend so
    /// output is guaranteed to be parseable by
    /// `alice_bamboo::parse_lol` Turn off for debugging free-form output
    pub enforce_lol_grammar: bool,
    /// Most recent share-payload dry-run path (GAP-12) Populated when the
    /// generation success path serialises a `SharePayload` to
    /// `data_dir/share_dry_run/{uuid}.json` The real Cloudflare Workers
    /// upload (Epic-Infra #35) will consume the same payload; the dry-run
    /// path keeps the opt-in gate honest even when the backend is offline
    pub pending_share_dry_run: Option<std::path::PathBuf>,
}

pub enum GenerationStatus {
    Idle,
    Generating,
    Done {
        lol_source: String,
        // Boxed because `MeshStats` is ~200 B and this variant is the largest
        // enum arm by an order of magnitude (`clippy::large_enum_variant`)
        mesh_stats: Option<Box<MeshStats>>,
    },
    Error(String),
}

pub enum GenerationMessage {
    PhaseStart(GenerationPhase),
    PhaseDone(GenerationPhase, Duration),
    Success {
        id: String,
        lol_source: String,
        /// Boxed for the same reason as [`GenerationStatus::Done::mesh_stats`]
        mesh_stats: Option<Box<MeshStats>>,
        /// Number of LLM retries performed during this generation Non-zero
        /// values mean the initial LOL DSL raised safety violations and the
        /// [`text_to_print_llm::backend::generate_with_retry`] loop
        /// requested a revised DSL via `fix_prompt` (Stage 8 T8.2)
        retry_count: u32,
        /// Path to the dry-run share payload JSON (GAP-12) Populated when
        /// `share_lol_dsl` is on and the generation produced a 3MF export;
        /// otherwise `None`
        share_dry_run: Option<std::path::PathBuf>,
    },
    Failure {
        id: String,
        error: String,
    },
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Self {
        let db_path = data_dir.join("text-to-print.db");
        let db = Database::open(&db_path).expect("failed to open database");

        let profile_id = load_or_create_profile_id(&data_dir);
        let tier = db.get_or_create_profile(&profile_id).unwrap_or(Tier::Free);
        let share_lol_dsl = db.get_share_lol_dsl(&profile_id).unwrap_or(true);
        // v0.1.0-beta.1 (2026-08-07): default を Sidecar → Embedded に変更
        // sidecar は alice-llm-server binary の別途 install を必要とする
        // (release.yml は bundle 済だが local `cargo run` では欠落) →
        // 初回起動 UX が壊れる Embedded は alice-llm を rlib 直リンクなので
        // binary 追加なしで動く GGUF DL は既存 downloader flow で自動化済
        let backend_kind = BackendKind::from_db_str(
            &db.get_backend_kind(&profile_id)
                .unwrap_or_else(|_| "Embedded".to_string()),
        );
        let execution_mode = ExecutionMode::from_db_str(
            &db.get_execution_mode(&profile_id)
                .unwrap_or_else(|_| "Cpu".to_string()),
        );
        // v0.1.0-beta.1 (2026-08-07): default を true → false に変更
        // (詳細は db.rs::get_enforce_lol_grammar コメント参照)
        let enforce_lol_grammar = db.get_enforce_lol_grammar(&profile_id).unwrap_or(false);

        let history = db.list_generations(&profile_id, 50).unwrap_or_default();

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("failed to create tokio runtime");

        let (result_tx, result_rx) = mpsc::channel();

        // モデルチェック + バックグラウンドダウンロード
        // Stage 3-C.9: model_choice に対応する GGUF を保持 起動時は
        // LlmConfig::default() (= Qwen 3.5-4B) を採用、UI で dropdown 変更
        // → Embedded 有効時は on_model_choice_changed が呼ばれ切替
        let models_dir = data_dir.join("models");
        let initial_choice = LlmConfig::default().model_choice;
        let model_ready = text_to_print_llm::downloader::model_exists(&models_dir, initial_choice);
        let initial_status = if model_ready {
            text_to_print_llm::downloader::DownloadStatus::Complete
        } else {
            text_to_print_llm::downloader::DownloadStatus::Pending
        };
        let (progress_tx, progress_rx) =
            tokio::sync::watch::channel(text_to_print_llm::downloader::DownloadProgress {
                downloaded_bytes: 0,
                total_bytes: None,
                status: initial_status,
            });

        if !model_ready {
            let md = models_dir.clone();
            runtime.spawn(async move {
                if let Err(e) =
                    text_to_print_llm::downloader::download_model(&md, initial_choice, progress_tx)
                        .await
                {
                    tracing::error!(error = %e, "model download failed");
                }
            });
        }

        // sidecar auto-spawn: model DL 完了を待ってから `alice-llm-server` を起動
        // 2026-08-07: preferred port 8000 は user 環境で Python HTTP server 等が
        // 良く塞ぐため、事前 free port scan 済 chosen port は LlmConfig の
        // endpoint も同期更新 client → sidecar 疎通が確実になる
        let (sidecar_tx, sidecar_rx) = tokio::sync::watch::channel(SidecarStatus::Waiting);
        let default_llm_config = LlmConfig::default();
        let preferred_port = default_sidecar_port(&default_llm_config.endpoint);
        let chosen_port =
            text_to_print_llm::sidecar::find_free_port_starting_at(preferred_port, 16)
                .unwrap_or(preferred_port);
        let llm_config = if chosen_port == preferred_port {
            default_llm_config
        } else {
            tracing::info!(
                preferred = preferred_port,
                chosen = chosen_port,
                "sidecar preferred port in use, endpoint updated"
            );
            LlmConfig {
                endpoint: format!("http://localhost:{chosen_port}/v1/chat/completions"),
                ..default_llm_config
            }
        };
        {
            let models_dir_for_sidecar = models_dir.clone();
            let mut model_progress_rx = progress_rx.clone();
            runtime.spawn(async move {
                // model DL の完了を待つ (model_ready なら DownloadStatus::Complete で初期化済)
                while !matches!(
                    model_progress_rx.borrow().status,
                    text_to_print_llm::downloader::DownloadStatus::Complete
                ) {
                    if model_progress_rx.changed().await.is_err() {
                        // sender drop = AppState 破棄、task 終了
                        return;
                    }
                }
                let model_path = text_to_print_llm::downloader::model_path(
                    &models_dir_for_sidecar,
                    initial_choice,
                );
                text_to_print_llm::sidecar::run_auto_spawn(model_path, chosen_port, sidecar_tx)
                    .await;
            });
        }
        // Sidecar warm-up: healthy 検知後に background で dummy 1-token
        // request 送出 Metal shader compile + buffer alloc が初回のみ
        // ~50-100s 走る問題を、user が prompt 打ち込む前に済ませておく
        // fire-and-forget、失敗は log のみ (実 request 時に retry される)
        {
            let mut sidecar_rx_for_warmup = sidecar_rx.clone();
            let endpoint = llm_config.endpoint.clone();
            let model_id = llm_config.model_choice.model_id().to_string();
            runtime.spawn(async move {
                // Running になるまで待つ
                loop {
                    if sidecar_rx_for_warmup.borrow().is_running() {
                        break;
                    }
                    if sidecar_rx_for_warmup.changed().await.is_err() {
                        return;
                    }
                }
                tracing::info!("sidecar warm-up starting (background dummy request)");
                let started = std::time::Instant::now();
                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(300))
                    .build()
                    .ok();
                if let Some(client) = client {
                    // 現実的な payload で warm-up: max_tokens=1 だと Metal
                    // shader の一部 pipeline が compile されず初回本番 request
                    // で遅延する (user 実測 warm-up 2s 後の本 request 155s)
                    // 対策: 実 LOL DSL 生成に近い payload (short prompt +
                    // max_tokens 50) で warm-up、shader の generation path
                    // まで compile 済にする
                    let body = serde_json::json!({
                        "model": model_id,
                        "messages": [{
                            "role": "user",
                            "content": "Output a 10mm sphere in LOL DSL: sphere(5)"
                        }],
                        "max_tokens": 50,
                        "temperature": 0.7,
                    });
                    match client.post(&endpoint).json(&body).send().await {
                        Ok(resp) => tracing::info!(
                            elapsed_ms = started.elapsed().as_millis() as u64,
                            status = %resp.status(),
                            "sidecar warm-up complete (realistic payload)"
                        ),
                        Err(e) => tracing::warn!("sidecar warm-up failed: {e}"),
                    }
                }
            });
        }

        // Stage 5: real share upload sweep — retry any queued payloads
        // from prior sessions Delayed 5s to let the sidecar + UI settle
        // before hitting the network Runs once per launch; the offline
        // queue keeps payloads for 24 h (`QUEUE_TTL`) so a subsequent
        // launch drains anything that didn't land this time
        {
            let queue_dir = data_dir.join("share_queue");
            runtime.spawn(async move {
                tokio::time::sleep(Duration::from_secs(5)).await;
                let cfg = text_to_print_network::share::ShareConfig::default();
                match text_to_print_network::share::retry_queued_uploads(&cfg, &queue_dir).await {
                    Ok(summary) => tracing::info!(
                        delivered = summary.delivered,
                        still_pending = summary.still_pending,
                        rejected = summary.rejected_permanently,
                        expired = summary.expired,
                        "share queue sweep complete"
                    ),
                    Err(e) => tracing::warn!(error = %e, "share queue sweep failed"),
                }
            });
        }

        // Stage 3-C.6: initial embedded slot Empty until the user opts
        // into Embedded via the settings UI Kicking off the load here
        // (even when backend_kind == Embedded from DB) would block app
        // startup for tens of seconds; instead we surface the toggle in
        // Settings and let the load begin only when the user asks
        let embedded = std::sync::Arc::new(std::sync::Mutex::new(None));
        let embedded_status_cell =
            std::sync::Arc::new(std::sync::Mutex::new(EmbeddedStatus::NotLoaded));

        // If the persisted preference is Embedded, prime the load in the
        // background so the first generation doesn't pay the ~30 s load
        // cost synchronously The sidecar remains available in the mean
        // time as a fallback
        if backend_kind == BackendKind::Embedded {
            spawn_embedded_load(
                &runtime,
                models_dir.clone(),
                initial_choice,
                execution_mode,
                embedded.clone(),
                embedded_status_cell.clone(),
                progress_rx.clone(),
            );
        }

        Self {
            data_dir,
            tier,
            llm_config,
            prompt_input: String::new(),
            generation_status: GenerationStatus::Idle,
            current_lol: None,
            viewer_mesh: None,
            mesh_version: 0,
            pending_publish: None,
            history,
            db,
            profile_id,
            runtime,
            result_rx,
            result_tx,
            model_progress: progress_rx,
            model_ready,
            phase_progress: PhaseProgress::default(),
            prompt_focused_once: false,
            sidecar_status: sidecar_rx,
            backend_kind,
            execution_mode,
            embedded,
            embedded_status_cell,
            share_lol_dsl,
            enforce_lol_grammar,
            pending_share_dry_run: None,
        }
    }

    pub fn refresh_history(&mut self) {
        self.history = self
            .db
            .list_generations(&self.profile_id, 50)
            .unwrap_or_default();
    }

    pub fn today(&self) -> String {
        chrono::Local::now().format("%Y-%m-%d").to_string()
    }

    pub fn daily_usage(&self) -> u32 {
        self.db
            .get_daily_usage(&self.profile_id, &self.today())
            .unwrap_or(0)
    }

    pub fn can_generate(&self) -> bool {
        self.daily_usage() < self.tier.limits().daily_generations
    }

    /// Directory used by the share dry-run gate (GAP-12) The generation
    /// success path writes `SharePayload` JSON blobs here whenever the
    /// user has opted in and produced a mesh; the Cloudflare Workers
    /// backend (Epic-Infra #35) will drain the queue later
    pub fn share_dry_run_dir(&self) -> std::path::PathBuf {
        self.data_dir.join("share_dry_run")
    }

    /// Directory used by the real upload queue (Stage 5) `enqueue` writes
    /// `SharePayload` JSON blobs here that `retry_queued_uploads` drains
    /// on app startup / periodic sweep against the Cloudflare Worker
    /// endpoint Distinct from `share_dry_run_dir` so operators can inspect
    /// the dry-run corpus without touching live queue state
    pub fn share_queue_dir(&self) -> std::path::PathBuf {
        self.data_dir.join("share_queue")
    }

    /// Effective share flag Combines the user's opt-in toggle with the
    /// tier gate: paid tiers (General / Pro / Enterprise) never share
    /// regardless of the toggle Free tier respects the toggle
    ///
    /// This is the single value the generation path should consult before
    /// dumping / enqueueing a `SharePayload` The settings UI can still
    /// surface the raw toggle to communicate opt-in state
    pub fn share_effective_enabled(&self) -> bool {
        match self.tier {
            Tier::Free => self.share_lol_dsl,
            Tier::General | Tier::Pro | Tier::Enterprise => false,
        }
    }

    /// Snapshot the current [`EmbeddedStatus`] for UI display Cheap `Clone`
    /// under a `std::sync::Mutex` so egui's sync render path can call it
    #[must_use]
    pub fn embedded_status(&self) -> EmbeddedStatus {
        self.embedded_status_cell
            .lock()
            .map(|g| g.clone())
            .unwrap_or(EmbeddedStatus::NotLoaded)
    }

    /// Persist a new [`BackendKind`] and, if switching to Embedded, kick
    /// off a background load task Idempotent on the same kind
    pub fn switch_backend_kind(&mut self, new_kind: BackendKind) {
        if self.backend_kind == new_kind {
            return;
        }
        self.backend_kind = new_kind;
        if let Err(e) = self
            .db
            .set_backend_kind(&self.profile_id, new_kind.to_db_str())
        {
            tracing::warn!(error = %e, "failed to persist backend_kind");
        }
        if new_kind == BackendKind::Embedded {
            let already_loaded = self.embedded.lock().map(|g| g.is_some()).unwrap_or(false);
            if already_loaded {
                return;
            }
            spawn_embedded_load(
                &self.runtime,
                self.data_dir.join("models"),
                self.llm_config.model_choice,
                self.execution_mode,
                self.embedded.clone(),
                self.embedded_status_cell.clone(),
                self.model_progress.clone(),
            );
        }
    }

    /// Stage 3-C.12: change the Embedded execution mode (CPU ↔ GPU) at
    /// runtime If Embedded is currently active, drops the loaded model
    /// and re-loads under the new mode
    pub fn switch_execution_mode(&mut self, new_mode: ExecutionMode) {
        if self.execution_mode == new_mode {
            return;
        }
        self.execution_mode = new_mode;
        if let Err(e) = self
            .db
            .set_execution_mode(&self.profile_id, new_mode.to_db_str())
        {
            tracing::warn!(error = %e, "failed to persist execution_mode");
        }
        // Only reload if Embedded is the active backend Sidecar path
        // ignores execution_mode entirely (server subprocess decides its
        // own CPU / GPU via its own --hybrid flag)
        if self.backend_kind != BackendKind::Embedded {
            return;
        }
        if let Ok(mut slot) = self.embedded.lock() {
            *slot = None;
        }
        if let Ok(mut status) = self.embedded_status_cell.lock() {
            *status = EmbeddedStatus::NotLoaded;
        }
        spawn_embedded_load(
            &self.runtime,
            self.data_dir.join("models"),
            self.llm_config.model_choice,
            new_mode,
            self.embedded.clone(),
            self.embedded_status_cell.clone(),
            self.model_progress.clone(),
        );
    }

    /// Stage 3-C.9: handle a `ModelChoice` change from the Settings UI
    /// Called after the ComboBox has already updated
    /// `self.llm_config.model_choice` to the newly selected value
    ///
    /// If Embedded is active, drop the currently loaded backend and kick
    /// off a load for the new choice The Sidecar path is unaffected — the
    /// alice-llm-server subprocess reads its `--model` arg once at spawn
    /// time and would need a full sidecar restart to swap models (out of
    /// scope for this hook)
    ///
    /// Stage 3-C.13: If the currently loaded backend already tracks the
    /// new choice (e.g. the user flipped through the dropdown and landed
    /// back on the loaded choice), skip the drop + reload cycle — a
    /// wasted ~30 s and duplicate memory pressure
    pub fn on_model_choice_changed(&mut self, prev_choice: text_to_print_llm::model::ModelChoice) {
        let new_choice = self.llm_config.model_choice;
        if prev_choice == new_choice {
            return;
        }
        if self.backend_kind != BackendKind::Embedded {
            return;
        }
        let already_loaded = self
            .embedded
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|b| b.loaded_choice()))
            == Some(new_choice);
        if already_loaded {
            tracing::info!(
                ?new_choice,
                "embedded backend already loaded with target choice, skipping swap"
            );
            return;
        }
        tracing::info!(
            prev = ?prev_choice,
            new = ?new_choice,
            "embedded backend swap on model choice change"
        );
        // Drop the currently loaded backend so the worker thread's stack
        // unwinds and mmap / gguf / model release their memory before we
        // begin loading the new file (avoids a 2× resident-set spike)
        if let Ok(mut slot) = self.embedded.lock() {
            *slot = None;
        }
        if let Ok(mut status) = self.embedded_status_cell.lock() {
            *status = EmbeddedStatus::NotLoaded;
        }
        spawn_embedded_load(
            &self.runtime,
            self.data_dir.join("models"),
            new_choice,
            self.execution_mode,
            self.embedded.clone(),
            self.embedded_status_cell.clone(),
            self.model_progress.clone(),
        );
    }

    /// Snapshot the currently-active backend for a generation request
    ///
    /// - `BackendKind::Sidecar` → wraps the current `LlmConfig`
    /// - `BackendKind::Embedded` → clones the loaded backend if `Ready`,
    ///   otherwise falls back to `Sidecar` (so a generation request
    ///   during Embedded load doesn't fail silently)
    #[must_use]
    pub fn active_backend(&self) -> text_to_print_llm::backend_kind::LlmBackend {
        use text_to_print_llm::backend_kind::LlmBackend;
        match self.backend_kind {
            BackendKind::Sidecar => LlmBackend::Sidecar(self.llm_config.clone()),
            BackendKind::Embedded => {
                let backend = self.embedded.lock().ok().and_then(|g| g.clone());
                match backend {
                    Some(b) => LlmBackend::Embedded(b),
                    None => LlmBackend::Sidecar(self.llm_config.clone()),
                }
            }
        }
    }
}

/// `http://host:PORT/...` の PORT を抽出、失敗時は 8000
fn default_sidecar_port(endpoint: &str) -> u16 {
    endpoint
        .split("://")
        .nth(1)
        .and_then(|s| s.split('/').next())
        .and_then(|host_port| host_port.rsplit(':').next())
        .and_then(|p| p.parse().ok())
        .unwrap_or(8000)
}

fn load_or_create_profile_id(data_dir: &std::path::Path) -> String {
    let id_path = data_dir.join("profile_id");
    if let Ok(id) = std::fs::read_to_string(&id_path) {
        let id = id.trim().to_string();
        if !id.is_empty() {
            return id;
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    let _ = std::fs::write(&id_path, &id);
    id
}

/// Stage 3-C.9: spawn a background task that waits for model DL to
/// finish, then loads the specified `ModelChoice` into `embedded_slot`
/// and reflects state through `status_slot`
///
/// Extracted so both the app startup path (when the persisted
/// `backend_kind` is Embedded) and the runtime switch paths
/// (`switch_backend_kind` / `on_model_choice_changed`) share the same
/// wait-for-DL-then-load flow
fn spawn_embedded_load(
    runtime: &tokio::runtime::Runtime,
    models_dir: PathBuf,
    choice: text_to_print_llm::model::ModelChoice,
    execution_mode: ExecutionMode,
    embedded_slot: std::sync::Arc<std::sync::Mutex<Option<EmbeddedBackend>>>,
    status_slot: std::sync::Arc<std::sync::Mutex<EmbeddedStatus>>,
    mut model_progress_rx: tokio::sync::watch::Receiver<
        text_to_print_llm::downloader::DownloadProgress,
    >,
) {
    runtime.spawn(async move {
        // Wait for model DL to complete Choice-agnostic here — the DL
        // pipeline only knows about the initial choice; runtime-switched
        // choices are expected to be user-placed at
        // `models_dir/{choice.default_filename()}` If missing, load fails
        // fast with an IO error which surfaces as EmbeddedStatus::Error
        while !matches!(
            model_progress_rx.borrow().status,
            text_to_print_llm::downloader::DownloadStatus::Complete
        ) {
            if model_progress_rx.changed().await.is_err() {
                return;
            }
            // Once initial DL completes we still proceed even if the user
            // switched to a different choice — the load path will
            // discover whether the file exists on disk
            if text_to_print_llm::downloader::model_exists(&models_dir, choice) {
                break;
            }
        }
        let model_path = text_to_print_llm::downloader::model_path(&models_dir, choice);
        *status_slot.lock().expect("embedded status lock") = EmbeddedStatus::Loading;
        let mp_first = model_path.clone();
        let load_result = tokio::task::spawn_blocking(move || {
            EmbeddedBackend::load_full(&mp_first, choice, execution_mode)
        })
        .await;
        match load_result {
            Ok(Ok(backend)) => {
                *embedded_slot.lock().expect("embedded slot lock") = Some(backend);
                *status_slot.lock().expect("embedded status lock") = EmbeddedStatus::Ready;
                tracing::info!(?choice, ?execution_mode, "embedded backend loaded");
            }
            Ok(Err(e)) => {
                // Stage 3-C.15: GPU load failure → automatic CPU
                // fallback so users on hostless-GPU systems (or with
                // insufficient VRAM) still get a working Embedded path
                // Surfaces as `EmbeddedStatus::Ready` with a fallback
                // log line rather than `Error(...)` so the UI doesn't
                // look like the toggle was rejected
                if execution_mode == ExecutionMode::Gpu {
                    tracing::warn!(
                        error = %e,
                        ?choice,
                        "GPU load failed, falling back to CPU"
                    );
                    let mp_cpu = model_path.clone();
                    let cpu_result = tokio::task::spawn_blocking(move || {
                        EmbeddedBackend::load_full(&mp_cpu, choice, ExecutionMode::Cpu)
                    })
                    .await;
                    match cpu_result {
                        Ok(Ok(backend)) => {
                            *embedded_slot.lock().expect("embedded slot lock") = Some(backend);
                            *status_slot.lock().expect("embedded status lock") =
                                EmbeddedStatus::Ready;
                            tracing::info!(
                                ?choice,
                                "embedded backend loaded (CPU fallback after GPU failure)"
                            );
                            return;
                        }
                        Ok(Err(e_cpu)) => {
                            *status_slot.lock().expect("embedded status lock") =
                                EmbeddedStatus::Error(format!("GPU: {e} / CPU fallback: {e_cpu}"));
                            tracing::warn!(
                                error = %e_cpu,
                                ?choice,
                                "CPU fallback also failed"
                            );
                            return;
                        }
                        Err(join_err) => {
                            *status_slot.lock().expect("embedded status lock") =
                                EmbeddedStatus::Error(format!(
                                    "GPU: {e} / CPU fallback task panic: {join_err}"
                                ));
                            return;
                        }
                    }
                }
                *status_slot.lock().expect("embedded status lock") =
                    EmbeddedStatus::Error(e.to_string());
                tracing::warn!(
                    error = %e,
                    ?choice,
                    ?execution_mode,
                    "embedded backend load failed"
                );
            }
            Err(join_err) => {
                *status_slot.lock().expect("embedded status lock") =
                    EmbeddedStatus::Error(join_err.to_string());
                tracing::warn!(
                    error = %join_err,
                    ?choice,
                    ?execution_mode,
                    "embedded backend load task panicked"
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{GenerationPhase, PhaseProgress, default_sidecar_port};
    use std::time::Duration;

    #[test]
    fn port_from_localhost_endpoint() {
        assert_eq!(
            default_sidecar_port("http://localhost:8000/v1/chat/completions"),
            8000
        );
        assert_eq!(
            default_sidecar_port("http://127.0.0.1:12345/v1/chat/completions"),
            12345
        );
    }

    #[test]
    fn port_defaults_when_missing() {
        assert_eq!(default_sidecar_port("http://localhost/v1/x"), 8000);
        assert_eq!(default_sidecar_port(""), 8000);
        assert_eq!(default_sidecar_port("garbage"), 8000);
    }

    // ────────────────────────────────────────────────────────
    // Generation phase state-machine tests
    //
    // UI 側で表示する 5 phase の進捗 (Llm → Parse → Mesh → Safety → Export)
    // を pure state (egui / DB / tokio 非依存) で verify する 実 pipeline は
    // `crates/core/tests/e2e_pipeline.rs` で E2E 通過を確認しているため、
    // ここは phase transition の invariant (順序 / 冪等 / reset) のみ担保
    // ────────────────────────────────────────────────────────

    #[test]
    fn phase_all_covers_five_phases_in_order() {
        assert_eq!(GenerationPhase::ALL.len(), 5);
        assert_eq!(GenerationPhase::ALL[0], GenerationPhase::Llm);
        assert_eq!(GenerationPhase::ALL[1], GenerationPhase::Parse);
        assert_eq!(GenerationPhase::ALL[2], GenerationPhase::Mesh);
        assert_eq!(GenerationPhase::ALL[3], GenerationPhase::Safety);
        assert_eq!(GenerationPhase::ALL[4], GenerationPhase::Export);
    }

    #[test]
    fn phase_label_stable_for_ui() {
        assert_eq!(GenerationPhase::Llm.label(), "LLM");
        assert_eq!(GenerationPhase::Parse.label(), "parse");
        assert_eq!(GenerationPhase::Mesh.label(), "mesh");
        assert_eq!(GenerationPhase::Safety.label(), "safety");
        assert_eq!(GenerationPhase::Export.label(), "export");
    }

    #[test]
    fn phase_progress_default_is_empty() {
        let p = PhaseProgress::default();
        assert!(p.current.is_none());
        assert!(p.completed.is_empty());
        assert_eq!(p.retry_count, 0);
        for phase in GenerationPhase::ALL {
            assert!(
                !p.is_done(phase),
                "{} should not be done initially",
                phase.label()
            );
            assert!(p.latency_of(phase).is_none());
        }
    }

    #[test]
    fn phase_progress_marks_completed_phase() {
        let mut p = PhaseProgress {
            current: Some(GenerationPhase::Llm),
            completed: vec![(GenerationPhase::Llm, Duration::from_millis(120))],
            retry_count: 0,
            generation_start: None,
        };
        p.current = Some(GenerationPhase::Parse);
        assert!(p.is_done(GenerationPhase::Llm));
        assert!(!p.is_done(GenerationPhase::Parse));
        assert_eq!(
            p.latency_of(GenerationPhase::Llm),
            Some(Duration::from_millis(120))
        );
        assert!(p.latency_of(GenerationPhase::Parse).is_none());
    }

    #[test]
    fn phase_progress_full_pipeline_flow() {
        let latencies = [80, 5, 340, 15, 40];
        let completed: Vec<_> = GenerationPhase::ALL
            .iter()
            .zip(latencies)
            .map(|(phase, ms)| (*phase, Duration::from_millis(ms)))
            .collect();
        let p = PhaseProgress {
            current: Some(GenerationPhase::Export),
            completed,
            retry_count: 2,
            generation_start: None,
        };
        for (phase, ms) in GenerationPhase::ALL.iter().zip(latencies) {
            assert!(p.is_done(*phase));
            assert_eq!(p.latency_of(*phase), Some(Duration::from_millis(ms)));
        }
        assert_eq!(p.retry_count, 2);
    }

    #[test]
    fn phase_progress_reset_clears_all_state() {
        let mut p = PhaseProgress {
            current: Some(GenerationPhase::Mesh),
            completed: vec![
                (GenerationPhase::Llm, Duration::from_millis(1)),
                (GenerationPhase::Parse, Duration::from_millis(2)),
            ],
            retry_count: 3,
            generation_start: None,
        };
        p.reset();
        assert!(p.current.is_none());
        assert!(p.completed.is_empty());
        assert_eq!(p.retry_count, 0);
        for phase in GenerationPhase::ALL {
            assert!(!p.is_done(phase));
        }
    }

    #[test]
    fn phase_progress_idempotent_reset() {
        let mut p = PhaseProgress::default();
        p.reset();
        p.reset();
        assert!(p.current.is_none());
        assert!(p.completed.is_empty());
        assert_eq!(p.retry_count, 0);
    }
}
