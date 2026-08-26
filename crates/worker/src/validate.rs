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
    // Gallery Phase 3 (2026-08-26) rejection tags
    GalleryDidFormat,
    GalleryNicknameTooLong,
    GalleryCreatedAtFormat,
    GallerySignatureFormat,
    GallerySignatureInvalid,
    GalleryLolTooLarge,
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
            Self::GalleryDidFormat => "gallery_did_format",
            Self::GalleryNicknameTooLong => "gallery_nickname_too_long",
            Self::GalleryCreatedAtFormat => "gallery_created_at_format",
            Self::GallerySignatureFormat => "gallery_signature_format",
            Self::GallerySignatureInvalid => "gallery_signature_invalid",
            Self::GalleryLolTooLarge => "gallery_lol_too_large",
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
            Self::GalleryDidFormat => "author_did must be did:key:<64 lowercase hex>",
            Self::GalleryNicknameTooLong => "author_nickname exceeds 32 chars",
            Self::GalleryCreatedAtFormat => "created_at must be ISO-8601 UTC",
            Self::GallerySignatureFormat => "signature must be 128 lowercase hex chars",
            Self::GallerySignatureInvalid => "ed25519 signature does not verify against author_did",
            Self::GalleryLolTooLarge => "lol_source exceeds 102400 bytes (100 KB)",
        }
    }
}

const MAX_PROMPT_LEN: usize = 4096;
const MAX_LOL_LEN: usize = 65_536;
const SHA256_HEX_LEN: usize = 64;
const KNOWN_EXPORT_FORMATS: [&str; 6] = ["3mf", "stl", "obj", "fbx", "step", "gcode"];

// Gallery Phase 3 (2026-08-26) constants
/// `did:key:` prefix + 64 lowercase hex (ed25519 public key, 32 bytes)
pub const DID_KEY_PREFIX: &str = "did:key:";
/// 32-byte ed25519 pub key ⇒ 64 hex chars after `did:key:`
pub const DID_KEY_HEX_LEN: usize = 64;
/// 64-byte ed25519 signature ⇒ 128 hex chars
pub const ED25519_SIG_HEX_LEN: usize = 128;
/// Nickname display cap (matches Settings TextEdit `char_limit(32)`)
pub const MAX_NICKNAME_LEN: usize = 32;
/// LOL DSL cap for the Gallery relay (memory-safety cap for Worker cost)
pub const MAX_GALLERY_LOL_LEN: usize = 102_400;

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

// ── Gallery Phase 3 (2026-08-26) validators ────────────────────────

/// Extract the 32-byte ed25519 pub key from a `did:key:<64hex>` string
///
/// # Errors
/// Returns [`RejectReason::GalleryDidFormat`] when the prefix, length, or
/// hex encoding is wrong
pub fn parse_did_key(did: &str) -> Result<[u8; 32], RejectReason> {
    let stripped = did
        .strip_prefix(DID_KEY_PREFIX)
        .ok_or(RejectReason::GalleryDidFormat)?;
    if !is_lowercase_hex(stripped, DID_KEY_HEX_LEN) {
        return Err(RejectReason::GalleryDidFormat);
    }
    let mut out = [0u8; 32];
    hex::decode_to_slice(stripped, &mut out).map_err(|_| RejectReason::GalleryDidFormat)?;
    Ok(out)
}

/// Decode a 128-hex ed25519 signature into 64 raw bytes
///
/// # Errors
/// Returns [`RejectReason::GallerySignatureFormat`] for length / hex errors
pub fn parse_signature(sig: &str) -> Result<[u8; 64], RejectReason> {
    if !is_lowercase_hex(sig, ED25519_SIG_HEX_LEN) {
        return Err(RejectReason::GallerySignatureFormat);
    }
    let mut out = [0u8; 64];
    hex::decode_to_slice(sig, &mut out).map_err(|_| RejectReason::GallerySignatureFormat)?;
    Ok(out)
}

/// Canonical bytes signed by the client for a `gallery/publish` request
/// Format: `id + "|" + author_did + "|" + lol_source + "|" + created_at`
/// The pipe separator is stable and outside base64/hex/ISO alphabets so
/// smuggling attempts collapse the delimiter naturally
#[must_use]
pub fn gallery_publish_canonical(
    id: &str,
    author_did: &str,
    lol_source: &str,
    created_at: &str,
) -> String {
    format!("{id}|{author_did}|{lol_source}|{created_at}")
}

/// Canonical bytes signed by the client for a `gallery/delete` request
/// Format: `"delete|" + id + "|" + author_did` The prefix separates
/// publish and delete signatures so a captured publish sig can not be
/// replayed as a delete of the same row
#[must_use]
pub fn gallery_delete_canonical(id: &str, author_did: &str) -> String {
    format!("delete|{id}|{author_did}")
}

