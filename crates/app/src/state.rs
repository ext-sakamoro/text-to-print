use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use text_to_print_core::db::{Database, GenerationRow};
use text_to_print_core::pipeline::MeshStats;
use text_to_print_core::tier::Tier;
use text_to_print_llm::backend::LlmConfig;
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
    /// LoRA share opt-in flag (Stage 5 T5.2) When `true` (default) the
    /// LOL DSL + quality signals are queued for upload to the shared LoRA
    /// training set; when `false` the user has opted out
    pub share_lol_dsl: bool,
}

pub enum GenerationStatus {
    Idle,
    Generating,
    Done {
        lol_source: String,
        mesh_stats: Option<MeshStats>,
    },
    Error(String),
}

pub enum GenerationMessage {
    PhaseStart(GenerationPhase),
    PhaseDone(GenerationPhase, Duration),
    Success {
        id: String,
        lol_source: String,
        mesh_stats: Option<MeshStats>,
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
            share_lol_dsl,
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
