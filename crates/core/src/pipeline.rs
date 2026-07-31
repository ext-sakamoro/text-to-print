use alice_bamboo::color4::{Color4Config, quantize_to_4color};
use alice_bamboo::overhang::{OverhangConfig, OverhangReport, analyze_overhang};
use alice_bamboo::safety::{SafetyReport, safety_validate};
use alice_lol::print_export::{ExportStats, PrintConfig};
use alice_lol::runtime_parser;
use alice_sdf::io::threemf::export_3mf;
use alice_sdf::mesh::{MarchingCubesConfig, MeshRepair, sdf_to_mesh};
use alice_sdf::tight_aabb::{TightAabbConfig, compute_tight_aabb_with_config};
use anyhow::{Result, bail};
use glam::Vec3;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::info;

use crate::manifest::{self, AliceManifest, ManifestBuilder};
use crate::tier::Tier;

#[derive(Debug, Clone)]
pub struct GenerationResult {
    pub lol_source: String,
    pub stats: Option<MeshStats>,
}

#[derive(Debug, Clone)]
pub struct MeshStats {
    pub vertex_count: usize,
    pub triangle_count: usize,
    pub path: String,
    /// Overhang analysis summary (populated only for 3MF exports via
    /// `alice_bamboo` pipeline; FBX/STL exports leave this `None`).
    pub overhang_summary: Option<OverhangSummary>,
    /// Safety validation summary from `alice_bamboo::safety` (populated
    /// only for 3MF exports).
    pub safety_summary: Option<SafetySummary>,
    /// G-code slicer summary (populated only for `ExportFormat::Gcode`
    /// exports via `alice_print::slice_sdf`)
    pub slice_summary: Option<SliceSummary>,
}

/// Compact G-code slicer summary kept in `MeshStats` for UI display
/// (Stage 7 #5) Mirrors the subset of `alice_print::SliceResult` the UI
/// actually renders — full result stays inside the pipeline
#[derive(Debug, Clone, Copy)]
pub struct SliceSummary {
    pub layer_count: usize,
    pub filament_meters: f32,
    pub print_time_seconds: f32,
}

/// Compact overhang report kept in `MeshStats` for UI display.
#[derive(Debug, Clone)]
pub struct OverhangSummary {
    pub overhang_face_count: usize,
    pub total_face_count: usize,
    pub overhang_ratio: f32,
    pub max_wall_angle_deg: f32,
    pub total_overhang_area_mm2: f32,
}

impl OverhangSummary {
    fn from_report(report: &OverhangReport) -> Self {
        Self {
            overhang_face_count: report.overhang_face_count(),
            total_face_count: report.total_face_count(),
            overhang_ratio: report.overhang_ratio(),
            max_wall_angle_deg: report.max_wall_angle_deg,
            total_overhang_area_mm2: report.total_overhang_area_mm2,
        }
    }
}

/// Compact safety report kept in `MeshStats` for UI display.
#[derive(Debug, Clone)]
pub struct SafetySummary {
    pub material_name: String,
    pub is_safe: bool,
    pub warp_category: String,
    pub messages: Vec<String>,
}

