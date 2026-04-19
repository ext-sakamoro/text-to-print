use std::path::PathBuf;
use std::sync::mpsc;

use tdvbgaran_core::db::{Database, GenerationRow};
use tdvbgaran_core::pipeline::MeshStats;
use tdvbgaran_core::tier::Tier;
use tdvbgaran_llm::backend::LlmConfig;

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
    pub model_progress: tokio::sync::watch::Receiver<tdvbgaran_llm::downloader::DownloadProgress>,
    pub model_ready: bool,
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
        let db_path = data_dir.join("3dvbgaran.db");
        let db = Database::open(&db_path).expect("failed to open database");

        let profile_id = load_or_create_profile_id(&data_dir);
        let tier = db.get_or_create_profile(&profile_id).unwrap_or(Tier::Free);

        let history = db.list_generations(&profile_id, 50).unwrap_or_default();

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("failed to create tokio runtime");

        let (result_tx, result_rx) = mpsc::channel();

        // モデルチェック + バックグラウンドダウンロード
        let models_dir = data_dir.join("models");
        let model_ready = tdvbgaran_llm::downloader::model_exists(&models_dir);
        let (progress_tx, progress_rx) = tokio::sync::watch::channel(
            tdvbgaran_llm::downloader::DownloadProgress {
                downloaded_bytes: 0,
                total_bytes: None,
                status: if model_ready {
                    tdvbgaran_llm::downloader::DownloadStatus::Complete
                } else {
                    tdvbgaran_llm::downloader::DownloadStatus::Pending
                },
            },
        );

        if !model_ready {
            let md = models_dir.clone();
            runtime.spawn(async move {
                if let Err(e) = tdvbgaran_llm::downloader::download_model(&md, progress_tx).await {
                    tracing::error!(error = %e, "model download failed");
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
