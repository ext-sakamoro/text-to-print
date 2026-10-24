use alice_crypto::sha256;
use anyhow::{Result, bail};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::info;

use crate::model::{ModelChoice, PinnedFile};

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

/// A model file that may be handed to the loader ([`crate::embedded_backend::EmbeddedBackend`]
/// and the sidecar take nothing else)
///
/// Made only by [`download_model`] and [`verified_model`], which check a pinned
/// file against its pin, and by [`ModelFile::user_supplied`] for a file the user
/// picked themselves
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFile {
    path: PathBuf,
    choice: ModelChoice,
}

impl ModelFile {
    /// A GGUF file the user chose by its path (the custom model setting) It is
    /// not checked: the user named this exact file Not for the files of
    /// [`ModelChoice`], which come from [`download_model`] / [`verified_model`]
    #[must_use]
    pub const fn user_supplied(path: PathBuf, choice: ModelChoice) -> Self {
        Self { path, choice }
    }

    /// Where the file is
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The choice the file is loaded as
    #[must_use]
    pub const fn choice(&self) -> ModelChoice {
        self.choice
    }
}

/// Whether the model of `choice` is in `models_dir` and usable
///
/// A pinned choice ([`ModelChoice::pinned_file`]) counts only once its bytes
/// have been checked against the pin: the check leaves a marker next to the
/// file holding the SHA-256, the size and the modification time it saw, and
/// a file changed since then no longer counts (the next [`download_model`]
/// checks it again) A choice with no pin is placed by hand and counts when the
/// file is there
#[must_use]
pub fn model_exists(models_dir: &Path, choice: ModelChoice) -> bool {
    let path = model_path(models_dir, choice);
    match choice.pinned_file() {
        Some(pin) => marker_matches(&path, &pin),
        None => path.is_file(),
    }
}

/// Stage 3-C.9: 指定 `ModelChoice` のローカルパスを返す 各 choice ごとの
/// GGUF filename (`ModelChoice::default_filename()`) を `models_dir` に
/// 結合するため、Qwen / Bonsai を並列にキャッシュ可能 Not public: a path
/// from here has not been checked; callers get a [`ModelFile`] instead
#[must_use]
pub(crate) fn model_path(models_dir: &Path, choice: ModelChoice) -> PathBuf {
    models_dir.join(choice.default_filename())
}

/// The URL a pinned file is fetched from: the file at its commit, never at a
/// branch that can move
#[must_use]
pub fn resolve_url(pin: &PinnedFile) -> String {
    format!(
        "https://huggingface.co/{}/resolve/{}/{}",
        pin.repo, pin.revision, pin.file
    )
}

/// Make the model of `choice` present and checked in `models_dir`
///
/// A file already there is checked against the pin (once; the marker records
/// it) and kept if it matches; otherwise it is removed and fetched again The
/// download goes to a temporary file whose SHA-256 is computed as it arrives;
/// it becomes the model only if its size and SHA-256 equal the pin, and is
/// removed otherwise
///
/// # Errors
///
/// - `models_dir` 作成失敗
/// - a choice with no pin whose file is not there (no download source)
/// - HTTP エラー / ネットワーク中断 / 書き込み失敗
/// - a download whose size or SHA-256 differs from the pin
pub async fn download_model(
    models_dir: &Path,
    choice: ModelChoice,
    progress_tx: tokio::sync::watch::Sender<DownloadProgress>,
) -> Result<ModelFile> {
    std::fs::create_dir_all(models_dir)?;
    let output_path = model_path(models_dir, choice);

    let Some(pin) = choice.pinned_file() else {
        if output_path.is_file() {
            send_complete(&progress_tx, 0, None);
            return Ok(ModelFile {
                path: output_path,
                choice,
            });
        }
        bail!(
            "{choice:?} has no download source; place the file at {} by hand",
            output_path.display()
        );
    };

    let path = ensure_pinned(&resolve_url(&pin), &output_path, &pin, &progress_tx).await?;
    Ok(ModelFile { path, choice })
}

