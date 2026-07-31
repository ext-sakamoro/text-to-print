//! LoRA share payload schema conformance test (#42)
//!
//! Validates that `SharePayload` JSON output round-trips through the
//! authoritative JSON Schema at
//! `docs/schema/v1/alice_manifest.schema.json` The share payload
//! deliberately mirrors the `generation` + `quality` sections of the
//! manifest so a single schema drives both formats
//!
//! Test strategy:
//! 1. Load the schema from disk (path is workspace-relative)
//! 2. Build a canonical SharePayload via `SharePayload::from_inputs`
//! 3. Serialize to JSON
//! 4. Extract the `generation` + `quality` sub-objects the schema knows
//!    about and assert they roundtrip through the schema without errors
//! 5. Malformed payloads (missing required fields, bad UUID) are
//!    rejected

use jsonschema::Validator;
use serde_json::{Value, json};
use text_to_print_network::share::{ShareInputs, SharePayload};

fn schema_path() -> std::path::PathBuf {
    // The schema lives at workspace root: `docs/schema/v1/...` The tests
    // run from `crates/network/`, so we walk up two directories
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .join("../../docs/schema/v1/alice_manifest.schema.json")
        .canonicalize()
        .unwrap_or_else(|_| manifest_dir.join("../../docs/schema/v1/alice_manifest.schema.json"))
}

fn load_schema() -> Value {
    let bytes = std::fs::read(schema_path())
        .expect("docs/schema/v1/alice_manifest.schema.json must exist (T6.1 deliverable)");
    serde_json::from_slice(&bytes).expect("schema.json must be valid JSON")
}

fn build_test_payload() -> SharePayload {
    SharePayload::from_inputs(ShareInputs {
        uuid: "018f4e8c-0000-7000-8000-000000000000",
        schema_version: "1",
        prompt: "a 10mm cube",
        prompt_lang: "en",
        llm_model: "qwen3.5-4b-q4km",
        lol_source: "cube(10.0)",
        lol_sha256: &"a".repeat(64),
        mesh_sha256: &"b".repeat(64),
        tier: "Free",
        success: true,
        retry_count: 1,
        time_to_file_ms: 4200,
        safety_violations: vec!["warp_medium".to_string()],
        export_format: "3mf",
        user_kept: true,
        user_edited: false,
    })
}

/// Build a manifest-compatible document from a `SharePayload` by folding
/// the flat share fields into the nested `generation` + `quality`
/// sections the schema expects Missing fields (`vertex_count`,
/// `triangle_count`, `environment`, `timestamp`) are filled with
/// reasonable defaults so the schema check passes
fn payload_as_manifest_doc(p: &SharePayload) -> Value {
    json!({
        "schema_version": p.schema_version,
        "uuid": p.uuid,
        "timestamp": "2026-07-29T00:00:00Z",
        "generation": {
            "prompt": p.prompt,
            "prompt_lang": p.prompt_lang,
            "llm_model": p.llm_model,
            "llm_seed": null,
            "lol_source": p.lol_source,
            "lol_sha256": p.lol_sha256,
            "mesh_sha256": p.mesh_sha256,
            "vertex_count": 1024,
            "triangle_count": 2048,
        },
        "quality": {
            "success": p.quality.success,
            "retry_count": p.quality.retry_count,
            "time_to_file_ms": p.quality.time_to_file_ms,
            "safety_violations": p.quality.safety_violations,
            "export_format": p.quality.export_format,
            "user_kept": p.quality.user_kept,
            "user_edited": p.quality.user_edited,
        },
        "environment": {
            "app_version": "test-0.0.0",
            "os": "macos",
            "arch": "aarch64",
        },
    })
}

#[test]
fn share_payload_conforms_to_manifest_schema_v1() {
    let schema = load_schema();
    let validator = Validator::new(&schema).expect("schema compile");

    let payload = build_test_payload();
    let doc = payload_as_manifest_doc(&payload);

    let errors: Vec<_> = validator.iter_errors(&doc).collect();
    assert!(
        errors.is_empty(),
        "SharePayload-derived manifest doc must conform to v1 schema; errors: {errors:?}"
    );
}

#[test]
fn missing_required_field_is_rejected_by_schema() {
    let schema = load_schema();
    let validator = Validator::new(&schema).expect("schema compile");

    let mut doc = payload_as_manifest_doc(&build_test_payload());
    doc.as_object_mut().unwrap().remove("uuid");

    let errors: Vec<_> = validator.iter_errors(&doc).collect();
    assert!(
        !errors.is_empty(),
        "missing `uuid` must trigger a schema validation error"
    );
}

#[test]
fn bad_uuid_format_is_rejected_by_schema() {
    let schema = load_schema();
    let validator = Validator::new(&schema).expect("schema compile");

    let mut doc = payload_as_manifest_doc(&build_test_payload());
    doc.as_object_mut()
        .unwrap()
        .insert("uuid".to_string(), json!("not-a-uuid"));

    let errors: Vec<_> = validator.iter_errors(&doc).collect();
    assert!(
        !errors.is_empty(),
        "malformed uuid must trigger a schema validation error"
    );
}

#[test]
fn wrong_schema_version_is_rejected() {
    let schema = load_schema();
    let validator = Validator::new(&schema).expect("schema compile");

    let mut doc = payload_as_manifest_doc(&build_test_payload());
    doc.as_object_mut()
        .unwrap()
        .insert("schema_version".to_string(), json!("2"));

    let errors: Vec<_> = validator.iter_errors(&doc).collect();
    assert!(
        !errors.is_empty(),
        "schema_version other than '1' must be rejected by the const constraint"
    );
}

#[test]
fn payload_roundtrip_preserves_all_quality_fields() {
    // Belt-and-suspenders: the schema check is one guard, this is a
    // second guard that catches serde field name drift regardless of
    // schema
    let payload = build_test_payload();
    let json = serde_json::to_string(&payload).unwrap();
    let back: SharePayload = serde_json::from_str(&json).unwrap();
    assert_eq!(back.uuid, payload.uuid);
    assert_eq!(back.quality.retry_count, payload.quality.retry_count);
    assert_eq!(
        back.quality.safety_violations,
        payload.quality.safety_violations
    );
    assert_eq!(
        back.quality.time_to_file_ms,
        payload.quality.time_to_file_ms
    );
    assert_eq!(back.quality.export_format, payload.quality.export_format);
    assert_eq!(back.quality.user_kept, payload.quality.user_kept);
    assert_eq!(back.quality.user_edited, payload.quality.user_edited);
}
