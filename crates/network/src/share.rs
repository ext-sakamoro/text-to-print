use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// LoRA share upload payload.
///
/// Free-tier generations opt-in to sharing (prompt, lol source, mesh hash) with
/// piggybacked quality signals — success / retry / latency / safety / user
/// intent — that feed the RLHF-lite preference label for DSL quality LoRA
/// training. Mirrors the `quality` section of `alice_manifest.json` (v1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharePayload {
    pub schema_version: String,
    pub uuid: String,
    pub prompt: String,
    pub prompt_lang: String,
    pub llm_model: String,
    pub lol_source: String,
    pub lol_sha256: String,
    pub mesh_sha256: String,
    pub quality: QualitySignals,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualitySignals {
    pub success: bool,
    pub retry_count: u32,
    pub time_to_file_ms: u64,
    pub safety_violations: Vec<String>,
    pub export_format: String,
    pub user_kept: bool,
    pub user_edited: bool,
}

/// Fields that describe a completed generation, before serialising to the wire.
///
/// A concrete conversion from `text_to_print_core::manifest::AliceManifest`
/// lives at the app boundary to keep this crate free of a hard core dep.
pub struct ShareInputs<'a> {
    pub uuid: &'a str,
    pub schema_version: &'a str,
    pub prompt: &'a str,
    pub prompt_lang: &'a str,
    pub llm_model: &'a str,
    pub lol_source: &'a str,
    pub lol_sha256: &'a str,
    pub mesh_sha256: &'a str,
    pub success: bool,
    pub retry_count: u32,
    pub time_to_file_ms: u64,
    pub safety_violations: Vec<String>,
    pub export_format: &'a str,
    pub user_kept: bool,
    pub user_edited: bool,
}

/// Persist a `SharePayload` to a local dry-run directory so we can
/// verify the opt-in path actually queues something before the real
/// Cloudflare Workers upload backend (Epic-Infra #35) is in place
///
/// Files are named `{uuid}.json` and pretty-printed for easy inspection
/// The caller supplies `output_dir` (typically
/// `data_dir/share_dry_run/`) which is created if missing
///
/// # Errors
///
/// - IO error creating `output_dir` or writing the JSON file
/// - Serialization error from `serde_json` (should not happen for well-
///   formed `SharePayload` but is surfaced for completeness)
pub fn dump_dry_run(
    payload: &SharePayload,
    output_dir: &std::path::Path,
) -> anyhow::Result<std::path::PathBuf> {
    std::fs::create_dir_all(output_dir)?;
    let path = output_dir.join(format!("{}.json", payload.uuid));
    let json = serde_json::to_string_pretty(payload)?;
    std::fs::write(&path, json)?;
    Ok(path)
}

/// Count the number of dry-run share payloads queued in `dir`
///
/// Returns `0` when `dir` does not exist (queue-empty condition) and
/// counts `.json` files at the top level Meant for the Settings UI
/// "queued: N" indicator
#[must_use]
pub fn count_dry_run_queued(dir: &std::path::Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext == "json")
        })
        .count()
}

impl SharePayload {
    pub fn from_inputs(i: ShareInputs<'_>) -> Self {
        Self {
            schema_version: i.schema_version.to_string(),
            uuid: i.uuid.to_string(),
            prompt: i.prompt.to_string(),
            prompt_lang: i.prompt_lang.to_string(),
            llm_model: i.llm_model.to_string(),
            lol_source: i.lol_source.to_string(),
            lol_sha256: i.lol_sha256.to_string(),
            mesh_sha256: i.mesh_sha256.to_string(),
            quality: QualitySignals {
                success: i.success,
                retry_count: i.retry_count,
                time_to_file_ms: i.time_to_file_ms,
                safety_violations: i.safety_violations,
                export_format: i.export_format.to_string(),
                user_kept: i.user_kept,
                user_edited: i.user_edited,
            },
        }
    }
}

// ============================================================================
// GAP-3 (#44): SharePayload upload via HTTPS + offline queue
// ============================================================================

/// Client configuration for the Cloudflare Workers upload endpoint (#35)
///
/// Endpoint default = `text-to-print.alicelaw.net/api/share` (alicelaw.net
/// is Cloudflare-managed so the subdomain add is trivial) The endpoint can
/// be overridden for testing (mock server) or when the operator re-deploys
/// the worker at a different route
#[derive(Debug, Clone)]
pub struct ShareConfig {
    /// Full URL of the upload endpoint
    pub endpoint_url: String,
    /// Per-request HTTP timeout Default 10s
    pub timeout: std::time::Duration,
}

