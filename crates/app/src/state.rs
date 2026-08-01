use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use text_to_print_core::db::{Database, GenerationRow};
use text_to_print_core::pipeline::MeshStats;
use text_to_print_core::tier::Tier;
use text_to_print_llm::backend::LlmConfig;
use text_to_print_llm::backend_kind::{BackendKind, EmbeddedStatus};
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
}

impl PhaseProgress {
    pub fn reset(&mut self) {
        self.current = None;
        self.completed.clear();
        self.retry_count = 0;
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
}

pub struct AppState {
    #[allow(dead_code)]
    pub data_dir: PathBuf,
    pub tier: Tier,
    pub llm_config: LlmConfig,
    pub prompt_input: String,
    pub generation_status: GenerationStatus,
    pub current_lol: Option<String>,
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
        let backend_kind = BackendKind::from_db_str(
            &db.get_backend_kind(&profile_id)
                .unwrap_or_else(|_| "Sidecar".to_string()),
        );

        let history = db.list_generations(&profile_id, 50).unwrap_or_default();

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("failed to create tokio runtime");

        let (result_tx, result_rx) = mpsc::channel();

        // モデルチェック + バックグラウンドダウンロード
        let models_dir = data_dir.join("models");
        let model_ready = text_to_print_llm::downloader::model_exists(&models_dir);
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
                    text_to_print_llm::downloader::download_model(&md, progress_tx).await
                {
                    tracing::error!(error = %e, "model download failed");
                }
            });
        }

        // sidecar auto-spawn: model DL 完了を待ってから `alice-llm-server` を起動
        let (sidecar_tx, sidecar_rx) = tokio::sync::watch::channel(SidecarStatus::Waiting);
        {
            let models_dir_for_sidecar = models_dir.clone();
            let mut model_progress_rx = progress_rx.clone();
            let sidecar_port = default_sidecar_port(&LlmConfig::default().endpoint);
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
                let model_path = text_to_print_llm::downloader::model_path(&models_dir_for_sidecar);
                text_to_print_llm::sidecar::run_auto_spawn(model_path, sidecar_port, sidecar_tx)
                    .await;
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
            let models_dir_for_embed = models_dir.clone();
            let embedded_slot = embedded.clone();
            let status_slot = embedded_status_cell.clone();
            let mut model_progress_rx = progress_rx.clone();
            runtime.spawn(async move {
                while !matches!(
                    model_progress_rx.borrow().status,
                    text_to_print_llm::downloader::DownloadStatus::Complete
                ) {
                    if model_progress_rx.changed().await.is_err() {
                        return;
                    }
                }
                let model_path = text_to_print_llm::downloader::model_path(&models_dir_for_embed);
                *status_slot.lock().expect("embedded status lock") = EmbeddedStatus::Loading;
                let load_result =
                    tokio::task::spawn_blocking(move || EmbeddedBackend::load(&model_path)).await;
                match load_result {
                    Ok(Ok(backend)) => {
                        *embedded_slot.lock().expect("embedded slot lock") = Some(backend);
                        *status_slot.lock().expect("embedded status lock") = EmbeddedStatus::Ready;
                        tracing::info!("embedded backend loaded");
                    }
                    Ok(Err(e)) => {
                        *status_slot.lock().expect("embedded status lock") =
                            EmbeddedStatus::Error(e.to_string());
                        tracing::warn!(error = %e, "embedded backend load failed");
                    }
                    Err(join_err) => {
                        *status_slot.lock().expect("embedded status lock") =
                            EmbeddedStatus::Error(join_err.to_string());
                        tracing::warn!(error = %join_err, "embedded backend load task panicked");
                    }
                }
            });
        }

        Self {
            data_dir,
            tier,
            llm_config: LlmConfig::default(),
            prompt_input: String::new(),
            generation_status: GenerationStatus::Idle,
            current_lol: None,
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
            embedded,
            embedded_status_cell,
            share_lol_dsl,
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
            let models_dir = self.data_dir.join("models");
            let embedded_slot = self.embedded.clone();
            let status_slot = self.embedded_status_cell.clone();
            let mut model_progress_rx = self.model_progress.clone();
            self.runtime.spawn(async move {
                while !matches!(
                    model_progress_rx.borrow().status,
                    text_to_print_llm::downloader::DownloadStatus::Complete
                ) {
                    if model_progress_rx.changed().await.is_err() {
                        return;
                    }
                }
                let model_path = text_to_print_llm::downloader::model_path(&models_dir);
                *status_slot.lock().expect("embedded status lock") = EmbeddedStatus::Loading;
                let load_result =
                    tokio::task::spawn_blocking(move || EmbeddedBackend::load(&model_path)).await;
                match load_result {
                    Ok(Ok(backend)) => {
                        *embedded_slot.lock().expect("embedded slot lock") = Some(backend);
                        *status_slot.lock().expect("embedded status lock") = EmbeddedStatus::Ready;
                        tracing::info!("embedded backend loaded via UI toggle");
                    }
                    Ok(Err(e)) => {
                        *status_slot.lock().expect("embedded status lock") =
                            EmbeddedStatus::Error(e.to_string());
                        tracing::warn!(error = %e, "embedded backend load failed");
                    }
                    Err(join_err) => {
                        *status_slot.lock().expect("embedded status lock") =
                            EmbeddedStatus::Error(join_err.to_string());
                        tracing::warn!(error = %join_err, "embedded backend load task panicked");
                    }
                }
            });
        }
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

#[cfg(test)]
mod tests {
    use super::default_sidecar_port;

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
}