impl SafetySummary {
    fn from_report(report: &SafetyReport) -> Self {
        Self {
            material_name: report.material_name.to_string(),
            is_safe: report.is_safe,
            warp_category: format!("{:?}", report.warp.category),
            messages: report.messages.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ExportFormat {
    ThreeMf,
    Fbx,
    Stl,
    /// STEP AP203 (ISO 10303-21) — CAD kernel neutral format used by
    /// Fusion 360 / FreeCAD / SolidWorks for import Backed by
    /// `alice_sdf::io::step::export_step` which tessellates the SDF
    /// then writes a valid STEP faceted BREP
    Step,
    /// G-code direct output — bypasses Bambu Studio Backed by
    /// `alice_print::slice_sdf` with Bambu Lab preset The output is
    /// Marlin flavor by default
    Gcode,
}

#[derive(Debug, Clone, Copy)]
pub enum Quality {
    Preview,
    High,
    Ultra,
}

impl Quality {
    pub fn to_print_config(self) -> PrintConfig {
        match self {
            Self::Preview => PrintConfig::preview(),
            Self::High => PrintConfig::high_quality(),
            Self::Ultra => PrintConfig::ultra(),
        }
    }

    /// Marching-cubes resolution used by the `alice_bamboo` 3MF pipeline.
    pub fn mesh_resolution(self) -> usize {
        match self {
            Self::Preview => 128,
            Self::High => 192,
            Self::Ultra => 256,
        }
    }
}

/// LOL ソースを検証（パースできるか）
pub fn validate_lol(lol_source: &str) -> Result<()> {
    runtime_parser::parse_lol(lol_source)
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("LOL parse error: {}", e.message))
}

/// LOL → メッシュファイル (.3mf / .fbx / .stl) にエクスポート
///
/// 3MF 経路は `alice_bamboo` を経由し、`safety_validate` (alice-physics
/// filament_db + warp risk) と `analyze_overhang` の結果を `MeshStats`
/// に格納する FBX / STL 経路は `alice_lol` の直接 export を使用する
/// (alice-bamboo は現状 3MF のみ対応)
pub fn export_mesh(
    lol_source: &str,
    output_dir: &Path,
    format: ExportFormat,
    quality: Quality,
) -> Result<MeshStats> {
    let filename = format!("{}.{}", uuid::Uuid::now_v7(), format.extension());
    let output_path = output_dir.join(&filename);

    info!(
        format = format.extension(),
        quality = ?quality,
        path = %output_path.display(),
        "exporting mesh"
    );

    match format {
        ExportFormat::ThreeMf => export_3mf_via_bamboo(lol_source, &output_path, quality),
        ExportFormat::Fbx => {
            let config = quality.to_print_config();
            let stats = alice_lol::print_export::lol_to_fbx(lol_source, &output_path, &config)?;
            Ok(to_mesh_stats(&stats))
        }
        ExportFormat::Stl => {
            let config = quality.to_print_config();
            let stats = alice_lol::print_export::lol_to_stl(lol_source, &output_path, &config)?;
            Ok(to_mesh_stats(&stats))
        }
        ExportFormat::Step => export_step_via_alice_sdf(lol_source, &output_path, quality),
        ExportFormat::Gcode => export_gcode_via_alice_print(lol_source, &output_path),
    }
}

/// LOL → 3MF via alice-bamboo, with safety + overhang analysis.
///
/// Pipeline:
/// 1. `alice_bamboo::lol_to_sdf` — parse LOL DSL into an `SdfNode` tree.
/// 2. `alice_bamboo::safety::safety_validate` — filament DB + warp risk +
///    thermal stress via `alice-physics`.
/// 3. Build mesh via marching cubes + manifold repair (mirrors
///    `alice_bamboo::export_to_3mf` internals so we can hand the mesh to
///    the overhang analyser before writing the 3MF file).
/// 4. `alice_bamboo::overhang::analyze_overhang` — per-face wall angle
///    analysis for FDM support planning.
/// 5. `alice_sdf::io::threemf::export_3mf` — write the repaired mesh.
fn export_3mf_via_bamboo(
    lol_source: &str,
    output_path: &Path,
    quality: Quality,
) -> Result<MeshStats> {
    let sdf = alice_bamboo::lol_to_sdf(lol_source)
        .map_err(|e| anyhow::anyhow!("LOL parse error: {e}"))?;

    let safety_report = safety_validate(&sdf, "PLA", None);
    if !safety_report.is_safe {
        info!(
            material = safety_report.material_name,
            warp_category = ?safety_report.warp.category,
            "safety report flagged issues (informational, export continues)"
        );
    }
    let safety_summary = SafetySummary::from_report(&safety_report);

    let aabb_config = TightAabbConfig {
        initial_half_size: 500.0,
        bisection_iterations: 24,
        coarse_subdivisions: 16,
    };
    let aabb = compute_tight_aabb_with_config(&sdf, &aabb_config);
    let padding = Vec3::splat(1.0);
    let min_bounds = aabb.min - padding;
    let max_bounds = aabb.max + padding;
    let mc_config = MarchingCubesConfig {
        resolution: quality.mesh_resolution(),
        ..Default::default()
    };
    let mesh = sdf_to_mesh(&sdf, min_bounds, max_bounds, &mc_config);
    let mesh = MeshRepair::repair_all(&mesh, 5e-3);

    let overhang_report = analyze_overhang(&mesh, &OverhangConfig::default());
    let overhang_summary = OverhangSummary::from_report(&overhang_report);

    let vertex_count = mesh.vertices.len();
    let triangle_count = mesh.indices.len() / 3;
    export_3mf(&mesh, output_path).map_err(|e| anyhow::anyhow!("3MF export error: {e}"))?;

    Ok(MeshStats {
        vertex_count,
        triangle_count,
        path: output_path.to_string_lossy().into_owned(),
        overhang_summary: Some(overhang_summary),
        safety_summary: Some(safety_summary),
        slice_summary: None,
    })
}

fn to_mesh_stats(stats: &ExportStats) -> MeshStats {
    MeshStats {
        vertex_count: stats.vertex_count,
        triangle_count: stats.triangle_count,
        path: stats.path.clone(),
        overhang_summary: None,
        safety_summary: None,
        slice_summary: None,
    }
}

/// LOL → STEP (ISO 10303-21 AP203) via `alice_sdf::io::step::export_step`
///
/// The SDF is tessellated internally by `export_step` using its own
/// marching-cubes pass then written as a Faceted BREP entity We rebuild
/// the mesh separately to surface a vertex / triangle count in
/// `MeshStats` for the UI That path is small (order of megabytes) so
/// the duplicate work is acceptable
fn export_step_via_alice_sdf(
    lol_source: &str,
    output_path: &Path,
    quality: Quality,
) -> Result<MeshStats> {
    let sdf = alice_bamboo::lol_to_sdf(lol_source)
        .map_err(|e| anyhow::anyhow!("LOL parse error: {e}"))?;

    // Vertex / triangle counts via a preview-quality mesh — the exported
    // STEP file uses its own internal tessellation but this at least
    // gives the UI a rough size estimate
    let aabb_config = TightAabbConfig {
        initial_half_size: 500.0,
        bisection_iterations: 24,
        coarse_subdivisions: 16,
    };
    let aabb = compute_tight_aabb_with_config(&sdf, &aabb_config);
    let padding = Vec3::splat(1.0);
    let mc_config = MarchingCubesConfig {
        resolution: quality.mesh_resolution(),
        ..Default::default()
    };
    let mesh = sdf_to_mesh(&sdf, aabb.min - padding, aabb.max + padding, &mc_config);
    let vertex_count = mesh.vertices.len();
    let triangle_count = mesh.indices.len() / 3;

    let step_cfg = alice_sdf::io::step::StepConfig {
        bounds: (aabb.min.min_element() - 1.0, aabb.max.max_element() + 1.0),
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        resolution: quality.mesh_resolution() as u32,
        name: format!("text-to-print-{}", uuid::Uuid::now_v7()),
    };
    alice_sdf::io::step::export_step(output_path, &sdf, &step_cfg)
        .map_err(|e| anyhow::anyhow!("STEP export error: {e}"))?;

    Ok(MeshStats {
        vertex_count,
        triangle_count,
        path: output_path.to_string_lossy().into_owned(),
        overhang_summary: None,
        safety_summary: None,
        slice_summary: None,
    })
}

/// LOL → G-code via `alice_print::slice_sdf`
///
/// The slicer uses the Bambu Lab preset (`SlicerConfig::bambu()`) with
/// Marlin flavor by default Consumers who need Klipper / a different
/// printer preset should call `alice_print::slice_sdf` directly for now
/// (a `GcodeConfig` UI slot is planned separately)
///
/// `SliceSummary` is surfaced through `MeshStats.slice_summary` so the
/// UI can show layer count, filament usage, and print-time estimates
fn export_gcode_via_alice_print(lol_source: &str, output_path: &Path) -> Result<MeshStats> {
    let sdf = alice_bamboo::lol_to_sdf(lol_source)
        .map_err(|e| anyhow::anyhow!("LOL parse error: {e}"))?;
    let slice = alice_bamboo::slice_sdf(
        &sdf,
        &alice_bamboo::SlicerConfig::bambu(),
        alice_bamboo::GcodeFlavor::Marlin,
    );
    std::fs::write(output_path, &slice.gcode)
        .map_err(|e| anyhow::anyhow!("G-code write error: {e}"))?;

    Ok(MeshStats {
        vertex_count: 0,
        triangle_count: 0,
        path: output_path.to_string_lossy().into_owned(),
        overhang_summary: None,
        safety_summary: None,
        slice_summary: Some(SliceSummary {
            layer_count: slice.layer_count,
            filament_meters: slice.filament_meters,
            print_time_seconds: slice.print_time_seconds,
        }),
    })
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::ThreeMf => "3mf",
            Self::Fbx => "fbx",
            Self::Stl => "stl",
            Self::Step => "step",
            Self::Gcode => "gcode",
        }
    }
}

