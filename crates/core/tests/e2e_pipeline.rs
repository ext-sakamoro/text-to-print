//! End-to-end pipeline integration test (#41)
//!
//! Exercises the full `text prompt → LOL DSL → mesh → 3MF` path without
//! going through a real LLM by using a hard-coded "LLM response" that
//! embeds a valid LOL fenced code block The rest of the pipeline runs
//! for real: `pipeline::extract_lol` → `pipeline::validate_lol` →
//! `pipeline::export_mesh` (3MF via `alice-bamboo`) → 3MF ZIP contents
//! sanity check
//!
//! This satisfies the P1 品質 gate around the flywheel described in the
//! Stage 5 flywheel plan — any regression that breaks 3MF generation or
//! embedded manifest will surface here rather than at real-user runtime

use std::io::Read;
use text_to_print_core::manifest::AliceManifest;
use text_to_print_core::pipeline::{self, ExportFormat, MetadataInputs, Quality};

/// Minimal LOL DSL that produces a solid, printable SDF for the mesh
/// export path This mirrors what a well-behaved LLM would output for
/// prompt "20mm sphere"
const MOCK_LLM_RESPONSE: &str = "\
Here is your sphere model:

```lol
sphere(10.0)
```

Enjoy!";

/// Alternate response with no fenced block — used to exercise the
/// `extract_lol` fallback path
const RAW_LOL_RESPONSE: &str = "sphere(15.0)";

#[test]
fn mock_llm_response_yields_valid_3mf() {
    let dir = tempfile::tempdir().expect("tempdir");

    // 1. LLM response → LOL DSL
    let lol = pipeline::extract_lol(MOCK_LLM_RESPONSE)
        .expect("mock response should contain a lol fenced block");
    assert_eq!(lol, "sphere(10.0)");

    // 2. LOL validate
    pipeline::validate_lol(&lol).expect("sphere(10) should parse");

    // 3. Export 3MF via alice-bamboo pipeline
    let stats = pipeline::export_mesh(&lol, dir.path(), ExportFormat::ThreeMf, Quality::Preview)
        .expect("3MF export should succeed");

    assert!(stats.vertex_count > 0, "mesh should have vertices");
    assert!(stats.triangle_count > 0, "mesh should have triangles");
    assert!(stats.path.ends_with(".3mf"), "output should be a .3mf");

    // 4. alice-bamboo safety + overhang summaries should be populated
    assert!(
        stats.safety_summary.is_some(),
        "3MF export should populate SafetySummary"
    );
    assert!(
        stats.overhang_summary.is_some(),
        "3MF export should populate OverhangSummary"
    );
    let safety = stats.safety_summary.as_ref().unwrap();
    assert_eq!(safety.material_name, "PLA");

    // 5. 3MF file is a valid ZIP archive containing at least
    //    `3D/3dmodel.model`
    let bytes = std::fs::read(&stats.path).expect("read 3MF bytes");
    let mut ar = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("open 3MF as ZIP");
    let mut model = ar
        .by_name("3D/3dmodel.model")
        .expect("3MF must contain 3D/3dmodel.model");
    let mut xml = String::new();
    model.read_to_string(&mut xml).expect("read model xml");
    assert!(xml.contains("<model"), "3dmodel.model should be XML model");
}

#[test]
fn raw_response_without_fence_falls_through_extract_lol() {
    // `extract_lol` returns None; the app path treats the raw response
    // as the LOL source This test proves the fallback still validates
    let extracted = pipeline::extract_lol(RAW_LOL_RESPONSE);
    assert!(extracted.is_none());

    // Downstream would use the raw string as-is
    pipeline::validate_lol(RAW_LOL_RESPONSE).expect("raw sphere(15) should parse");
}

#[test]
fn e2e_export_with_metadata_embeds_manifest_and_matches_stats() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lol = pipeline::extract_lol(MOCK_LLM_RESPONSE).expect("extract");

    let meta = MetadataInputs {
        prompt: "20mm sphere",
        prompt_lang: "en",
        llm_model: "mock-model",
        llm_seed: Some(42),
        retry_count: 0,
        safety_violations: vec![],
    };
    let (stats, manifest) = pipeline::export_mesh_with_metadata(
        &lol,
        dir.path(),
        ExportFormat::ThreeMf,
        Quality::Preview,
        meta,
    )
    .expect("metadata export");

    assert_eq!(manifest.schema_version, "1");
    assert_eq!(manifest.generation.vertex_count, stats.vertex_count);
    assert_eq!(manifest.generation.triangle_count, stats.triangle_count);
    assert_eq!(manifest.quality.export_format, "3mf");
    assert_eq!(manifest.quality.retry_count, 0);

    // The 3MF should have the manifest embedded at Metadata/alice_manifest.json
    let bytes = std::fs::read(&stats.path).expect("read 3MF");
    let mut ar = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("open ZIP");
    let mut man_entry = ar
        .by_name("Metadata/alice_manifest.json")
        .expect("manifest must be embedded for 3MF");
    let mut json = String::new();
    man_entry
        .read_to_string(&mut json)
        .expect("read manifest json");
    let parsed: AliceManifest = serde_json::from_str(&json).expect("parse embedded manifest");
    assert_eq!(parsed.uuid, manifest.uuid);
    assert_eq!(parsed.generation.vertex_count, stats.vertex_count);
}

#[test]
fn invalid_lol_source_surfaces_parse_error() {
    // `not_a_primitive` is deliberately not in the alice-lol grammar; the
    // pipeline should refuse rather than silently producing junk
    let dir = tempfile::tempdir().expect("tempdir");
    assert!(pipeline::validate_lol("not_a_primitive(1.0)").is_err());
    let export = pipeline::export_mesh(
        "not_a_primitive(1.0)",
        dir.path(),
        ExportFormat::ThreeMf,
        Quality::Preview,
    );
    assert!(export.is_err(), "invalid LOL should not produce a 3MF");
}
