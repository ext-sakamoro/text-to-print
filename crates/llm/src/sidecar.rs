//! ALICE-LLM sidecar プロセス管理
//!
//! `alice-llm-server` バイナリを子プロセスとして起動し、OpenAI 互換 HTTP endpoint
//! (`http://localhost:<port>/v1/chat/completions`) を提供する
//!
//! 起動フロー:
//!   1. `Command::new(bin) --model <path> --port <N> [--hybrid]` で spawn
//!   2. `/health` endpoint を poll して ready 待ち (最大 60 秒)
//!   3. Drop 時に自動 kill
//!
//! バイナリ配置:
//!   - `sidecar_bin_path` を明示指定 (絶対 path 推奨)
//!   - 未指定時は PATH から `alice-llm-server` を解決
//!   - `cargo install --path ~/ALICE-LLM --features server` でインストール可能
//!
//! app 統合:
//!   - [`SidecarStatus`] を `tokio::sync::watch` channel に流し、GUI で
//!     "起動中 / Running / Error" を可視化する
//!   - app 側は起動時に background task を spawn し、model DL 完了待ち →
//!     [`SidecarProcess::spawn`] → 完了後 `std::future::pending()` で park
//!     runtime drop 時に task cancel → SidecarProcess Drop で `kill_on_drop`

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::{Child, Command};
use tracing::{info, warn};

/// Platform-specific filename for the bundled sidecar binary
#[must_use]
pub const fn sidecar_exe_filename() -> &'static str {
    if cfg!(target_os = "windows") {
        "alice-llm-server.exe"
    } else {
        "alice-llm-server"
    }
}

/// Internal resolver: pure function taking optional exe directory so tests
/// can exercise the priority chain without touching `std::env::current_exe`
fn resolve_bin_path_from(explicit: Option<&Path>, exe_dir: Option<&Path>) -> PathBuf {
    if let Some(p) = explicit {
        return p.to_path_buf();
    }
    if let Some(dir) = exe_dir {
        let candidate = dir.join(sidecar_exe_filename());
        if candidate.exists() {
            return candidate;
        }
    }
    PathBuf::from(sidecar_exe_filename())
}

/// Resolve which `alice-llm-server` binary to spawn
///
/// Priority:
///   1. `config.bin_path` if explicitly set
///   2. Same directory as the current executable (bundled desktop layout)
///   3. Fallback to bare filename so the OS resolves via `PATH`
///
/// The bundled case (2) makes MSI / .deb / .AppImage / .tar.gz installs
/// self-contained no additional install step required for end users
#[must_use]
pub fn resolve_bin_path(config: &SidecarConfig) -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    resolve_bin_path_from(config.bin_path.as_deref(), exe_dir.as_deref())
}

/// GUI に露出する sidecar プロセス状態
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidecarStatus {
    /// model 未 DL 等で起動待ち
    Waiting,
    /// spawn 発行済、`/health` 応答待ち
    Starting,
    /// `/health` 200 応答済、推論 request 受付可
    Running,
    /// spawn 失敗 (バイナリ不在 / model 不在 / health timeout 等)
    Error(String),
}

impl SidecarStatus {
    #[must_use]
    pub const fn is_running(&self) -> bool {
        matches!(self, Self::Running)
    }
}

/// Sidecar 起動設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarConfig {
    /// `alice-llm-server` 実行ファイルの path。`None` の場合は PATH 解決
    pub bin_path: Option<PathBuf>,
    /// GGUF モデル file の path
    pub model_path: PathBuf,
    /// listen port (default 8000)
    pub port: u16,
    /// `--hybrid` flag (CPU forward、Jetson 8GB / GPU 数値誤差回避用)
    pub hybrid: bool,
    /// health check の poll 間隔
    pub health_poll_interval: Duration,
    /// health check の timeout
    pub health_timeout: Duration,
}

impl Default for SidecarConfig {
    fn default() -> Self {
        Self {
            bin_path: None,
            model_path: PathBuf::from("model.gguf"),
            port: 8000,
            hybrid: false,
            health_poll_interval: Duration::from_millis(500),
            // 300s (5 min) — first-time boot needs to page in the ~4.7 GB
            // GGUF via mmap and finish tokenizer parse; 60s was too tight
            // on slow disks and left users with only the timeout message
            health_timeout: Duration::from_secs(300),
        }
    }
}

