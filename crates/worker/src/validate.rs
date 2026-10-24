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

/// The bytes the client signs for a `gallery/publish` request: every field of
/// the request, see [`text_to_print_gallery_sign::publish_message`]
///
/// # Errors
/// [`RejectReason::GallerySignatureInvalid`] if the message can not be encoded
/// (no signature can cover it)
pub fn gallery_publish_message(fields: &GalleryPublishFields<'_>) -> Result<Vec<u8>, RejectReason> {
    text_to_print_gallery_sign::publish_message(&text_to_print_gallery_sign::PublishFields {
        id: fields.id,
        author_did: fields.author_did,
        author_nickname: fields.author_nickname,
        lol_source: fields.lol_source,
        created_at: fields.created_at,
    })
    .map_err(|_| RejectReason::GallerySignatureInvalid)
}

/// The bytes the client signs for a `gallery/delete` request; its domain differs
/// from publish, so a captured publish signature is never a delete signature
///
/// # Errors
/// [`RejectReason::GallerySignatureInvalid`] if the message can not be encoded
pub fn gallery_delete_message(id: &str, author_did: &str) -> Result<Vec<u8>, RejectReason> {
    text_to_print_gallery_sign::delete_message(id, author_did)
        .map_err(|_| RejectReason::GallerySignatureInvalid)
}

/// Verify a strict ed25519 signature by a `did:key:` author over `message`
///
/// # Errors
/// - [`RejectReason::GalleryDidFormat`] if the DID is malformed
/// - [`RejectReason::GallerySignatureFormat`] if the sig hex is malformed
/// - [`RejectReason::GallerySignatureInvalid`] if the sig does not verify
pub fn verify_gallery_signature(
    did: &str,
    signature_hex: &str,
    message: &[u8],
) -> Result<(), RejectReason> {
    let public_key = parse_did_key(did)?;
    let signature = parse_signature(signature_hex)?;
    if text_to_print_gallery_sign::verify(public_key, message, signature) {
        Ok(())
    } else {
        Err(RejectReason::GallerySignatureInvalid)
    }
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
    if !is_rfc3339_timestamp(fields.created_at) {
        return Err(RejectReason::GalleryCreatedAtFormat);
    }
    let message = gallery_publish_message(fields)?;
    verify_gallery_signature(fields.author_did, fields.signature, &message)
}

/// An RFC 3339 timestamp: `YYYY-MM-DDTHH:MM:SS`, an optional fraction of 1 to 9
/// digits, then `Z` or `+HH:MM` / `-HH:MM`, with every field in range (the day
/// within its month, leap years included) Anything else is refused, so the stored
/// value is always one a reader can parse
fn is_rfc3339_timestamp(s: &str) -> bool {
    fn num(b: &[u8]) -> Option<u32> {
        if b.is_empty() || !b.iter().all(u8::is_ascii_digit) {
            return None;
        }
        Some(b.iter().fold(0, |n, d| n * 10 + u32::from(d - b'0')))
    }
    let b = s.as_bytes();
    if b.len() < 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
    {
        return false;
    }
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (
        num(&b[0..4]),
        num(&b[5..7]),
        num(&b[8..10]),
        num(&b[11..13]),
        num(&b[14..16]),
        num(&b[17..19]),
    ) else {
        return false;
    };
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let month_days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    // second 60 is a leap second, which RFC 3339 allows
    if !(1..=month_days).contains(&day) || hour > 23 || minute > 59 || second > 60 {
        return false;
    }
    let mut rest = &b[19..];
    if let Some(frac) = rest.strip_prefix(b".") {
        let digits = frac.iter().take_while(|c| c.is_ascii_digit()).count();
        if !(1..=9).contains(&digits) {
            return false;
        }
        rest = &frac[digits..];
    }
    match rest {
        b"Z" => true,
        [sign, h1, h2, b':', m1, m2] if *sign == b'+' || *sign == b'-' => {
            matches!((num(&[*h1, *h2]), num(&[*m1, *m2])), (Some(h), Some(m)) if h <= 23 && m <= 59)
        }
        _ => false,
    }
}