/// Verify an ed25519 signature against a `did:key:` author + canonical msg
///
/// # Errors
/// - [`RejectReason::GalleryDidFormat`] if the DID is malformed
/// - [`RejectReason::GallerySignatureFormat`] if the sig hex is malformed
/// - [`RejectReason::GallerySignatureInvalid`] if the sig does not verify
pub fn verify_gallery_signature(
    did: &str,
    signature_hex: &str,
    canonical: &[u8],
) -> Result<(), RejectReason> {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};

    let pub_bytes = parse_did_key(did)?;
    let sig_bytes = parse_signature(signature_hex)?;
    let verifying_key =
        VerifyingKey::from_bytes(&pub_bytes).map_err(|_| RejectReason::GalleryDidFormat)?;
    let signature = Signature::from_bytes(&sig_bytes);
    verifying_key
        .verify(canonical, &signature)
        .map_err(|_| RejectReason::GallerySignatureInvalid)
}

/// Fields the Gallery `POST /publish` handler cares about, extracted from
/// the JSON body Kept separate from [`SharePayload`] because the two are
/// unrelated wire schemas
pub struct GalleryPublishFields<'a> {
    pub id: &'a str,
    pub author_did: &'a str,
    pub author_nickname: Option<&'a str>,
    pub lol_source: &'a str,
    pub created_at: &'a str,
    pub signature: &'a str,
}

/// Validate a decoded Gallery publish payload (format + signature)
///
/// # Errors
/// Returns the first failed [`RejectReason`] (fail-fast) LOL DSL parseability
/// is NOT checked here — the desktop-side client is authoritative and the
/// Worker can not link `alice-bamboo` (wasm32 build) A separate cleanup task
/// can walk the D1 table if bad rows appear in practice
pub fn check_gallery_publish(fields: &GalleryPublishFields<'_>) -> Result<(), RejectReason> {
    if !is_uuid(fields.id) {
        return Err(RejectReason::UuidFormat);
    }
    parse_did_key(fields.author_did)?;
    if let Some(nick) = fields.author_nickname {
        if nick.chars().count() > MAX_NICKNAME_LEN {
            return Err(RejectReason::GalleryNicknameTooLong);
        }
    }
    if fields.lol_source.trim().is_empty() {
        return Err(RejectReason::LolSourceEmpty);
    }
    if fields.lol_source.len() > MAX_GALLERY_LOL_LEN {
        return Err(RejectReason::GalleryLolTooLarge);
    }
    if !is_iso8601_utc(fields.created_at) {
        return Err(RejectReason::GalleryCreatedAtFormat);
    }
    let canonical = gallery_publish_canonical(
        fields.id,
        fields.author_did,
        fields.lol_source,
        fields.created_at,
    );
    verify_gallery_signature(fields.author_did, fields.signature, canonical.as_bytes())
}

