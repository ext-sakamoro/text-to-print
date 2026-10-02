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

/// 無向エッジごとの共有三角形数から (境界エッジ数, 非多様体エッジ数) を数える
///
/// 閉じた多様体では全ての無向エッジがちょうど 2 枚に共有される
fn edge_defects(tris: &[[usize; 3]]) -> (usize, usize) {
    let mut edges: std::collections::HashMap<(usize, usize), usize> =
        std::collections::HashMap::new();
    for &[a, b, c] in tris {
        for (u, v) in [(a, b), (b, c), (c, a)] {
            *edges.entry((u.min(v), u.max(v))).or_insert(0) += 1;
        }
    }
    let boundary = edges.values().filter(|&&n| n == 1).count();
    let non_manifold = edges.values().filter(|&&n| n > 2).count();
    (boundary, non_manifold)
}

/// `lol` を 3MF に書き出して、中の mesh を独立パーサで読み戻す
fn export_and_read(lol: &str, quality: Quality) -> (Vec<[f64; 3]>, Vec<[usize; 3]>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let stats = pipeline::export_mesh(lol, dir.path(), ExportFormat::ThreeMf, quality)
        .expect("3MF export should succeed");
    read_3mf_mesh(&stats.path)
}

#[test]
fn exported_3mf_is_watertight() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lol = pipeline::extract_lol(MOCK_LLM_RESPONSE).expect("extract");
    let stats = pipeline::export_mesh(&lol, dir.path(), ExportFormat::ThreeMf, Quality::Preview)
        .expect("3MF export should succeed");
    let (_, tris) = read_3mf_mesh(&stats.path);

    // oracle: 閉じた多様体では全ての無向エッジがちょうど 2 枚に共有される
    let (boundary, non_manifold) = edge_defects(&tris);
    assert_eq!(boundary, 0, "境界エッジ {boundary} 本 = 水密でない");
    assert_eq!(non_manifold, 0, "非多様体エッジ {non_manifold} 本");
}

/// 薄板 `box3d(15, 15, 0.4)` (half extents なので 30 x 30 x 0.8 mm) は
/// `should_use_dual_contouring` が真 (min_dim <= 5mm) になる DC 経路の入力
/// t2p の主力は薄物なのに、DC 経路の実 mesh は体積も水密も突合されていなかった
const THIN_PLATE: &str = "box3d(15.0, 15.0, 0.4)";
/// 同じ箱を厚くしたもの (20 x 20 x 20 mm、aspect 1 / min_dim 20 なので MC 経路) の対照
const BULKY_BOX: &str = "box3d(10.0, 10.0, 10.0)";

#[test]
fn thin_plate_dc_volume_matches_the_analytic_box() {
    let (verts, tris) = export_and_read(THIN_PLATE, Quality::Preview);
    // oracle: 箱の体積 = 辺の積 (half extents 15, 15, 0.4 -> 30 * 30 * 0.8)
    let want = 30.0 * 30.0 * 0.8;
    let got = signed_volume(&verts, &tris);
    assert!(got > 0.0, "符号付き体積が負 = 内向き巻き ({got})");
    // DC は Hermite data で軸平行な面を厳密に再現するので誤差は浮動小数の丸めだけ
    // (実測 rel 1.1e-5)  薄板を MC 経路に回すと 1.4e-3 まで落ちるので、許容を 5e-4 に
    // 絞って「DC 経路で作られたこと」まで pin する (5% では経路の取り違えを検出できない)
    let rel = (got - want).abs() / want;
    assert!(
        rel < 5e-4,
        "薄板 (DC 経路) の体積 {got:.4}mm³ が箱の閉形式 {want:.4}mm³ から外れた (rel {rel:.6})"
    );
}

#[test]
fn thin_plate_dc_mesh_is_watertight() {
    let (_, tris) = export_and_read(THIN_PLATE, Quality::Preview);
    let (boundary, non_manifold) = edge_defects(&tris);
    assert_eq!(boundary, 0, "薄板の境界エッジ {boundary} 本 = 水密でない");
    assert_eq!(non_manifold, 0, "薄板の非多様体エッジ {non_manifold} 本");
}