impl Default for ShareConfig {
    fn default() -> Self {
        Self {
            endpoint_url: "https://text-to-print.alicelaw.net/api/share".to_string(),
            timeout: std::time::Duration::from_secs(10),
        }
    }
}

/// Receipt returned by the Cloudflare Worker on successful accept (`202`)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UploadReceipt {
    pub status: String,
    pub receipt_id: String,
    pub timestamp: String,
}

/// Server-side rejection body (both `400` schema and `429` rate-limit share
/// the same wire shape; distinguished via [`UploadError`])
#[derive(Debug, Clone, Deserialize, Serialize)]
struct UploadRejection {
    pub status: String,
    pub reason: String,
    pub details: String,
}

/// Errors that can occur during a [`upload_share_payload`] call
///
/// The `Rejected` / `RateLimited` variants carry the server-side `reason`
/// tag so the caller can pattern-match (schema failure vs uuid cap vs ip cap)
#[derive(Debug, thiserror::Error)]
pub enum UploadError {
    #[error("HTTP client build error: {0}")]
    Client(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("rejected by server: {reason} — {details}")]
    Rejected { reason: String, details: String },
    #[error("rate limited by server: {reason}")]
    RateLimited { reason: String },
    #[error("server error: HTTP {0}")]
    Server(u16),
}

/// Return type of [`upload_share_payload`] Distinguishes fresh accept vs
/// idempotent replay (same uuid re-sent)
#[derive(Debug, Clone)]
pub enum UploadOutcome {
    Accepted(UploadReceipt),
}

/// POST the payload to the configured Cloudflare Workers endpoint
///
/// # Errors
///
/// - [`UploadError::Client`] if `reqwest::Client` build fails
/// - [`UploadError::Network`] on I/O / TLS / DNS failure
/// - [`UploadError::Rejected`] on `400` schema violation
/// - [`UploadError::RateLimited`] on `429` per-uuid / per-ip cap
/// - [`UploadError::Server`] on any other non-2xx status
pub async fn upload_share_payload(
    config: &ShareConfig,
    payload: &SharePayload,
) -> Result<UploadOutcome, UploadError> {
    let client = reqwest::Client::builder()
        .timeout(config.timeout)
        .build()
        .map_err(|e| UploadError::Client(e.to_string()))?;

    let resp = client
        .post(&config.endpoint_url)
        .json(payload)
        .send()
        .await
        .map_err(|e| UploadError::Network(e.to_string()))?;

    let status = resp.status().as_u16();
    match status {
        // 202 Accepted — worker persisted the row (or absorbed idempotent replay)
        200 | 202 => {
            let receipt: UploadReceipt = resp
                .json()
                .await
                .map_err(|e| UploadError::Network(e.to_string()))?;
            Ok(UploadOutcome::Accepted(receipt))
        }
        // 400 = schema / validation reject
        400 => {
            let body: UploadRejection = resp
                .json()
                .await
                .map_err(|e| UploadError::Network(e.to_string()))?;
            Err(UploadError::Rejected {
                reason: body.reason,
                details: body.details,
            })
        }
        // 429 = rate limit hit
        429 => {
            let body: UploadRejection = resp
                .json()
                .await
                .map_err(|e| UploadError::Network(e.to_string()))?;
            Err(UploadError::RateLimited {
                reason: body.reason,
            })
        }
        // 5xx or anything else — surface raw
        other => Err(UploadError::Server(other)),
    }
}

// ============================================================================
// Offline queue (file-backed, 24 h TTL)
// ============================================================================

/// Maximum time a queued payload is retained before being discarded
///
/// 24 h matches the tier discussion in `memory/text-to-print.md`
/// (Stage 5 T5.2 opt-out toggle + offline queue TTL)
pub const QUEUE_TTL: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// Persist a payload to the pending queue directory so a later
/// [`retry_queued_uploads`] sweep can attempt delivery
///
/// # Errors
///
/// - IO error creating the directory or writing the file
/// - Serialization error from `serde_json`
pub fn enqueue(payload: &SharePayload, queue_dir: &Path) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(queue_dir)?;
    let path = queue_dir.join(format!("{}.json", payload.uuid));
    let json = serde_json::to_string_pretty(payload)?;
    std::fs::write(&path, json)?;
    Ok(path)
}

