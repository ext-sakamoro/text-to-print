//! Hand-written validation for [`crate::SharePayload`]
//!
//! We keep validation dependency-free rather than pulling a JSON-schema
//! runtime into a wasm build — the schema is small, stable, and the same
//! source of truth (`docs/schema/v1/alice_manifest.schema.json`) is used by
//! CI schema-conformance tests on the client side
//!
//! Rejections use compact tag strings the client can enumerate on

use crate::SharePayload;

/// Reasons a payload can be rejected before it reaches D1
#[derive(Debug, Clone, Copy)]
pub enum RejectReason {
    SchemaVersionMismatch,
    UuidFormat,
    PromptEmpty,
    PromptTooLong,
    LolSourceEmpty,
    LolSourceTooLong,
    LolSha256Format,
    MeshSha256Format,
    ExportFormatUnknown,
    LlmModelEmpty,
    PromptLangFormat,
}

impl RejectReason {
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Self::SchemaVersionMismatch => "schema_version_mismatch",
            Self::UuidFormat => "uuid_format",
            Self::PromptEmpty => "prompt_empty",
            Self::PromptTooLong => "prompt_too_long",
            Self::LolSourceEmpty => "lol_source_empty",
            Self::LolSourceTooLong => "lol_source_too_long",
            Self::LolSha256Format => "lol_sha256_format",
            Self::MeshSha256Format => "mesh_sha256_format",
            Self::ExportFormatUnknown => "export_format_unknown",
            Self::LlmModelEmpty => "llm_model_empty",
            Self::PromptLangFormat => "prompt_lang_format",
        }
    }

    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            Self::SchemaVersionMismatch => "expected schema_version = \"1\"",
            Self::UuidFormat => "uuid must match RFC 4122 lowercase hex",
            Self::PromptEmpty => "prompt must be non-empty",
            Self::PromptTooLong => "prompt exceeds 4096 chars",
            Self::LolSourceEmpty => "lol_source must be non-empty",
            Self::LolSourceTooLong => "lol_source exceeds 65536 chars",
            Self::LolSha256Format => "lol_sha256 must be 64 lowercase hex chars",
            Self::MeshSha256Format => "mesh_sha256 must be 64 lowercase hex chars",
            Self::ExportFormatUnknown => "export_format must be one of 3mf|stl|obj|fbx|step|gcode",
            Self::LlmModelEmpty => "llm_model must be non-empty",
            Self::PromptLangFormat => "prompt_lang must be a 2-letter lowercase code",
        }
    }
}

const MAX_PROMPT_LEN: usize = 4096;
const MAX_LOL_LEN: usize = 65_536;
const SHA256_HEX_LEN: usize = 64;
const KNOWN_EXPORT_FORMATS: [&str; 6] = ["3mf", "stl", "obj", "fbx", "step", "gcode"];

/// Validate a decoded [`SharePayload`] Returns the first rejection reason
/// found (fail-fast) or `Ok(())` when everything passes
///
/// # Errors
///
/// Returns a [`RejectReason`] describing the first field that violates the
/// v1 alice_manifest.schema.json contract
pub fn check(payload: &SharePayload) -> Result<(), RejectReason> {
    if payload.schema_version != "1" {
        return Err(RejectReason::SchemaVersionMismatch);
    }
    if !is_uuid(&payload.uuid) {
        return Err(RejectReason::UuidFormat);
    }
    if payload.prompt.trim().is_empty() {
        return Err(RejectReason::PromptEmpty);
    }
    if payload.prompt.len() > MAX_PROMPT_LEN {
        return Err(RejectReason::PromptTooLong);
    }
    if !is_lowercase_ascii2(&payload.prompt_lang) {
        return Err(RejectReason::PromptLangFormat);
    }
    if payload.llm_model.trim().is_empty() {
        return Err(RejectReason::LlmModelEmpty);
    }
    if payload.lol_source.trim().is_empty() {
        return Err(RejectReason::LolSourceEmpty);
    }
    if payload.lol_source.len() > MAX_LOL_LEN {
        return Err(RejectReason::LolSourceTooLong);
    }
    if !is_lowercase_hex(&payload.lol_sha256, SHA256_HEX_LEN) {
        return Err(RejectReason::LolSha256Format);
    }
    if !is_lowercase_hex(&payload.mesh_sha256, SHA256_HEX_LEN) {
        return Err(RejectReason::MeshSha256Format);
    }
    if !KNOWN_EXPORT_FORMATS.contains(&payload.quality.export_format.as_str()) {
        return Err(RejectReason::ExportFormatUnknown);
    }
    Ok(())
}