impl SidecarConfig {
    /// この設定で使う OpenAI 互換 endpoint URL
    #[must_use]
    pub fn chat_endpoint(&self) -> String {
        format!("http://localhost:{}/v1/chat/completions", self.port)
    }

    #[must_use]
    pub fn health_endpoint(&self) -> String {
        format!("http://localhost:{}/health", self.port)
    }
}

/// Rolling buffer of the child's most recent stderr lines Captured for
/// error diagnostics — surfaced when the sidecar dies early or fails
/// its health check so users see the actual reason instead of the
/// generic timeout
type StderrTail = std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<String>>>;

const STDERR_TAIL_MAX_LINES: usize = 40;

/// 起動中の sidecar プロセス。Drop 時に kill される
#[derive(Debug)]
pub struct SidecarProcess {
    child: Child,
    config: SidecarConfig,
    /// Keep the tail alive alongside the child so operators can inspect
    /// via [`Self::recent_stderr`] after the sidecar becomes healthy
    stderr_tail: StderrTail,
}

impl SidecarProcess {
    /// sidecar を起動し、`/health` が応答するまで待つ
    ///
    /// stderr は piped で取得し、直近 [`STDERR_TAIL_MAX_LINES`] 行を保持
    /// health check timeout や early exit 時にこのバッファを error msg に
    /// 含めて surface する 親コンソールにも `[sidecar]` prefix 付きで
    /// echo するので tracing subscriber と併せて cross-reference 可能
    pub async fn spawn(config: SidecarConfig) -> Result<Self> {
        let bin = resolve_bin_path(&config);

        if !config.model_path.exists() {
            bail!(
                "sidecar model file not found: {}",
                config.model_path.display()
            );
        }

        let mut cmd = Command::new(&bin);
        cmd.arg("--model")
            .arg(&config.model_path)
            .arg("--port")
            .arg(config.port.to_string());
        if config.hybrid {
            cmd.arg("--hybrid");
        }
        // stdout は継承 (少量 log)、stderr は piped で capture して
        // 直近 STDERR_TAIL_MAX_LINES 行を保持 早期 exit / health timeout
        // 時にこれを error msg に含めることで生 error を surface できる
        cmd.stdout(std::process::Stdio::inherit());
        cmd.stderr(std::process::Stdio::piped());
        cmd.kill_on_drop(true);

        info!(
            bin = %bin.display(),
            model = %config.model_path.display(),
            port = config.port,
            hybrid = config.hybrid,
            "spawning alice-llm-server sidecar"
        );

        let mut child = cmd
            .spawn()
            .with_context(|| format!("failed to spawn sidecar binary: {}", bin.display()))?;

        let stderr_tail: StderrTail = std::sync::Arc::new(std::sync::Mutex::new(
            std::collections::VecDeque::with_capacity(STDERR_TAIL_MAX_LINES),
        ));

        if let Some(stderr) = child.stderr.take() {
            let tail = stderr_tail.clone();
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    // Mirror to parent stderr with prefix so terminal
                    // debuggers see it in real time (tracing subscriber
                    // + parent shell both work)
                    eprintln!("[sidecar] {line}");
                    if let Ok(mut buf) = tail.lock() {
                        if buf.len() >= STDERR_TAIL_MAX_LINES {
                            buf.pop_front();
                        }
                        buf.push_back(line);
                    }
                }
            });
        }

        let mut sidecar = Self {
            child,
            config,
            stderr_tail,
        };
        sidecar.wait_healthy().await?;
        Ok(sidecar)
    }

    /// Snapshot of the most recent stderr lines (up to
    /// [`STDERR_TAIL_MAX_LINES`]) — useful for surfacing sidecar
    /// failures to the UI without keeping a live subscriber
    pub fn recent_stderr(&self) -> Vec<String> {
        self.stderr_tail
            .lock()
            .map(|buf| buf.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn stderr_snapshot_for_error(&self) -> String {
        let lines = self.recent_stderr();
        if lines.is_empty() {
            "(sidecar produced no stderr output)".to_string()
        } else {
            format!(
                "--- sidecar stderr (last {}) ---\n{}",
                lines.len(),
                lines.join("\n")
            )
        }
    }

    async fn wait_healthy(&mut self) -> Result<()> {
        let deadline = tokio::time::Instant::now() + self.config.health_timeout;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()?;
        let url = self.config.health_endpoint();

        loop {
            // Detect early exit — surface the actual exit status + stderr
            // instead of waiting the full timeout for a dead process
            if let Ok(Some(status)) = self.child.try_wait() {
                bail!(
                    "sidecar exited before becoming healthy (status: {})\n{}",
                    status,
                    self.stderr_snapshot_for_error()
                );
            }
            if tokio::time::Instant::now() >= deadline {
                bail!(
                    "sidecar failed to become healthy within {:?}\n{}",
                    self.config.health_timeout,
                    self.stderr_snapshot_for_error()
                );
            }
            match client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    info!(url = %url, "sidecar healthy");
                    return Ok(());
                }
                Ok(resp) => {
                    warn!(status = %resp.status(), "sidecar health check non-2xx");
                }
                Err(_) => {
                    // まだ起動中、待つ
                }
            }
            tokio::time::sleep(self.config.health_poll_interval).await;
        }
    }

    #[must_use]
    pub const fn config(&self) -> &SidecarConfig {
        &self.config
    }

    /// 明示的な shutdown。Drop でも kill されるが即時終了させたい場合に呼ぶ
    pub async fn shutdown(mut self) -> Result<()> {
        info!("shutting down sidecar");
        self.child.kill().await?;
        let _ = self.child.wait().await;
        Ok(())
    }
}