/// Metadata inputs required to build an [`AliceManifest`] alongside an export.
pub struct MetadataInputs<'a> {
    pub prompt: &'a str,
    pub prompt_lang: &'a str,
    pub llm_model: &'a str,
    pub llm_seed: Option<u64>,
    pub retry_count: u32,
    pub safety_violations: Vec<String>,
    /// Tier at generation time Populated on the [`crate::manifest::Attribution`]
    /// section so the LoRA share pipeline can filter by tier and slicers see
    /// `<metadata name="alice:tier">` in the exported 3MF
    pub tier: Tier,
}

/// Fast safety check for LLM retry loop
///
/// Parses the given LOL source into an SDF tree, runs
/// [`alice_bamboo::safety::safety_validate`] against a default `"PLA"`
/// material, and returns the raw safety violation messages Meant to be
/// used as the `safety_check` closure passed to
/// [`text_to_print_llm::backend::generate_with_retry`]
///
/// Behavior:
/// - **LOL parse error** returns a single-message violation vector so the
///   retry loop can request a syntactically valid LOL DSL
/// - **`safety_validate.is_safe == true`** returns an empty vector — the
///   caller is expected to interpret `[]` as "no retry needed"
/// - **`safety_validate.is_safe == false`** returns
///   `safety_validate.messages` verbatim The messages already come from
///   `alice_bamboo::safety::SafetyReport` so
///   [`text_to_print_llm::fix_prompt::SafetyViolationKind::from_message`]
///   can classify them into `fix_directive` instructions
///
/// This helper deliberately skips the mesh build (marching cubes) so it
/// stays cheap enough to be invoked between LLM retries — the actual
/// mesh gets built later during the final `export_mesh` call
#[must_use]
pub fn safety_check_lol(lol_source: &str) -> Vec<String> {
    let sdf = match alice_bamboo::lol_to_sdf(lol_source) {
        Ok(sdf) => sdf,
        Err(e) => return vec![format!("LOL parse error: {e}")],
    };
    let report = alice_bamboo::safety::safety_validate(&sdf, "PLA", None);
    if report.is_safe {
        Vec::new()
    } else {
        report.messages
    }
}

