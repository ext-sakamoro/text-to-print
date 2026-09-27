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

/// `3D/3dmodel.model` から頂点と三角形を読み戻す
///
/// 3MF を「ZIP として開ける」「`<model` を含む」だけでなく、**中の mesh が
/// 意図した立体になっているか** を突き合わせるための独立パーサ
fn read_3mf_mesh(path: &str) -> (Vec<[f64; 3]>, Vec<[usize; 3]>) {
    let bytes = std::fs::read(path).expect("read 3MF bytes");
    let mut ar = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("open 3MF as ZIP");
    let mut root = String::new();
    ar.by_name("3D/3dmodel.model")
        .expect("3MF must contain 3D/3dmodel.model")
        .read_to_string(&mut root)
        .expect("read model xml");

    // Bambu / MakerWorld 3MF は mesh を production extension の別 part
    // (`<component p:path="/3D/Objects/object_1.model">`) に置くので、root から
    // 参照を辿る 参照先が存在しない 3MF は Bambu Studio が開けない
    let parts: Vec<String> = root
        .split('<')
        .filter_map(|tag| str_attr(tag, "p:path"))
        .map(|p| p.trim_start_matches('/').to_string())
        .collect();
    assert!(
        !parts.is_empty(),
        "root model が geometry part を参照していない"
    );

    let mut verts: Vec<[f64; 3]> = Vec::new();
    let mut tris: Vec<[usize; 3]> = Vec::new();
    for part in &parts {
        let mut xml = String::new();
        ar.by_name(part)
            .unwrap_or_else(|_| panic!("参照先 {part} が 3MF に無い"))
            .read_to_string(&mut xml)
            .expect("read part");
        let base = verts.len();
        for tag in xml.split('<') {
            if tag.starts_with("vertex ") {
                let f = |k: &str| {
                    str_attr(tag, k)
                        .and_then(|v| v.parse::<f64>().ok())
                        .unwrap_or_else(|| panic!("vertex {k}"))
                };
                verts.push([f("x"), f("y"), f("z")]);
            } else if tag.starts_with("triangle ") {
                let i = |k: &str| {
                    str_attr(tag, k)
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or_else(|| panic!("triangle {k}"))
                };
                tris.push([base + i("v1"), base + i("v2"), base + i("v3")]);
            }
        }
    }
    assert!(!verts.is_empty(), "3MF に頂点が無い");
    assert!(!tris.is_empty(), "3MF に三角形が無い");
    (verts, tris)
}

/// 開始タグから属性値を読む
///
/// 属性名は token 境界から始まり `=` が続くものだけを拾う (`x` を単純検索すると
/// `vertex` の `x` に当たって値が読めない)
fn str_attr<'a>(tag: &'a str, key: &str) -> Option<&'a str> {
    let mut from = 0usize;
    loop {
        let at = from + tag[from..].find(key)?;
        let after = at + key.len();
        let boundary = at == 0 || tag[..at].ends_with(char::is_whitespace);
        let value = if boundary {
            tag[after..].trim_start().strip_prefix('=')
        } else {
            None
        };
        if let Some(rest) = value {
            let rest = rest.trim_start();
            let quote = rest.chars().next()?;
            let rest = &rest[1..];
            let end = rest.find(quote)?;
            return Some(&rest[..end]);
        }
        from = after;
    }
}

/// 符号付き体積 (Σ a·(b×c)/6) 外向き巻きなら正
fn signed_volume(verts: &[[f64; 3]], tris: &[[usize; 3]]) -> f64 {
    tris.iter()
        .map(|&[i, j, k]| {
            let (a, b, c) = (verts[i], verts[j], verts[k]);
            let cross = [
                b[1] * c[2] - b[2] * c[1],
                b[2] * c[0] - b[0] * c[2],
                b[0] * c[1] - b[1] * c[0],
            ];
            (a[0] * cross[0] + a[1] * cross[1] + a[2] * cross[2]) / 6.0
        })
        .sum()
}

#[test]
fn exported_3mf_volume_matches_the_analytic_sphere() {
    let dir = tempfile::tempdir().expect("tempdir");
    // `sphere(10.0)` = 半径 10mm の球
    let lol = pipeline::extract_lol(MOCK_LLM_RESPONSE).expect("extract");
    let stats = pipeline::export_mesh(&lol, dir.path(), ExportFormat::ThreeMf, Quality::Preview)
        .expect("3MF export should succeed");
    let (verts, tris) = read_3mf_mesh(&stats.path);
    assert_eq!(
        verts.len(),
        stats.vertex_count,
        "3MF の頂点数が stats と不一致"
    );
    assert_eq!(
        tris.len(),
        stats.triangle_count,
        "3MF の三角形数が stats と不一致"
    );

    // oracle: 球の体積 4/3 π r³
    let want = 4.0 / 3.0 * std::f64::consts::PI * 1000.0;
    let got = signed_volume(&verts, &tris);
    assert!(got > 0.0, "符号付き体積が負 = 内向き巻き ({got})");
    let rel = (got - want).abs() / want;
    assert!(
        rel < 0.05,
        "3MF の体積 {got:.2}mm³ が 4/3πr³ {want:.2}mm³ から外れた (rel {rel:.4})"
    );
}

#[test]
fn exported_3mf_is_watertight() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lol = pipeline::extract_lol(MOCK_LLM_RESPONSE).expect("extract");
    let stats = pipeline::export_mesh(&lol, dir.path(), ExportFormat::ThreeMf, Quality::Preview)
        .expect("3MF export should succeed");
    let (_, tris) = read_3mf_mesh(&stats.path);

    // oracle: 閉じた多様体では全ての無向エッジがちょうど 2 枚に共有される
    let mut edges: std::collections::HashMap<(usize, usize), usize> =
        std::collections::HashMap::new();
    for &[a, b, c] in &tris {
        for (u, v) in [(a, b), (b, c), (c, a)] {
            *edges.entry((u.min(v), u.max(v))).or_insert(0) += 1;
        }
    }
    let boundary = edges.values().filter(|&&n| n == 1).count();
    let non_manifold = edges.values().filter(|&&n| n > 2).count();
    assert_eq!(boundary, 0, "境界エッジ {boundary} 本 = 水密でない");
    assert_eq!(non_manifold, 0, "非多様体エッジ {non_manifold} 本");
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
        tier: text_to_print_core::tier::Tier::Free,
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
