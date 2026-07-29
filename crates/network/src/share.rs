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