/// caller 由来 violations に、pipeline 内 `safety_validate` の messages を
/// unsafe 判定時のみ merge (重複除外)
///
/// - `caller` は UI / LLM retry 由来の追加 violation
/// - `summary.is_safe == true` の場合は追加なし
/// - `summary == None` (FBX/STL path、safety_validate なし) の場合も追加なし
fn merge_safety_violations(
    mut caller: Vec<String>,
    summary: Option<&SafetySummary>,
) -> Vec<String> {
    if let Some(sum) = summary
        && !sum.is_safe
    {
        for msg in &sum.messages {
            if !caller.contains(msg) {
                caller.push(msg.clone());
            }
        }
    }
    caller
}

/// Export mesh and — for 3MF — embed `alice_manifest.json` + XML metadata tags.
///
/// Non-3MF formats are exported unchanged and still return a manifest computed
/// from the resulting file bytes, so downstream (LoRA share, DB) can piggyback
/// the same quality signals regardless of format.
pub fn export_mesh_with_metadata(
    lol_source: &str,
    output_dir: &Path,
    format: ExportFormat,
    quality: Quality,
    meta: MetadataInputs<'_>,
) -> Result<(MeshStats, AliceManifest)> {
    let start = Instant::now();
    let stats = export_mesh(lol_source, output_dir, format, quality)?;
    let path = Path::new(&stats.path);
    let mesh_bytes = std::fs::read(path)?;

    // GAP-2: 3MF path で計算された safety_summary の messages を
    // manifest.safety_violations に流し込む
    let safety_violations =
        merge_safety_violations(meta.safety_violations, stats.safety_summary.as_ref());

    let manifest = ManifestBuilder {
        prompt: meta.prompt,
        prompt_lang: meta.prompt_lang,
        llm_model: meta.llm_model,
        llm_seed: meta.llm_seed,
        lol_source,
        mesh_bytes: &mesh_bytes,
        vertex_count: stats.vertex_count,
        triangle_count: stats.triangle_count,
        export_format: format.extension(),
        retry_count: meta.retry_count,
        time_to_file_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
        safety_violations,
        success: true,
        tier: meta.tier,
    }
    .build();

    if matches!(format, ExportFormat::ThreeMf) {
        manifest::embed_in_3mf(path, &manifest)?;
    }

    Ok((stats, manifest))
}