/// Keep the file at `output_path` if it matches `pin`, otherwise remove it and
/// fetch `url` in its place
pub(crate) async fn ensure_pinned(
    url: &str,
    output_path: &Path,
    pin: &PinnedFile,
    progress_tx: &tokio::sync::watch::Sender<DownloadProgress>,
) -> Result<PathBuf> {
    if output_path.is_file() {
        if marker_matches(output_path, pin) || verify_existing(output_path, pin).await? {
            send_complete(progress_tx, pin.size, Some(pin.size));
            return Ok(output_path.to_path_buf());
        }
        tracing::warn!(
            path = %output_path.display(),
            "model file does not match its pinned SHA-256; fetching it again"
        );
        remove_model(output_path);
    }

    download_pinned(url, output_path, pin, progress_tx).await
}

/// The model of `choice`, only if it may be loaded
///
/// A pinned choice is returned only when its file matches the pin: the marker
/// agrees with the file as it is now, or hashing the file gives the pinned
/// SHA-256 (the marker is then written) Nothing is fetched or removed here A
/// choice with no pin is placed by hand and is returned when its file is there
/// The loader takes only a [`ModelFile`], which comes from here, from
/// [`download_model`] or from [`ModelFile::user_supplied`]
///
/// # Errors
/// The file is missing, or (for a pinned choice) its bytes differ from the pin
pub async fn verified_model(models_dir: &Path, choice: ModelChoice) -> Result<ModelFile> {
    let path = model_path(models_dir, choice);
    if !path.is_file() {
        bail!(
            "{choice:?} is not downloaded ({} is missing)",
            path.display()
        );
    }
    match choice.pinned_file() {
        None => Ok(ModelFile { path, choice }),
        Some(pin) => {
            if marker_matches(&path, &pin) || verify_existing(&path, &pin).await? {
                Ok(ModelFile { path, choice })
            } else {
                bail!(
                    "{} does not match the pinned SHA-256 of {choice:?}; download it again",
                    path.display()
                )
            }
        }
    }
}

/// Fetch `url` into `output_path`, keeping it only if it matches `pin`
pub(crate) async fn download_pinned(
    url: &str,
    output_path: &Path,
    pin: &PinnedFile,
    progress_tx: &tokio::sync::watch::Sender<DownloadProgress>,
) -> Result<PathBuf> {
    info!(url = %url, "downloading LLM model");
    let _ = progress_tx.send(DownloadProgress {
        downloaded_bytes: 0,
        total_bytes: Some(pin.size),
        status: DownloadStatus::Downloading,
    });

    let tmp_path = output_path.with_extension("gguf.tmp");
    let fetched = fetch_checked(url, &tmp_path, pin, progress_tx).await;
    if let Err(e) = fetched {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(e);
    }

    tokio::fs::rename(&tmp_path, output_path).await?;
    write_marker(output_path, pin)?;
    info!(path = %output_path.display(), bytes = pin.size, "model download complete");
    send_complete(progress_tx, pin.size, Some(pin.size));
    Ok(output_path.to_path_buf())
}

/// Stream `url` into `tmp_path` while hashing it; fails unless the bytes have
/// exactly the pinned size and SHA-256
async fn fetch_checked(
    url: &str,
    tmp_path: &Path,
    pin: &PinnedFile,
    progress_tx: &tokio::sync::watch::Sender<DownloadProgress>,
) -> Result<()> {
    let response = reqwest::Client::new().get(url).send().await?;
    if !response.status().is_success() {
        let status = response.status();
        bail!("download failed: HTTP {status}");
    }
    if let Some(len) = response.content_length()
        && len != pin.size
    {
        bail!(
            "download is {len} bytes, the pinned file is {} bytes",
            pin.size
        );
    }

    let mut file = tokio::fs::File::create(tmp_path).await?;
    let mut hasher = sha256::Hasher::new();
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();
    use tokio_stream::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        downloaded += chunk.len() as u64;
        if downloaded > pin.size {
            bail!("download is longer than the pinned {} bytes", pin.size);
        }
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
        let _ = progress_tx.send(DownloadProgress {
            downloaded_bytes: downloaded,
            total_bytes: Some(pin.size),
            status: DownloadStatus::Downloading,
        });
    }
    file.flush().await?;
    drop(file);

    if downloaded != pin.size {
        bail!(
            "download ended at {downloaded} of the pinned {} bytes",
            pin.size
        );
    }
    let digest = hex(&hasher.finalize());
    if digest != pin.sha256 {
        bail!(
            "download SHA-256 {digest} differs from the pinned {}",
            pin.sha256
        );
    }
    Ok(())
}