#[cfg(test)]
#[path = "validate_signature_scope_tests.rs"]
mod signature_scope_tests;

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
                printability: None,
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

    use ed25519_dalek::SigningKey;

    fn sign_message(key: &SigningKey, message: &[u8]) -> String {
        hex::encode(text_to_print_gallery_sign::sign(key.to_bytes(), message))
    }

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
        assert!(matches!(
            parse_did_key(&up),
            Err(RejectReason::GalleryDidFormat)
        ));
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
        let message = gallery_delete_message("018f4e8c-0000-7000-8000-000000000001", &did).unwrap();
        let sig_hex = sign_message(&signing_key, &message);
        assert!(verify_gallery_signature(&did, &sig_hex, &message).is_ok());
    }

    #[test]
    fn gallery_publish_signature_wrong_msg_rejected() {
        let (signing_key, did) = make_gallery_identity();
        let message = gallery_delete_message("id1", &did).unwrap();
        let sig_hex = sign_message(&signing_key, &message);
        assert!(matches!(
            verify_gallery_signature(&did, &sig_hex, b"other message"),
            Err(RejectReason::GallerySignatureInvalid)
        ));
    }

    #[test]
    fn a_publish_signature_is_not_a_delete_signature() {
        // Replay guard: a publish sig must not verify as a delete of the same row
        let (signing_key, did) = make_gallery_identity();
        let id = "018f4e8c-0000-7000-8000-000000000001";
        let publish = gallery_publish_message(&GalleryPublishFields {
            id,
            author_did: &did,
            author_nickname: None,
            lol_source: "cube(1)",
            created_at: "2026-08-26T12:00:00Z",
            signature: "",
        })
        .unwrap();
        let sig_hex = sign_message(&signing_key, &publish);
        let delete = gallery_delete_message(id, &did).unwrap();
        assert!(matches!(
            verify_gallery_signature(&did, &sig_hex, &delete),
            Err(RejectReason::GallerySignatureInvalid)
        ));
    }

    #[test]
    fn check_gallery_publish_valid() {
        let (signing_key, did) = make_gallery_identity();
        let id = "018f4e8c-0000-7000-8000-000000000001";
        let lol = "sphere(1.0)";
        let ts = "2026-08-26T12:00:00Z";
        let unsigned = GalleryPublishFields {
            id,
            author_did: &did,
            author_nickname: Some("alice"),
            lol_source: lol,
            created_at: ts,
            signature: "",
        };
        let sig_hex = sign_message(&signing_key, &gallery_publish_message(&unsigned).unwrap());
        let fields = GalleryPublishFields {
            signature: &sig_hex,
            ..unsigned
        };
        assert!(check_gallery_publish(&fields).is_ok());
    }

    #[test]
    fn check_gallery_publish_lol_over_100kb_rejected() {
        let (signing_key, did) = make_gallery_identity();
        let id = "018f4e8c-0000-7000-8000-000000000001";
        let big = "x".repeat(MAX_GALLERY_LOL_LEN + 1);
        let ts = "2026-08-26T12:00:00Z";
        let unsigned = GalleryPublishFields {
            id,
            author_did: &did,
            author_nickname: None,
            lol_source: &big,
            created_at: ts,
            signature: "",
        };
        let sig_hex = sign_message(&signing_key, &gallery_publish_message(&unsigned).unwrap());
        let fields = GalleryPublishFields {
            signature: &sig_hex,
            ..unsigned
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
        let long_nick = "a".repeat(MAX_NICKNAME_LEN + 1);
        let unsigned = GalleryPublishFields {
            id,
            author_did: &did,
            author_nickname: Some(&long_nick),
            lol_source: lol,
            created_at: ts,
            signature: "",
        };
        let sig_hex = sign_message(&signing_key, &gallery_publish_message(&unsigned).unwrap());
        let fields = GalleryPublishFields {
            signature: &sig_hex,
            ..unsigned
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
        let unsigned = GalleryPublishFields {
            id,
            author_did: &did,
            author_nickname: None,
            lol_source: lol,
            created_at: ts,
            signature: "",
        };
        let sig_hex = sign_message(&signing_key, &gallery_publish_message(&unsigned).unwrap());
        let fields = GalleryPublishFields {
            signature: &sig_hex,
            ..unsigned
        };
        assert!(matches!(
            check_gallery_publish(&fields),
            Err(RejectReason::GalleryCreatedAtFormat)
        ));
    }

    #[test]
    fn rfc3339_timestamps_the_client_sends_are_accepted() {
        for ok in [
            "2026-08-26T12:00:00Z",
            "2026-10-25T00:14:28.123456789+00:00",
            "2026-10-25T09:14:28+09:00",
            "2024-02-29T23:59:60.5-05:30",
        ] {
            assert!(is_rfc3339_timestamp(ok), "{ok}");
        }
    }

    #[test]
    fn malformed_or_out_of_range_timestamps_are_refused() {
        for bad in [
            "2026-01-01T|2026-10-24T00:00:00Z",
            "2026-10-24T00:00:00Zjunk",
            "2026-10-24T00:00:00",
            "2026-10-24 00:00:00Z",
            "2026-13-01T00:00:00Z",
            "2026-02-29T00:00:00Z",
            "2026-04-31T00:00:00Z",
            "2026-10-00T00:00:00Z",
            "2026-10-24T24:00:00Z",
            "2026-10-24T00:60:00Z",
            "2026-10-24T00:00:61Z",
            "2026-10-24T00:00:00.Z",
            "2026-10-24T00:00:00.1234567890Z",
            "2026-10-24T00:00:00+24:00",
            "2026-10-24T00:00:00+09:60",
            "2026-10-24T00:00:00+0900",
            "+026-10-24T00:00:00Z",
            "2026-1a-24T00:00:00Z",
        ] {
            assert!(!is_rfc3339_timestamp(bad), "{bad}");
        }
    }
}