/// 4-color multi-filament export (Bambu Lab AMS / Prusa MMU 対応)
///
/// LOL → SDF → mesh → 正面 image 投影 + k-means 量子化 → palette 色ごと
/// 3MF 分割 出力: `{uuid}_color{N}.3mf` を palette 色数だけ生成、
/// 空 mesh は skip
///
/// # Errors
///
/// - LOL parse error
/// - `alice_bamboo::color4::Color4Error` (invalid color count / empty mesh
///   / degenerate bounding box)
/// - 3MF write error (IO)
pub fn export_mesh_color4(
    lol_source: &str,
    output_dir: &Path,
    image: &image::RgbImage,
    quality: Quality,
    cfg: Color4Config,
) -> Result<Vec<PathBuf>> {
    let sdf = alice_bamboo::lol_to_sdf(lol_source)
        .map_err(|e| anyhow::anyhow!("LOL parse error: {e}"))?;

    let aabb_config = TightAabbConfig {
        initial_half_size: 500.0,
        bisection_iterations: 24,
        coarse_subdivisions: 16,
    };
    let aabb = compute_tight_aabb_with_config(&sdf, &aabb_config);
    let padding = Vec3::splat(1.0);
    let mc_config = MarchingCubesConfig {
        resolution: quality.mesh_resolution(),
        ..Default::default()
    };
    let mesh = sdf_to_mesh(&sdf, aabb.min - padding, aabb.max + padding, &mc_config);
    let mesh = MeshRepair::repair_all(&mesh, 5e-3);

    let result = quantize_to_4color(&mesh, image, &cfg)
        .map_err(|e| anyhow::anyhow!("color4 quantization error: {e}"))?;

    let uuid = uuid::Uuid::now_v7();
    let mut output_paths = Vec::with_capacity(result.mesh_per_color.len());
    for (i, sub_mesh) in result.mesh_per_color.iter().enumerate() {
        if sub_mesh.vertices.is_empty() {
            continue;
        }
        let filename = format!("{uuid}_color{i}.3mf");
        let output_path = output_dir.join(&filename);
        export_3mf(sub_mesh, &output_path)
            .map_err(|e| anyhow::anyhow!("3MF write error for color {i}: {e}"))?;
        output_paths.push(output_path);
    }

    Ok(output_paths)
}

/// LOL ソースからコードブロックを抽出
pub fn extract_lol(llm_response: &str) -> Option<String> {
    let start = llm_response.find("```lol")?;
    let code_start = llm_response[start..].find('\n')? + start + 1;
    let end = llm_response[code_start..].find("```")? + code_start;
    Some(llm_response[code_start..end].trim().to_string())
}