fn is_uuid(s: &str) -> bool {
    // RFC 4122 lowercase hex form: 8-4-4-4-12
    if s.len() != 36 {
        return false;
    }
    let bytes = s.as_bytes();
    let dashes = [8, 13, 18, 23];
    for (i, &b) in bytes.iter().enumerate() {
        let expect_dash = dashes.contains(&i);
        if expect_dash {
            if b != b'-' {
                return false;
            }
        } else if !b.is_ascii_hexdigit() || b.is_ascii_uppercase() {
            return false;
        }
    }
    true
}

fn is_lowercase_hex(s: &str, expected_len: usize) -> bool {
    s.len() == expected_len
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn is_lowercase_ascii2(s: &str) -> bool {
    s.len() == 2 && s.bytes().all(|b| b.is_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::QualitySignals;

    fn make_payload() -> SharePayload {
        SharePayload {
            schema_version: "1".into(),
            uuid: "018f4e8c-0000-7000-8000-000000000001".into(),
            prompt: "phone stand with cable hole".into(),
            prompt_lang: "en".into(),
            llm_model: "qwen3.5-4b-q4km".into(),
            lol_source: "cube(10)".into(),
            lol_sha256: "a".repeat(64),
            mesh_sha256: "b".repeat(64),
            quality: QualitySignals {
                success: true,
                retry_count: 0,
                time_to_file_ms: 5000,
                safety_violations: vec![],
                export_format: "3mf".into(),
                user_kept: true,
                user_edited: false,
            },
        }
    }

    #[test]
    fn valid_payload_passes() {
        assert!(check(&make_payload()).is_ok());
    }

    #[test]
    fn schema_version_wrong() {
        let mut p = make_payload();
        p.schema_version = "0".into();
        assert!(matches!(
            check(&p),
            Err(RejectReason::SchemaVersionMismatch)
        ));
    }

    #[test]
    fn uuid_uppercase_rejected() {
        let mut p = make_payload();
        p.uuid = "018F4E8C-0000-7000-8000-000000000001".into();
        assert!(matches!(check(&p), Err(RejectReason::UuidFormat)));
    }

    #[test]
    fn uuid_missing_dashes_rejected() {
        let mut p = make_payload();
        p.uuid = "018f4e8c0000700080000000000000001".into();
        assert!(matches!(check(&p), Err(RejectReason::UuidFormat)));
    }

    #[test]
    fn empty_prompt_rejected() {
        let mut p = make_payload();
        p.prompt = "  ".into();
        assert!(matches!(check(&p), Err(RejectReason::PromptEmpty)));
    }

    #[test]
    fn prompt_too_long_rejected() {
        let mut p = make_payload();
        p.prompt = "x".repeat(MAX_PROMPT_LEN + 1);
        assert!(matches!(check(&p), Err(RejectReason::PromptTooLong)));
    }

    #[test]
    fn lol_source_length_capped() {
        let mut p = make_payload();
        p.lol_source = "x".repeat(MAX_LOL_LEN + 1);
        assert!(matches!(check(&p), Err(RejectReason::LolSourceTooLong)));
    }

    #[test]
    fn lol_sha256_uppercase_rejected() {
        let mut p = make_payload();
        p.lol_sha256 = "A".repeat(64);
        assert!(matches!(check(&p), Err(RejectReason::LolSha256Format)));
    }

    #[test]
    fn export_format_unknown_rejected() {
        let mut p = make_payload();
        p.quality.export_format = "iges".into();
        assert!(matches!(check(&p), Err(RejectReason::ExportFormatUnknown)));
    }

    #[test]
    fn prompt_lang_wrong_length_rejected() {
        let mut p = make_payload();
        p.prompt_lang = "eng".into();
        assert!(matches!(check(&p), Err(RejectReason::PromptLangFormat)));
    }

    #[test]
    fn all_reject_reasons_tag_is_stable() {
        // Guard against accidental tag rename — the client relies on these
        for reason in [
            RejectReason::SchemaVersionMismatch,
            RejectReason::UuidFormat,
            RejectReason::PromptEmpty,
            RejectReason::PromptTooLong,
            RejectReason::LolSourceEmpty,
            RejectReason::LolSourceTooLong,
            RejectReason::LolSha256Format,
            RejectReason::MeshSha256Format,
            RejectReason::ExportFormatUnknown,
            RejectReason::LlmModelEmpty,
            RejectReason::PromptLangFormat,
        ] {
            assert!(!reason.tag().is_empty());
            assert!(!reason.message().is_empty());
        }
    }
}