#[test]
fn bulky_box_mc_volume_matches_the_analytic_box() {
    let (verts, tris) = export_and_read(BULKY_BOX, Quality::Preview);
    // oracle: half extents 10 -> 20 * 20 * 20
    let want = 20.0_f64.powi(3);
    let got = signed_volume(&verts, &tris);
    assert!(got > 0.0, "符号付き体積が負 = 内向き巻き ({got})");
    let rel = (got - want).abs() / want;
    assert!(
        rel < 0.05,
        "箱 (MC 経路) の体積 {got:.2}mm³ が閉形式 {want:.2}mm³ から外れた (rel {rel:.4})"
    );
    let (boundary, non_manifold) = edge_defects(&tris);
    assert_eq!((boundary, non_manifold), (0, 0), "箱の mesh が水密でない");
}

/// STEP (ISO 10303-21) の DATA section を `#id → (entity 名, 引数)` に読み戻す
///
/// `export_step` の出力を **独立に**パースするための最小実装 record は
/// `#12=ADVANCED_FACE('',(#11),#10,.T.);` の形なので `;` 区切りで拾う
fn read_step_entities(path: &str) -> std::collections::HashMap<u64, (String, String)> {
    let text = std::fs::read_to_string(path).expect("read step");
    parse_step_entities(&text)
}

/// [`read_step_entities`] の本体 (text から parse、detector 自体の test で再利用)
fn parse_step_entities(text: &str) -> std::collections::HashMap<u64, (String, String)> {
    assert!(
        text.starts_with("ISO-10303-21;"),
        "ISO-10303-21 の magic で始まっていない"
    );
    assert!(
        text.trim_end().ends_with("END-ISO-10303-21;"),
        "END-ISO-10303-21 で終わっていない"
    );
    let data = text
        .split_once("DATA;")
        .expect("DATA section が無い")
        .1
        .split_once("ENDSEC;")
        .expect("DATA の ENDSEC が無い")
        .0;

    let mut out = std::collections::HashMap::new();
    for record in data.split(';') {
        let record = record.trim();
        let Some(body) = record.strip_prefix('#') else {
            continue;
        };
        let Some((id, rhs)) = body.split_once('=') else {
            continue;
        };
        let Ok(id) = id.trim().parse::<u64>() else {
            continue;
        };
        // 閉じ括弧は 1 個だけ落とす (`trim_end_matches` だと
        // `CARTESIAN_POINT('',(0.0,0.0,0.0))` の内側の `)` まで剥がれる)
        let (name, args) = rhs
            .trim()
            .split_once('(')
            .map_or((rhs.trim(), ""), |(n, a)| {
                (n, a.strip_suffix(')').unwrap_or(a))
            });
        out.insert(id, (name.trim().to_string(), args.to_string()));
    }
    out
}