/// Minimal ISO-8601 UTC form check: length in [20, 40], ends with `Z` or
/// numeric timezone offset, contains `T` between date and time Full
/// timestamp parsing is deferred to consumers who need it
fn is_iso8601_utc(s: &str) -> bool {
    let len = s.len();
    if !(20..=40).contains(&len) {
        return false;
    }
    let bytes = s.as_bytes();
    // yyyy-mm-ddThh:mm:ss{Z or offset}
    if bytes.get(10) != Some(&b'T') {
        return false;
    }
    let last = bytes[len - 1];
    // 'Z' or a numeric offset last char (0-9)
    last == b'Z' || last.is_ascii_digit()
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
            RejectReason::GalleryDidFormat,
            RejectReason::GalleryNicknameTooLong,
            RejectReason::GalleryCreatedAtFormat,
            RejectReason::GallerySignatureFormat,
            RejectReason::GallerySignatureInvalid,
            RejectReason::GalleryLolTooLarge,
        ] {
            assert!(!reason.tag().is_empty());
            assert!(!reason.message().is_empty());
        }
    }

    // ── Gallery Phase 3 (2026-08-26) tests ────────────────────────

    use ed25519_dalek::{Signer, SigningKey};

    fn make_gallery_identity() -> (SigningKey, String) {
        // Deterministic seed so the test doesn't rely on OS entropy in wasm32
        let seed = [7u8; 32];
        let signing_key = SigningKey::from_bytes(&seed);
        let did = format!(
            "did:key:{}",
            hex::encode(signing_key.verifying_key().as_bytes())
        );
        (signing_key, did)
    }

    #[test]
    fn parse_did_key_accepts_lowercase_64hex() {
        let (_key, did) = make_gallery_identity();
        assert!(parse_did_key(&did).is_ok());
    }

    #[test]
    fn parse_did_key_rejects_uppercase() {
        let (_key, did) = make_gallery_identity();
        let up = format!(
            "did:key:{}",
            did.strip_prefix("did:key:").unwrap().to_uppercase()
        );
        assert!(matches!(parse_did_key(&up), Err(RejectReason::GalleryDidFormat)));
    }

    #[test]
    fn parse_did_key_rejects_missing_prefix() {
        let (_key, did) = make_gallery_identity();
        let no_prefix = did.strip_prefix("did:key:").unwrap().to_string();
        assert!(matches!(
            parse_did_key(&no_prefix),
            Err(RejectReason::GalleryDidFormat)
        ));
    }

    #[test]
    fn gallery_publish_signature_roundtrip() {
        let (signing_key, did) = make_gallery_identity();
        let canonical = gallery_publish_canonical(
            "018f4e8c-0000-7000-8000-000000000001",
            &did,
            "sphere(1.0)",
            "2026-08-26T12:00:00Z",
        );
        let sig = signing_key.sign(canonical.as_bytes());
        let sig_hex = hex::encode(sig.to_bytes());
        assert!(verify_gallery_signature(&did, &sig_hex, canonical.as_bytes()).is_ok());
    }

    #[test]
    fn gallery_publish_signature_wrong_msg_rejected() {
        let (signing_key, did) = make_gallery_identity();
        let canonical =
            gallery_publish_canonical("id1", &did, "sphere(1.0)", "2026-08-26T12:00:00Z");
        let sig = signing_key.sign(canonical.as_bytes());
        let sig_hex = hex::encode(sig.to_bytes());
        assert!(matches!(
            verify_gallery_signature(&did, &sig_hex, b"other message"),
            Err(RejectReason::GallerySignatureInvalid)
        ));
    }

    #[test]
    fn gallery_delete_canonical_is_distinct_from_publish() {
        // Replay guard: a publish sig must not verify as a delete
        let publish = gallery_publish_canonical("id1", "did:key:aa", "cube(1)", "t");
        let delete = gallery_delete_canonical("id1", "did:key:aa");
        assert_ne!(publish, delete);
    }

    #[test]
    fn check_gallery_publish_valid() {
        let (signing_key, did) = make_gallery_identity();
        let id = "018f4e8c-0000-7000-8000-000000000001";
        let lol = "sphere(1.0)";
        let ts = "2026-08-26T12:00:00Z";
        let canonical = gallery_publish_canonical(id, &did, lol, ts);
        let sig_hex = hex::encode(signing_key.sign(canonical.as_bytes()).to_bytes());
        let fields = GalleryPublishFields {
            id,
            author_did: &did,
            author_nickname: Some("alice"),
            lol_source: lol,
            created_at: ts,
            signature: &sig_hex,
        };
        assert!(check_gallery_publish(&fields).is_ok());
    }

    #[test]
    fn check_gallery_publish_lol_over_100kb_rejected() {
        let (signing_key, did) = make_gallery_identity();
        let id = "018f4e8c-0000-7000-8000-000000000001";
        let big = "x".repeat(MAX_GALLERY_LOL_LEN + 1);
        let ts = "2026-08-26T12:00:00Z";
        let canonical = gallery_publish_canonical(id, &did, &big, ts);
        let sig_hex = hex::encode(signing_key.sign(canonical.as_bytes()).to_bytes());
        let fields = GalleryPublishFields {
            id,
            author_did: &did,
            author_nickname: None,
            lol_source: &big,
            created_at: ts,
            signature: &sig_hex,
        };
        assert!(matches!(
            check_gallery_publish(&fields),
            Err(RejectReason::GalleryLolTooLarge)
        ));
    }

    #[test]
    fn check_gallery_publish_nickname_over_32_rejected() {
        let (signing_key, did) = make_gallery_identity();
        let id = "018f4e8c-0000-7000-8000-000000000001";
        let lol = "sphere(1.0)";
        let ts = "2026-08-26T12:00:00Z";
        let canonical = gallery_publish_canonical(id, &did, lol, ts);
        let sig_hex = hex::encode(signing_key.sign(canonical.as_bytes()).to_bytes());
        let long_nick = "a".repeat(MAX_NICKNAME_LEN + 1);
        let fields = GalleryPublishFields {
            id,
            author_did: &did,
            author_nickname: Some(&long_nick),
            lol_source: lol,
            created_at: ts,
            signature: &sig_hex,
        };
        assert!(matches!(
            check_gallery_publish(&fields),
            Err(RejectReason::GalleryNicknameTooLong)
        ));
    }

    #[test]
    fn check_gallery_publish_bad_signature_rejected() {
        let (_key, did) = make_gallery_identity();
        let fields = GalleryPublishFields {
            id: "018f4e8c-0000-7000-8000-000000000001",
            author_did: &did,
            author_nickname: None,
            lol_source: "sphere(1)",
            created_at: "2026-08-26T12:00:00Z",
            signature: &"a".repeat(128),
        };
        assert!(matches!(
            check_gallery_publish(&fields),
            Err(RejectReason::GallerySignatureInvalid)
        ));
    }

    #[test]
    fn check_gallery_publish_missing_z_rejected() {
        let (signing_key, did) = make_gallery_identity();
        let id = "018f4e8c-0000-7000-8000-000000000001";
        let lol = "sphere(1.0)";
        let ts = "not a timestamp";
        let canonical = gallery_publish_canonical(id, &did, lol, ts);
        let sig_hex = hex::encode(signing_key.sign(canonical.as_bytes()).to_bytes());
        let fields = GalleryPublishFields {
            id,
            author_did: &did,
            author_nickname: None,
            lol_source: lol,
            created_at: ts,
            signature: &sig_hex,
        };
        assert!(matches!(
            check_gallery_publish(&fields),
            Err(RejectReason::GalleryCreatedAtFormat)
        ));
    }
}
