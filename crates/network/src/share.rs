use serde::{Deserialize, Serialize};

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