/// Count the pending queue files (JSON at the top level of `queue_dir`)
/// Missing directory returns `0` — that's the empty-queue signal
#[must_use]
pub fn count_pending(queue_dir: &Path) -> usize {
    count_dry_run_queued(queue_dir)
}

/// Report of a single sweep of the queue
#[derive(Debug, Default, Clone)]
pub struct RetrySummary {
    /// Uploads that succeeded and were removed from the queue
    pub delivered: usize,
    /// Files older than [`QUEUE_TTL`] that were dropped without retry
    pub expired: usize,
    /// Uploads that failed transiently and remain in the queue
    pub still_pending: usize,
    /// Uploads rejected by the server (schema violation) and dropped from
    /// the queue since retrying an invalid payload can never succeed
    pub rejected_permanently: usize,
}

/// Attempt to upload every pending payload once
///
/// - `TTL`-expired files are unlinked without an upload attempt
/// - On `Accepted`, the queue file is unlinked
/// - On `Rejected` (400 schema violation), the queue file is unlinked —
///   retries can never fix a schema failure
/// - On `RateLimited` / `Network` / `Server` / `Client`, the file is kept
///   for the next sweep
///
/// The sweep is deliberately sequential to keep it easy to reason about; a
/// solo-user client sees only a handful of queued files at any time
///
/// # Errors
///
/// - IO error listing the queue directory
pub async fn retry_queued_uploads(
    config: &ShareConfig,
    queue_dir: &Path,
) -> anyhow::Result<RetrySummary> {
    let mut summary = RetrySummary::default();
    let entries = match std::fs::read_dir(queue_dir) {
        Ok(e) => e,
        // No queue dir yet = empty queue
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(summary),
        Err(err) => return Err(err.into()),
    };

    let now = std::time::SystemTime::now();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        // TTL check — discard files older than QUEUE_TTL
        if let Ok(meta) = entry.metadata()
            && let Ok(modified) = meta.modified()
            && now
                .duration_since(modified)
                .map(|d| d > QUEUE_TTL)
                .unwrap_or(false)
        {
            let _ = std::fs::remove_file(&path);
            summary.expired += 1;
            continue;
        }

        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(_) => {
                summary.still_pending += 1;
                continue;
            }
        };
        let payload: SharePayload = match serde_json::from_slice(&bytes) {
            Ok(p) => p,
            Err(_) => {
                // Corrupt file — drop it rather than retry forever
                let _ = std::fs::remove_file(&path);
                summary.rejected_permanently += 1;
                continue;
            }
        };

        match upload_share_payload(config, &payload).await {
            Ok(UploadOutcome::Accepted(_)) => {
                let _ = std::fs::remove_file(&path);
                summary.delivered += 1;
            }
            Err(UploadError::Rejected { .. }) => {
                // Schema violation is permanent — retrying can never succeed
                let _ = std::fs::remove_file(&path);
                summary.rejected_permanently += 1;
            }
            Err(_) => {
                summary.still_pending += 1;
            }
        }
    }

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_payload(uuid: &str) -> SharePayload {
        SharePayload::from_inputs(ShareInputs {
            uuid,
            schema_version: "1",
            prompt: "test",
            prompt_lang: "en",
            llm_model: "test-model",
            lol_source: "sphere(1.0)",
            lol_sha256: "a".repeat(64).as_str(),
            mesh_sha256: "b".repeat(64).as_str(),
            success: true,
            retry_count: 0,
            time_to_file_ms: 1000,
            safety_violations: vec![],
            export_format: "3mf",
            user_kept: true,
            user_edited: false,
        })
    }

    #[test]
    fn dump_dry_run_writes_json_file() {
        let dir = tempfile::tempdir().unwrap();
        let payload = make_test_payload("018f4e8c-0000-7000-8000-000000000001");
        let path = dump_dry_run(&payload, dir.path()).expect("dump should succeed");
        assert!(path.exists());
        assert_eq!(path.extension().unwrap(), "json");
        let content = std::fs::read_to_string(&path).unwrap();
        let back: SharePayload = serde_json::from_str(&content).unwrap();
        assert_eq!(back.uuid, payload.uuid);
    }

    #[test]
    fn dump_dry_run_creates_missing_directory() {
        let base = tempfile::tempdir().unwrap();
        let nested = base.path().join("does").join("not").join("exist");
        let payload = make_test_payload("018f4e8c-0000-7000-8000-000000000002");
        let path = dump_dry_run(&payload, &nested).expect("nested dir should be created");
        assert!(path.exists());
    }

    #[test]
    fn count_dry_run_queued_returns_zero_for_missing_dir() {
        let base = tempfile::tempdir().unwrap();
        assert_eq!(count_dry_run_queued(&base.path().join("nonexistent")), 0);
    }

    #[test]
    fn count_dry_run_queued_counts_json_files_only() {
        let dir = tempfile::tempdir().unwrap();
        dump_dry_run(&make_test_payload("uuid-1"), dir.path()).unwrap();
        dump_dry_run(&make_test_payload("uuid-2"), dir.path()).unwrap();
        std::fs::write(dir.path().join("ignore.txt"), "not json").unwrap();
        assert_eq!(count_dry_run_queued(dir.path()), 2);
    }

    // ---------- Upload path tests (GAP-3 #44) ----------

    fn make_config(url: String) -> ShareConfig {
        ShareConfig {
            endpoint_url: url,
            timeout: std::time::Duration::from_secs(5),
        }
    }

    #[tokio::test]
    async fn upload_accepts_202_response() {
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/share"))
            .and(header("content-type", "application/json"))
            .respond_with(ResponseTemplate::new(202).set_body_json(serde_json::json!({
                "status": "accepted",
                "receipt_id": "018f4e8c-0000-7000-8000-000000000001",
                "timestamp": "2026-07-29T09:00:00Z",
            })))
            .mount(&server)
            .await;

        let payload = make_test_payload("018f4e8c-0000-7000-8000-000000000001");
        let cfg = make_config(format!("{}/api/share", server.uri()));

        let outcome = upload_share_payload(&cfg, &payload).await.expect("accept");
        let UploadOutcome::Accepted(receipt) = outcome;
        assert_eq!(receipt.status, "accepted");
        assert_eq!(receipt.receipt_id, "018f4e8c-0000-7000-8000-000000000001");
    }

    #[tokio::test]
    async fn upload_maps_400_to_rejected() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/share"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "status": "rejected",
                "reason": "schema_version_mismatch",
                "details": "expected schema_version = \"1\"",
            })))
            .mount(&server)
            .await;

        let payload = make_test_payload("018f4e8c-0000-7000-8000-000000000002");
        let cfg = make_config(format!("{}/api/share", server.uri()));

        let err = upload_share_payload(&cfg, &payload).await.unwrap_err();
        let UploadError::Rejected { reason, .. } = err else {
            panic!("expected Rejected");
        };
        assert_eq!(reason, "schema_version_mismatch");
    }

    #[tokio::test]
    async fn upload_maps_429_to_rate_limited() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/share"))
            .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
                "status": "rejected",
                "reason": "rate_limit_uuid",
                "details": "per-UUID hourly cap exceeded",
            })))
            .mount(&server)
            .await;

        let payload = make_test_payload("018f4e8c-0000-7000-8000-000000000003");
        let cfg = make_config(format!("{}/api/share", server.uri()));

        let err = upload_share_payload(&cfg, &payload).await.unwrap_err();
        let UploadError::RateLimited { reason } = err else {
            panic!("expected RateLimited");
        };
        assert_eq!(reason, "rate_limit_uuid");
    }

    #[tokio::test]
    async fn upload_maps_5xx_to_server_error() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/share"))
            .respond_with(ResponseTemplate::new(503).set_body_string("upstream unavailable"))
            .mount(&server)
            .await;

        let payload = make_test_payload("018f4e8c-0000-7000-8000-000000000004");
        let cfg = make_config(format!("{}/api/share", server.uri()));

        let err = upload_share_payload(&cfg, &payload).await.unwrap_err();
        assert!(matches!(err, UploadError::Server(503)));
    }

    #[tokio::test]
    async fn retry_queue_delivers_pending_and_drops_permanent_rejects() {
        use wiremock::matchers::{body_partial_json, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        let cfg = make_config(format!("{}/api/share", server.uri()));

        let dir = tempfile::tempdir().unwrap();
        let queue = dir.path().join("share_queue");

        // Two payloads queued: one will get 202 (delivered), other 400 (permanent reject).
        // We route on the request body's uuid using body_json_schema.
        let good = make_test_payload("018f4e8c-0000-7000-8000-00000000000a");
        let bad = make_test_payload("018f4e8c-0000-7000-8000-00000000000b");
        enqueue(&good, &queue).unwrap();
        enqueue(&bad, &queue).unwrap();
        assert_eq!(count_pending(&queue), 2);

        // Mock: accept when uuid matches `good` (body_partial_json checks a subset match)
        Mock::given(method("POST"))
            .and(path("/api/share"))
            .and(body_partial_json(serde_json::json!({ "uuid": good.uuid })))
            .respond_with(ResponseTemplate::new(202).set_body_json(serde_json::json!({
                "status": "accepted",
                "receipt_id": good.uuid.clone(),
                "timestamp": "2026-07-29T09:00:00Z",
            })))
            .mount(&server)
            .await;

        // Mock: reject when uuid matches `bad`
        Mock::given(method("POST"))
            .and(path("/api/share"))
            .and(body_partial_json(serde_json::json!({ "uuid": bad.uuid })))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "status": "rejected",
                "reason": "prompt_empty",
                "details": "prompt must be non-empty",
            })))
            .mount(&server)
            .await;

        let summary = retry_queued_uploads(&cfg, &queue).await.unwrap();
        assert_eq!(summary.delivered, 1);
        assert_eq!(summary.rejected_permanently, 1);
        assert_eq!(summary.still_pending, 0);
        assert_eq!(summary.expired, 0);
        // Both files removed (delivered + permanent reject)
        assert_eq!(count_pending(&queue), 0);
    }

    #[tokio::test]
    async fn retry_queue_keeps_transient_failures() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        let cfg = make_config(format!("{}/api/share", server.uri()));

        let dir = tempfile::tempdir().unwrap();
        let queue = dir.path().join("share_queue");
        let payload = make_test_payload("018f4e8c-0000-7000-8000-00000000000c");
        enqueue(&payload, &queue).unwrap();

        Mock::given(method("POST"))
            .and(path("/api/share"))
            .respond_with(ResponseTemplate::new(503).set_body_string("temporarily down"))
            .mount(&server)
            .await;

        let summary = retry_queued_uploads(&cfg, &queue).await.unwrap();
        assert_eq!(summary.delivered, 0);
        assert_eq!(summary.still_pending, 1);
        assert_eq!(summary.rejected_permanently, 0);
        assert_eq!(count_pending(&queue), 1);
    }

    #[tokio::test]
    async fn retry_queue_missing_dir_returns_empty_summary() {
        let cfg = ShareConfig::default();
        let dir = tempfile::tempdir().unwrap();
        let nonexistent = dir.path().join("never-created");
        let summary = retry_queued_uploads(&cfg, &nonexistent).await.unwrap();
        assert_eq!(summary.delivered, 0);
        assert_eq!(summary.still_pending, 0);
        assert_eq!(summary.rejected_permanently, 0);
        assert_eq!(summary.expired, 0);
    }

    // ---------- Existing tests ----------

    #[test]
    fn payload_roundtrip_preserves_quality_signals() {
        let p = SharePayload::from_inputs(ShareInputs {
            uuid: "018f4e8c-0000-7000-8000-000000000000",
            schema_version: "1",
            prompt: "a cube 10mm",
            prompt_lang: "en",
            llm_model: "qwen3.5-4b-q4km",
            lol_source: "cube(10.0)",
            lol_sha256: "a".repeat(64).as_str(),
            mesh_sha256: "b".repeat(64).as_str(),
            success: true,
            retry_count: 2,
            time_to_file_ms: 7890,
            safety_violations: vec!["overhang_65deg".to_string()],
            export_format: "3mf",
            user_kept: true,
            user_edited: false,
        });

        let json = serde_json::to_string(&p).unwrap();
        let back: SharePayload = serde_json::from_str(&json).unwrap();
        assert_eq!(back.quality.retry_count, 2);
        assert_eq!(back.quality.time_to_file_ms, 7890);
        assert_eq!(back.quality.safety_violations, vec!["overhang_65deg"]);
        assert!(back.quality.user_kept);
        assert!(!back.quality.user_edited);
        assert_eq!(back.quality.export_format, "3mf");
    }
}