/// Check a file already on disk against `pin`, recording the result in its
/// marker when it matches
async fn verify_existing(path: &Path, pin: &PinnedFile) -> Result<bool> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = sha256::Hasher::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut total: u64 = 0;
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > pin.size {
            return Ok(false);
        }
        hasher.update(&buf[..n]);
    }
    if total != pin.size || hex(&hasher.finalize()) != pin.sha256 {
        return Ok(false);
    }
    write_marker(path, pin)?;
    Ok(true)
}

fn marker_path(path: &Path) -> PathBuf {
    path.with_extension("gguf.verified")
}

/// The marker line for a file in its current state: SHA-256, size and
/// modification time (nanoseconds since the epoch)
fn stamp(path: &Path, sha256: &str) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(format!("{sha256} {} {mtime}\n", meta.len()))
}

/// The marker records the file's SHA-256, size and modification time when it
/// was checked; a later file with another size or time is checked again This
/// guards against partial writes, replaced downloads and edits, not against a
/// local writer that restores the size and the modification time
fn marker_matches(path: &Path, pin: &PinnedFile) -> bool {
    let Some(expected) = stamp(path, pin.sha256) else {
        return false;
    };
    std::fs::metadata(path).is_ok_and(|m| m.len() == pin.size)
        && std::fs::read_to_string(marker_path(path)).is_ok_and(|m| m == expected)
}

fn write_marker(path: &Path, pin: &PinnedFile) -> Result<()> {
    let Some(line) = stamp(path, pin.sha256) else {
        bail!("can not read {} to record its check", path.display());
    };
    std::fs::write(marker_path(path), line)?;
    Ok(())
}

fn remove_model(path: &Path) {
    let _ = std::fs::remove_file(marker_path(path));
    let _ = std::fs::remove_file(path);
}

