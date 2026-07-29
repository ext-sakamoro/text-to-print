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

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use tokio::process::{Child, Command};
use tracing::{info, warn};

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
            health_timeout: Duration::from_secs(60),
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

/// 起動中の sidecar プロセス。Drop 時に kill される
#[derive(Debug)]
pub struct SidecarProcess {
    child: Child,
    config: SidecarConfig,
}

impl SidecarProcess {
    /// sidecar を起動し、`/health` が応答するまで待つ
    pub async fn spawn(config: SidecarConfig) -> Result<Self> {
        let bin = config
            .bin_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("alice-llm-server"));

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
        // 子プロセスの stdout/stderr は継承 (デバッグ用ログを親コンソールへ)
        cmd.kill_on_drop(true);

        info!(
            bin = %bin.display(),
            model = %config.model_path.display(),
            port = config.port,
            hybrid = config.hybrid,
            "spawning alice-llm-server sidecar"
        );

        let child = cmd
            .spawn()
            .with_context(|| format!("failed to spawn sidecar binary: {}", bin.display()))?;

        let sidecar = Self { child, config };
        sidecar.wait_healthy().await?;
        Ok(sidecar)
    }

    async fn wait_healthy(&self) -> Result<()> {
        let deadline = tokio::time::Instant::now() + self.config.health_timeout;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()?;
        let url = self.config.health_endpoint();

        loop {
            if tokio::time::Instant::now() >= deadline {
                bail!(
                    "sidecar failed to become healthy within {:?}",
                    self.config.health_timeout
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
}