/// LOL → WGSL シェーダー生成（SDF プレビュー用）
pub fn lol_to_wgsl(lol_source: &str) -> Result<String> {
    let node = runtime_parser::parse_lol(lol_source)
        .map_err(|e| anyhow::anyhow!("LOL parse error: {}", e.message))?;

    let wgsl = alice_lol::to_wgsl(&node);
    if wgsl.is_empty() {
        bail!("WGSL generation failed: empty output");
    }
    Ok(wgsl)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safety_check_lol_returns_empty_for_small_pla_sphere() {
        // small sphere with PLA material → safety_validate.is_safe == true
        let violations = safety_check_lol("sphere(10.0)");
        assert!(
            violations.is_empty(),
            "expected no violations for sphere(10), got {violations:?}"
        );
    }

    #[test]
    fn safety_check_lol_returns_parse_error_for_invalid_source() {
        let violations = safety_check_lol("not_a_primitive(1.0)");
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("parse error"));
    }

    #[test]
    fn test_extract_lol() {
        let response = "Here is the model:\n```lol\nsphere(1.0)\n```\nDone.";
        assert_eq!(extract_lol(response), Some("sphere(1.0)".to_string()));
    }

    #[test]
    fn test_extract_lol_no_block() {
        assert_eq!(extract_lol("no code here"), None);
    }

    #[test]
    fn test_validate_lol_valid() {
        assert!(validate_lol("sphere(1.0)").is_ok());
    }

    #[test]
    fn test_validate_lol_invalid() {
        assert!(validate_lol("not_a_primitive(1.0)").is_err());
    }

    #[test]
    fn test_quality_mesh_resolution() {
        assert_eq!(Quality::Preview.mesh_resolution(), 128);
        assert_eq!(Quality::High.mesh_resolution(), 192);
        assert_eq!(Quality::Ultra.mesh_resolution(), 256);
    }

    #[test]
    fn test_export_format_extension() {
        assert_eq!(ExportFormat::ThreeMf.extension(), "3mf");
        assert_eq!(ExportFormat::Fbx.extension(), "fbx");
        assert_eq!(ExportFormat::Stl.extension(), "stl");
        assert_eq!(ExportFormat::Step.extension(), "step");
        assert_eq!(ExportFormat::Gcode.extension(), "gcode");
    }

    #[test]
    fn test_export_step_produces_iso_10303_file() {
        let dir = tempfile::tempdir().unwrap();
        let stats = export_mesh(
            "sphere(10.0)",
            dir.path(),
            ExportFormat::Step,
            Quality::Preview,
        )
        .expect("STEP export should succeed for sphere(10)");
        assert!(stats.path.ends_with(".step"));
        assert!(stats.vertex_count > 0);
        assert!(stats.triangle_count > 0);
        assert!(stats.slice_summary.is_none());
        let header = std::fs::read_to_string(&stats.path).unwrap();
        assert!(
            header.contains("ISO-10303-21"),
            "STEP file should start with ISO-10303-21 magic"
        );
    }

    #[test]
    fn test_export_gcode_produces_slice_summary() {
        let dir = tempfile::tempdir().unwrap();
        let stats = export_mesh(
            "sphere(10.0)",
            dir.path(),
            ExportFormat::Gcode,
            Quality::Preview,
        )
        .expect("G-code export should succeed for sphere(10)");
        assert!(stats.path.ends_with(".gcode"));
        assert!(
            stats.slice_summary.is_some(),
            "G-code path should populate slice_summary"
        );
        let summary = stats.slice_summary.as_ref().unwrap();
        assert!(summary.layer_count > 0);
        assert!(summary.print_time_seconds > 0.0);
        // Sanity check the file contains at least one G0/G1 command
        let body = std::fs::read_to_string(&stats.path).unwrap();
        assert!(
            body.lines()
                .any(|l| l.starts_with("G0 ") || l.starts_with("G1 ")),
            "G-code file should contain G0 / G1 commands"
        );
    }

    fn safety_summary(is_safe: bool, messages: Vec<String>) -> SafetySummary {
        SafetySummary {
            material_name: "PLA".to_string(),
            is_safe,
            warp_category: "Low".to_string(),
            messages,
        }
    }

    #[test]
    fn merge_safety_violations_none_summary_passthrough() {
        let caller = vec!["llm_retry_over_65deg".to_string()];
        let merged = merge_safety_violations(caller.clone(), None);
        assert_eq!(merged, caller);
    }

    #[test]
    fn merge_safety_violations_safe_summary_passthrough() {
        let caller = vec!["llm_retry_over_65deg".to_string()];
        let sum = safety_summary(true, vec!["ignored_because_safe".to_string()]);
        let merged = merge_safety_violations(caller.clone(), Some(&sum));
        assert_eq!(merged, caller, "is_safe=true 時は messages を merge しない");
    }

    #[test]
    fn merge_safety_violations_unsafe_summary_appends() {
        let caller = vec!["llm_retry_over_65deg".to_string()];
        let sum = safety_summary(
            false,
            vec![
                "warp_high_risk".to_string(),
                "thin_wall_below_0.8mm".to_string(),
            ],
        );
        let merged = merge_safety_violations(caller, Some(&sum));
        assert_eq!(merged.len(), 3);
        assert!(merged.contains(&"llm_retry_over_65deg".to_string()));
        assert!(merged.contains(&"warp_high_risk".to_string()));
        assert!(merged.contains(&"thin_wall_below_0.8mm".to_string()));
    }

    #[test]
    fn merge_safety_violations_dedup() {
        let caller = vec!["warp_high_risk".to_string()];
        let sum = safety_summary(
            false,
            vec![
                "warp_high_risk".to_string(),
                "thin_wall_below_0.8mm".to_string(),
            ],
        );
        let merged = merge_safety_violations(caller, Some(&sum));
        assert_eq!(
            merged.len(),
            2,
            "caller に既存の violation は重複追加しない"
        );
    }

    #[test]
    fn test_export_3mf_via_bamboo_populates_summaries() {
        let dir = tempfile::tempdir().unwrap();
        let stats = export_mesh(
            "sphere(10.0)",
            dir.path(),
            ExportFormat::ThreeMf,
            Quality::Preview,
        )
        .expect("3MF export should succeed for sphere(10)");
        assert!(stats.vertex_count > 0);
        assert!(stats.triangle_count > 0);
        assert!(stats.path.ends_with(".3mf"));
        assert!(
            stats.overhang_summary.is_some(),
            "3MF path should populate overhang summary"
        );
        assert!(
            stats.safety_summary.is_some(),
            "3MF path should populate safety summary"
        );
        let safety = stats.safety_summary.as_ref().unwrap();
        assert_eq!(safety.material_name, "PLA");
    }

    #[test]
    fn test_export_mesh_color4_generates_split_3mf() {
        use image::{Rgb, RgbImage};
        let dir = tempfile::tempdir().unwrap();
        let mut img = RgbImage::new(2, 2);
        img.put_pixel(0, 0, Rgb([255, 0, 0]));
        img.put_pixel(1, 0, Rgb([255, 0, 0]));
        img.put_pixel(0, 1, Rgb([0, 0, 255]));
        img.put_pixel(1, 1, Rgb([0, 0, 255]));
        let cfg = Color4Config {
            n_colors: 2,
            ..Default::default()
        };
        let paths = export_mesh_color4("sphere(10.0)", dir.path(), &img, Quality::Preview, cfg)
            .expect("color4 export should succeed");
        assert!(
            !paths.is_empty(),
            "at least one color 3MF should be written"
        );
        for p in &paths {
            assert!(p.exists(), "output 3MF path {} must exist", p.display());
        }
    }

    #[test]
    fn export_mesh_with_metadata_embeds_manifest_for_3mf() {
        let dir = tempfile::tempdir().unwrap();
        let meta = MetadataInputs {
            prompt: "a sphere",
            prompt_lang: "en",
            llm_model: "test-model",
            llm_seed: None,
            retry_count: 0,
            safety_violations: vec![],
            tier: Tier::Free,
        };
        let (stats, manifest) = export_mesh_with_metadata(
            "sphere(1.0)",
            dir.path(),
            ExportFormat::ThreeMf,
            Quality::Preview,
            meta,
        )
        .unwrap();

        assert_eq!(manifest.schema_version, "1");
        assert_eq!(manifest.quality.export_format, "3mf");
        assert!(manifest.generation.vertex_count > 0);
        assert_eq!(stats.vertex_count, manifest.generation.vertex_count);

        let bytes = std::fs::read(&stats.path).unwrap();
        let mut ar = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut man = ar.by_name("Metadata/alice_manifest.json").unwrap();
        let mut json = String::new();
        std::io::Read::read_to_string(&mut man, &mut json).unwrap();
        let parsed: AliceManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.uuid, manifest.uuid);
    }
}