fn send_complete(
    progress_tx: &tokio::sync::watch::Sender<DownloadProgress>,
    bytes: u64,
    total: Option<u64>,
) {
    let _ = progress_tx.send(DownloadProgress {
        downloaded_bytes: bytes,
        total_bytes: total,
        status: DownloadStatus::Complete,
    });
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_path_uses_choice_filename() {
        // 2026-08-07 β: Qwen35_4B は Qwen 2.5-3B-Instruct を substitute
        // (Apple M3 iGPU 制約対策で 7B → 3B ダウンサイズ) enum 名は保持、
        // filename のみ 3B 版に変更
        let p = model_path(Path::new("/tmp/models"), ModelChoice::Qwen35_4B);
        assert!(
            p.to_str()
                .unwrap()
                .ends_with("qwen2.5-3b-instruct-q4_k_m.gguf")
        );

        let p2 = model_path(Path::new("/tmp/models"), ModelChoice::Gemma2_27B);
        assert!(p2.to_str().unwrap().ends_with("gemma-2-27b-it-q3_k_l.gguf"));
    }

    #[test]
    fn model_paths_differ_per_choice() {
        let base = Path::new("/tmp/models");
        assert_ne!(
            model_path(base, ModelChoice::Qwen35_4B),
            model_path(base, ModelChoice::Gemma2_27B),
        );
    }

    #[test]
    fn model_not_exists_in_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!model_exists(dir.path(), ModelChoice::Qwen35_4B));
        assert!(!model_exists(dir.path(), ModelChoice::Gemma2_27B));
    }

    #[test]
    fn download_status_eq() {
        assert_eq!(DownloadStatus::Pending, DownloadStatus::Pending);
        assert_ne!(DownloadStatus::Pending, DownloadStatus::Complete);
    }

    // oracle: FIPS 180-4 example, SHA-256("abc")
    const ABC: PinnedFile = PinnedFile {
        repo: "test/repo",
        revision: "0123456789abcdef0123456789abcdef01234567",
        file: "m.gguf",
        sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        size: 3,
    };

    /// Serve `body` once over HTTP on 127.0.0.1 with the given Content-Length
    async fn serve_once(body: &'static [u8], content_length: usize) -> String {
        serve_once_with(body, Some(content_length)).await
    }

    /// Serve `body` once; with `None` the response has no Content-Length and
    /// ends when the connection closes
    async fn serve_once_with(body: &'static [u8], content_length: Option<usize>) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut req = vec![0u8; 4096];
            let _ = sock.read(&mut req).await;
            let length =
                content_length.map_or(String::new(), |n| format!("Content-Length: {n}\r\n"));
            let head = format!("HTTP/1.1 200 OK\r\n{length}Connection: close\r\n\r\n");
            let _ = sock.write_all(head.as_bytes()).await;
            let _ = sock.write_all(body).await;
            let _ = sock.shutdown().await;
        });
        format!("http://{addr}/m.gguf")
    }

    fn progress() -> tokio::sync::watch::Sender<DownloadProgress> {
        tokio::sync::watch::channel(DownloadProgress {
            downloaded_bytes: 0,
            total_bytes: None,
            status: DownloadStatus::Pending,
        })
        .0
    }

    fn leftovers(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[tokio::test]
    async fn a_download_matching_the_pin_is_kept_and_marked() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("m.gguf");
        let url = serve_once(b"abc", 3).await;
        download_pinned(&url, &out, &ABC, &progress())
            .await
            .unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), b"abc");
        assert!(marker_matches(&out, &ABC));
        assert_eq!(leftovers(dir.path()), ["m.gguf", "m.gguf.verified"]);
    }

    #[tokio::test]
    async fn a_download_with_other_bytes_is_refused_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("m.gguf");
        let url = serve_once(b"abd", 3).await;
        let e = download_pinned(&url, &out, &ABC, &progress())
            .await
            .unwrap_err();
        assert!(e.to_string().contains("SHA-256"), "{e}");
        assert!(leftovers(dir.path()).is_empty());
    }

    #[tokio::test]
    async fn a_short_or_long_download_is_refused_and_removed() {
        for (body, len) in [(&b"ab"[..], 2), (&b"abcd"[..], 4)] {
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("m.gguf");
            let url = serve_once(body, len).await;
            assert!(
                download_pinned(&url, &out, &ABC, &progress())
                    .await
                    .is_err()
            );
            assert!(leftovers(dir.path()).is_empty(), "{body:?}");
        }
    }

    #[tokio::test]
    async fn a_body_cut_short_of_its_content_length_is_refused_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("m.gguf");
        let url = serve_once(b"ab", 3).await;
        assert!(
            download_pinned(&url, &out, &ABC, &progress())
                .await
                .is_err()
        );
        assert!(leftovers(dir.path()).is_empty());
    }

    #[tokio::test]
    async fn a_file_already_there_is_checked_before_it_counts() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("m.gguf");
        std::fs::write(&out, b"abc").unwrap();
        assert!(
            !marker_matches(&out, &ABC),
            "an unchecked file must not count"
        );
        assert!(verify_existing(&out, &ABC).await.unwrap());
        assert!(marker_matches(&out, &ABC));

        // the same size with other bytes, written after the check
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&out, b"abd").unwrap();
        assert!(
            !marker_matches(&out, &ABC),
            "a file changed after its check must not count"
        );
        assert!(!verify_existing(&out, &ABC).await.unwrap());
    }

    #[test]
    fn every_pinned_choice_names_a_commit_a_digest_and_a_size() {
        let mut pinned = 0;
        for &choice in ModelChoice::all() {
            let Some(pin) = choice.pinned_file() else {
                assert!(choice.requires_manual_placement(), "{choice:?}");
                continue;
            };
            let lower_hex = |s: &str, n: usize| {
                s.len() == n
                    && s.bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            };
            assert!(lower_hex(pin.revision, 40), "{choice:?} revision");
            assert!(lower_hex(pin.sha256, 64), "{choice:?} sha256");
            assert!(pin.size > 0, "{choice:?} size");
            assert_eq!((pin.repo, pin.file), choice.default_hf_ref(), "{choice:?}");
            let url = resolve_url(&pin);
            assert!(
                url.contains(&format!("/resolve/{}/", pin.revision)),
                "{url}"
            );
            assert!(!url.contains("/resolve/main/"), "{url}");
            pinned += 1;
        }
        assert_eq!(pinned, 5);
    }

    #[tokio::test]
    async fn a_choice_with_no_source_is_not_fetched() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!model_exists(dir.path(), ModelChoice::Bonsai27B));
        let e = download_model(dir.path(), ModelChoice::Bonsai27B, progress())
            .await
            .unwrap_err();
        assert!(e.to_string().contains("by hand"), "{e}");
    }

    /// A response with no length that runs past the pin stops being read at
    /// the pinned size: no progress beyond it is reported (so nothing beyond it
    /// is written), and nothing is kept
    #[tokio::test]
    async fn a_download_with_no_length_stops_at_the_pinned_size() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("m.gguf");
        let url = serve_once_with(b"abcabcabc", None).await;
        let (tx, rx) = tokio::sync::watch::channel(DownloadProgress {
            downloaded_bytes: 0,
            total_bytes: None,
            status: DownloadStatus::Pending,
        });
        let mut seen = rx.clone();
        let watcher = tokio::spawn(async move {
            let mut most = 0;
            while seen.changed().await.is_ok() {
                most = most.max(seen.borrow().downloaded_bytes);
            }
            most
        });
        assert!(download_pinned(&url, &out, &ABC, &tx).await.is_err());
        drop(tx);
        drop(rx);
        assert!(watcher.await.unwrap() <= ABC.size);
        assert!(leftovers(dir.path()).is_empty());
    }

    #[test]
    fn an_unchecked_file_at_the_model_path_is_not_a_model() {
        let dir = tempfile::tempdir().unwrap();
        let choice = ModelChoice::Qwen35_4B;
        std::fs::write(model_path(dir.path(), choice), b"not the pinned bytes").unwrap();
        assert!(!model_exists(dir.path(), choice));
        // a manually placed choice has nothing to check against
        std::fs::write(model_path(dir.path(), ModelChoice::Bonsai27B), b"x").unwrap();
        assert!(model_exists(dir.path(), ModelChoice::Bonsai27B));
    }

    /// A file already at the model path with other bytes is not used: it is
    /// removed and the pinned bytes are fetched in its place
    #[tokio::test]
    async fn an_existing_file_with_other_bytes_is_fetched_again() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("m.gguf");
        std::fs::write(&out, b"abd").unwrap();
        let url = serve_once(b"abc", 3).await;
        ensure_pinned(&url, &out, &ABC, &progress()).await.unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), b"abc");
        assert!(marker_matches(&out, &ABC));
    }

    /// A file already there with the pinned bytes is kept without fetching
    /// (the URL serves nothing)
    #[tokio::test]
    async fn an_existing_file_with_the_pinned_bytes_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("m.gguf");
        std::fs::write(&out, b"abc").unwrap();
        ensure_pinned("http://127.0.0.1:9/unused", &out, &ABC, &progress())
            .await
            .unwrap();
        assert!(marker_matches(&out, &ABC));
    }

    /// When the fetch that replaces a file with other bytes fails, those bytes
    /// are not left at the model path (other code opens the path directly)
    #[tokio::test]
    async fn a_failed_refetch_leaves_nothing_at_the_model_path() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("m.gguf");
        std::fs::write(&out, b"abd").unwrap();
        let url = serve_once(b"abx", 3).await;
        assert!(ensure_pinned(&url, &out, &ABC, &progress()).await.is_err());
        assert!(leftovers(dir.path()).is_empty());
    }

    /// A pinned choice whose file has other bytes is never handed to the loader
    #[tokio::test]
    async fn a_pinned_choice_with_other_bytes_is_not_loadable() {
        let dir = tempfile::tempdir().unwrap();
        let choice = ModelChoice::Gemma2_27B;
        assert!(verified_model(dir.path(), choice).await.is_err(), "missing");
        let path = model_path(dir.path(), choice);
        std::fs::write(&path, b"not the pinned bytes").unwrap();
        let e = verified_model(dir.path(), choice).await.unwrap_err();
        assert!(e.to_string().contains("pinned SHA-256"), "{e}");
        // the file is left as it was (nothing is removed on the load path)
        assert_eq!(std::fs::read(&path).unwrap(), b"not the pinned bytes");
    }

    #[tokio::test]
    async fn a_manually_placed_choice_is_loadable_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let choice = ModelChoice::Bonsai27B;
        assert!(verified_model(dir.path(), choice).await.is_err());
        std::fs::write(model_path(dir.path(), choice), b"x").unwrap();
        assert!(verified_model(dir.path(), choice).await.is_ok());
    }
}
