use anyhow::{Result, bail};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use tracing::info;

use crate::model::ModelChoice;

/// モデルダウンロードの進捗
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub status: DownloadStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DownloadStatus {
    Pending,
    Downloading,
    Complete,
    Error(String),
}

/// Stage 3-C.9: `models_dir/{choice.default_filename()}` に存在するか
#[must_use]
pub fn model_exists(models_dir: &Path, choice: ModelChoice) -> bool {
    model_path(models_dir, choice).exists()
}

/// Stage 3-C.9: 指定 `ModelChoice` のローカルパスを返す 各 choice ごとの
/// GGUF filename (`ModelChoice::default_filename()`) を `models_dir` に
/// 結合するため、Qwen / Bonsai を並列にキャッシュ可能
#[must_use]
pub fn model_path(models_dir: &Path, choice: ModelChoice) -> PathBuf {
    models_dir.join(choice.default_filename())
}

/// Hugging Face から指定 `ModelChoice` の GGUF をダウンロード 既に
/// `model_path(...)` が存在すれば short-circuit で Complete を通知
///
/// # Errors
///
/// - `models_dir` 作成失敗
/// - HTTP エラー (5xx 等)
/// - ネットワーク中断 / 書き込み失敗
pub async fn download_model(
    models_dir: &Path,
    choice: ModelChoice,
    progress_tx: tokio::sync::watch::Sender<DownloadProgress>,
) -> Result<PathBuf> {
    std::fs::create_dir_all(models_dir)?;

    let output_path = model_path(models_dir, choice);

    if output_path.exists() {
        let _ = progress_tx.send(DownloadProgress {
            downloaded_bytes: 0,
            total_bytes: None,
            status: DownloadStatus::Complete,
        });
        return Ok(output_path);
    }

    let (repo, file) = choice.default_hf_ref();
    let url = format!("https://huggingface.co/{repo}/resolve/main/{file}");

    info!(url = %url, "downloading LLM model");

    let _ = progress_tx.send(DownloadProgress {
        downloaded_bytes: 0,
        total_bytes: None,
        status: DownloadStatus::Downloading,
    });

    let client = reqwest::Client::new();
    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        let status = response.status();
        bail!("download failed: HTTP {status}");
    }

    let total_bytes = response.content_length();

    let tmp_path = output_path.with_extension("gguf.tmp");
    let mut file = tokio::fs::File::create(&tmp_path).await?;
    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;

    use tokio_stream::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;

        let _ = progress_tx.send(DownloadProgress {
            downloaded_bytes: downloaded,
            total_bytes,
            status: DownloadStatus::Downloading,
        });
    }

    file.flush().await?;
    drop(file);

    tokio::fs::rename(&tmp_path, &output_path).await?;

    info!(
        path = %output_path.display(),
        bytes = downloaded,
        "model download complete"
    );

    let _ = progress_tx.send(DownloadProgress {
        downloaded_bytes: downloaded,
        total_bytes,
        status: DownloadStatus::Complete,
    });

    Ok(output_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_path_uses_choice_filename() {
        let p = model_path(Path::new("/tmp/models"), ModelChoice::Qwen35_4B);
        assert!(p.to_str().unwrap().ends_with("qwen3.5-4b-q4_k_m.gguf"));

        let p2 = model_path(Path::new("/tmp/models"), ModelChoice::Bonsai27B);
        assert!(p2.to_str().unwrap().ends_with("bonsai-27b-q1_0.gguf"));
    }

    #[test]
    fn model_paths_differ_per_choice() {
        let base = Path::new("/tmp/models");
        assert_ne!(
            model_path(base, ModelChoice::Qwen35_4B),
            model_path(base, ModelChoice::Bonsai27B),
        );
    }

    #[test]
    fn model_not_exists_in_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!model_exists(dir.path(), ModelChoice::Qwen35_4B));
        assert!(!model_exists(dir.path(), ModelChoice::Bonsai27B));
    }

    #[test]
    fn download_status_eq() {
        assert_eq!(DownloadStatus::Pending, DownloadStatus::Pending);
        assert_ne!(DownloadStatus::Pending, DownloadStatus::Complete);
    }
}