/// GUI 統合用の auto-spawn task
///
/// app 起動時に background task として spawn する典型的な使い方:
///
/// ```ignore
/// let (tx, rx) = tokio::sync::watch::channel(SidecarStatus::Waiting);
/// runtime.spawn(async move {
///     run_auto_spawn(model_path, 8000, tx).await;
/// });
/// ```
///
/// 動作:
///   1. `Starting` 送信
///   2. [`SidecarProcess::spawn`] 実行
///   3. 成功 → `Running` 送信 → `std::future::pending()` で park
///      (runtime drop 時に task cancel、SidecarProcess Drop で `kill_on_drop`)
///   4. 失敗 → `Error(msg)` 送信 → task return
pub async fn run_auto_spawn(
    model_path: PathBuf,
    port: u16,
    status_tx: tokio::sync::watch::Sender<SidecarStatus>,
) {
    let _ = status_tx.send(SidecarStatus::Starting);
    // Port は caller (state.rs) 側で事前 free port scan 済み client の
    // LlmConfig.endpoint も同じ port に同期更新される (client → sidecar
    // 経路の一貫性保証のため authoritative は caller に置く)
    let cfg = SidecarConfig {
        model_path,
        port,
        ..Default::default()
    };
    match SidecarProcess::spawn(cfg).await {
        Ok(_sidecar) => {
            let _ = status_tx.send(SidecarStatus::Running);
            // SidecarProcess を task スコープに保持して drop を遅らせる
            // runtime 停止時に task が cancel され、`_sidecar` の Drop で
            // `kill_on_drop` 経由で子プロセス kill
            std::future::pending::<()>().await;
        }
        Err(e) => {
            let msg = e.to_string();
            warn!(error = %msg, "sidecar auto-spawn failed");
            let _ = status_tx.send(SidecarStatus::Error(msg));
        }
    }
}