/// 引数文字列に現れる `#N` 参照を全部拾う
fn step_refs(args: &str) -> Vec<u64> {
    let mut out = Vec::new();
    let bytes = args.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'#' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end > start
                && let Ok(n) = args[start..end].parse::<u64>()
            {
                out.push(n);
            }
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

/// 定義されていない `#N` を参照している record を列挙する
fn step_dangling_refs(entities: &std::collections::HashMap<u64, (String, String)>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (id, (name, args)) in entities {
        for r in step_refs(args) {
            if !entities.contains_key(&r) {
                out.push(format!("#{id}={name} が #{r} を参照 (未定義)"));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// `CARTESIAN_POINT('',(x,y,z))` の座標を読む
fn step_point(args: &str) -> Option<[f64; 3]> {
    let tuple = args.split_once('(')?.1;
    let tuple = tuple.split_once(')')?.0;
    let mut it = tuple.split(',').map(|v| v.trim().parse::<f64>());
    match (it.next(), it.next(), it.next()) {
        (Some(Ok(x)), Some(Ok(y)), Some(Ok(z))) => Some([x, y, z]),
        _ => None,
    }
}

/// STEP 出力が (1) 未定義参照なし (2) AP214 の必須 root あり (3) 頂点が解析解の
/// 球面に乗っている ことを 1 回の export で確認する
///
/// ALICE-SDF 側は `762b04c` まで未定義の `#0` を参照する file を書いていて
/// (= どの CAD でも開けない)、text-to-print の `ttp export --format step` は
/// そのまま壊れた file を出していた 本 test は t2p の pipeline 配線
/// (`StepConfig` の bounds / resolution) 経由で同じ破れを検出する
///
/// Preview 品質の `sphere(10)` STEP は 55 MB / 71,672 面あるので export は
/// 1 回だけ回して 3 つの oracle を同じ file に当てる
#[test]
fn exported_step_is_a_sound_brep_of_the_analytic_sphere() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lol = pipeline::extract_lol(MOCK_LLM_RESPONSE).expect("extract");
    let stats = pipeline::export_mesh(&lol, dir.path(), ExportFormat::Step, Quality::Preview)
        .expect("STEP export should succeed");
    assert!(stats.path.ends_with(".step"), "出力が .step でない");

    let entities = read_step_entities(&stats.path);
    assert!(!entities.is_empty(), "DATA section にレコードが無い");

    // (1) 未定義参照 0 件 (検出力は
    //     `step_dangling_ref_detector_catches_an_injected_fault` で別途確認)
    let dangling = step_dangling_refs(&entities);
    assert!(
        dangling.is_empty(),
        "解決できない参照 {} 件:\n{}",
        dangling.len(),
        dangling
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );

    // (2) solid として成立する root
    for root in [
        "CLOSED_SHELL",
        "MANIFOLD_SOLID_BREP",
        "ADVANCED_BREP_SHAPE_REPRESENTATION",
        "SHAPE_DEFINITION_REPRESENTATION",
    ] {
        assert!(
            entities.values().any(|(name, _)| name == root),
            "{root} が STEP に無い"
        );
    }

    // (3) VERTEX_POINT が指す CARTESIAN_POINT だけを見る (原点 / 軸の定義点は除外)
    // t2p は `StepConfig.bounds` を tight AABB ± 1mm、resolution を `Quality` から
    // 決めて渡す 配線が狂うと solid が clip / ずれるが「file として成立している」
    // だけの assert では検出できない
    let radii: Vec<f64> = entities
        .values()
        .filter(|(name, _)| name == "VERTEX_POINT")
        .filter_map(|(_, args)| step_refs(args).first().copied())
        .filter_map(|pid| entities.get(&pid))
        .filter(|(name, _)| name == "CARTESIAN_POINT")
        .filter_map(|(_, args)| step_point(args))
        .map(|[x, y, z]| (x * x + y * y + z * z).sqrt())
        .collect();
    assert!(
        radii.len() > 100,
        "VERTEX_POINT が {} 個しかない = tessellation が走っていない",
        radii.len()
    );

    // oracle: 半径 10mm の球面 marching cubes の頂点は線形補間で表面に乗る
    // 独立実装 (python で同 file を再パース) の実測は max |r − 10| = 0.0006mm
    // なので 0.05mm は 80 倍の余裕がある = 乖離したら bounds / scale の配線ずれ
    let worst = radii
        .iter()
        .copied()
        .fold(0.0_f64, |acc, r| acc.max((r - 10.0).abs()));
    assert!(
        worst < 0.05,
        "頂点の球面からの乖離 最大 {worst:.4}mm (許容 0.05mm) = bounds / scale の配線ずれ"
    );
}

/// detector 自体の検出力 — 未定義参照を 1 個注入したら必ず落ちること
///
/// 「未定義参照 0 件」の assert は parser が何も読めていなくても通る (vacuous)
/// ので、欠陥を注入して実際に検出されるかを別に確かめる
#[test]
fn step_dangling_ref_detector_catches_an_injected_fault() {
    let sound = "ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n\
        #1=CARTESIAN_POINT('',(0.0,0.0,0.0));\n\
        #2=VERTEX_POINT('',#1);\n\
        ENDSEC;\nEND-ISO-10303-21;\n";
    assert!(
        step_dangling_refs(&parse_step_entities(sound)).is_empty(),
        "健全な STEP で誤検出した"
    );

    let broken = sound.replace("VERTEX_POINT('',#1)", "VERTEX_POINT('',#0)");
    let hits = step_dangling_refs(&parse_step_entities(&broken));
    assert_eq!(
        hits.len(),
        1,
        "未定義 #0 を注入したのに検出されなかった: {hits:?}"
    );
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
