use anyhow::{Result, bail};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use tracing::info;

const DEFAULT_MODEL_REPO: &str = "Qwen/Qwen2.5-7B-Instruct-GGUF";
const DEFAULT_MODEL_FILE: &str = "qwen2.5-7b-instruct-q4_k_m.gguf";

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

/// モデルが存在するか確認
pub fn model_exists(models_dir: &Path) -> bool {
    model_path(models_dir).exists()
}

/// モデルのパスを返す
pub fn model_path(models_dir: &Path) -> PathBuf {
    models_dir.join(DEFAULT_MODEL_FILE)
}

/// Hugging Face からモデルをダウンロード
pub async fn download_model(
    models_dir: &Path,
    progress_tx: tokio::sync::watch::Sender<DownloadProgress>,
) -> Result<PathBuf> {
    std::fs::create_dir_all(models_dir)?;

    let output_path = model_path(models_dir);

    if output_path.exists() {
        let _ = progress_tx.send(DownloadProgress {
            downloaded_bytes: 0,
            total_bytes: None,
            status: DownloadStatus::Complete,
        });
        return Ok(output_path);
    }

    let url = format!(
        "https://huggingface.co/{}/resolve/main/{}",
        DEFAULT_MODEL_REPO, DEFAULT_MODEL_FILE
    );

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
    fn model_path_format() {
        let p = model_path(Path::new("/tmp/models"));
        assert!(p.to_str().unwrap().ends_with(".gguf"));
    }

    #[test]
    fn model_not_exists_in_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!model_exists(dir.path()));
    }

    #[test]
    fn download_status_eq() {
        assert_eq!(DownloadStatus::Pending, DownloadStatus::Pending);
        assert_ne!(DownloadStatus::Pending, DownloadStatus::Complete);
    }
}