/// preferred から up to `max_attempts` 個の port を試して最初に bind できた
/// ものを返す 全部塞がっていたら None (呼び出し側で preferred fallback)
///
/// 実装: `std::net::TcpListener` で 127.0.0.1:port を bind 試行 成功したら
/// 即 close して port 番号を返す (bind から close までの間に他 process が
/// grab する TOCTOU race は理論的に残るが、port scan の一般的な pattern
/// として許容)
pub fn find_free_port_starting_at(preferred: u16, max_attempts: u16) -> Option<u16> {
    for i in 0..max_attempts {
        let candidate = preferred.checked_add(i)?;
        if std::net::TcpListener::bind(("127.0.0.1", candidate)).is_ok() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_endpoints() {
        let cfg = SidecarConfig::default();
        assert_eq!(cfg.port, 8000);
        assert!(cfg.chat_endpoint().contains(":8000"));
        assert!(cfg.chat_endpoint().ends_with("/v1/chat/completions"));
        assert!(cfg.health_endpoint().ends_with("/health"));
    }

    #[test]
    fn config_serialization_roundtrip() {
        let cfg = SidecarConfig {
            bin_path: Some(PathBuf::from("/usr/local/bin/alice-llm-server")),
            model_path: PathBuf::from("/tmp/model.gguf"),
            port: 12345,
            hybrid: true,
            ..Default::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: SidecarConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.port, cfg.port);
        assert_eq!(back.hybrid, cfg.hybrid);
        assert_eq!(back.bin_path, cfg.bin_path);
        assert_eq!(back.model_path, cfg.model_path);
    }

    #[tokio::test]
    async fn spawn_fails_when_model_missing() {
        let cfg = SidecarConfig {
            model_path: PathBuf::from("/nonexistent/model.gguf"),
            ..Default::default()
        };
        let err = SidecarProcess::spawn(cfg).await.unwrap_err();
        assert!(err.to_string().contains("model file not found"));
    }

    #[test]
    fn status_is_running_only_for_running() {
        assert!(!SidecarStatus::Waiting.is_running());
        assert!(!SidecarStatus::Starting.is_running());
        assert!(SidecarStatus::Running.is_running());
        assert!(!SidecarStatus::Error("nope".into()).is_running());
    }

    #[test]
    fn resolve_uses_explicit_bin_path_when_set() {
        let explicit = PathBuf::from("/opt/custom/alice-llm-server");
        let resolved = resolve_bin_path_from(Some(&explicit), None);
        assert_eq!(resolved, explicit);
    }

    #[test]
    fn resolve_prefers_exe_neighbor_when_binary_exists() {
        let dir = tempfile::tempdir().unwrap();
        let neighbor = dir.path().join(sidecar_exe_filename());
        std::fs::write(&neighbor, b"stub").unwrap();

        let resolved = resolve_bin_path_from(None, Some(dir.path()));
        assert_eq!(resolved, neighbor);
    }

    #[test]
    fn resolve_falls_back_to_path_lookup_when_neighbor_missing() {
        let dir = tempfile::tempdir().unwrap();
        // no binary placed next to the fake exe dir
        let resolved = resolve_bin_path_from(None, Some(dir.path()));
        assert_eq!(resolved, PathBuf::from(sidecar_exe_filename()));
    }

    #[test]
    fn resolve_explicit_wins_over_neighbor() {
        let dir = tempfile::tempdir().unwrap();
        let neighbor = dir.path().join(sidecar_exe_filename());
        std::fs::write(&neighbor, b"stub").unwrap();

        let explicit = PathBuf::from("/opt/custom/alice-llm-server");
        let resolved = resolve_bin_path_from(Some(&explicit), Some(dir.path()));
        assert_eq!(resolved, explicit);
    }

    #[test]
    fn sidecar_exe_filename_matches_platform() {
        let name = sidecar_exe_filename();
        if cfg!(target_os = "windows") {
            assert_eq!(name, "alice-llm-server.exe");
        } else {
            assert_eq!(name, "alice-llm-server");
        }
    }

    #[tokio::test]
    async fn auto_spawn_reports_error_when_model_missing() {
        let (tx, mut rx) = tokio::sync::watch::channel(SidecarStatus::Waiting);
        run_auto_spawn(PathBuf::from("/nonexistent/model.gguf"), 18080, tx).await;
        // 最終状態は Error
        rx.borrow_and_update();
        let status = rx.borrow().clone();
        match status {
            SidecarStatus::Error(msg) => {
                assert!(
                    msg.contains("model file not found"),
                    "unexpected error: {msg}"
                );
            }
            other => panic!("expected Error, got {other:?}"),
        }
    }
}
