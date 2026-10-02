use alice_bamboo::bambu_3mf::export_bambu_3mf;
use alice_bamboo::color4::{Color4Config, quantize_to_4color};
use alice_bamboo::dfam::{
    DfamConfig, DfamReport, OrientationConfig, OrientationReport, Process, Verdict,
    evaluate_orientations,
};
use alice_bamboo::overhang::{OverhangConfig, OverhangReport, analyze_overhang};
use alice_bamboo::print_export::{ExportStats, PrintConfig};
use alice_bamboo::safety::{SafetyReport, safety_validate};
use alice_sdf::io::threemf::export_3mf;
use alice_sdf::mesh::{
    DualContouringConfig, MarchingCubesConfig, MeshRepair, dual_contouring, sdf_to_mesh,
};
use alice_sdf::tight_aabb::{TightAabbConfig, compute_tight_aabb_with_config};
use anyhow::Result;
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
    /// DfAM 測定 + 判定 (`alice_bamboo::dfam`、3MF export のみ)
    ///
    /// 壁厚 p05 / 突起 / 穴径 / ブリッジ / サポート比 / 単位疑義 / watertight
    /// FDM 限界 (ISO/ASTM 52910 準拠の保守値) と突き合わせた findings
    pub dfam_summary: Option<DfamSummary>,
    /// 印刷可能性の **証明ベース** 判定 (`alice_sdf::validity` +
    /// `alice_bamboo::law`、3MF export のみ)
    ///
    /// 肉厚と連結性は「標本で見つからなかった」を合格にしない judgement が
    /// 要るので、[`DfamSummary`] (標本測定) ではなくこちらが canonical
    pub printability_summary: Option<PrintabilitySummary>,
    /// G-code slicer summary (populated only for `ExportFormat::Gcode`
    /// exports via `alice_print::slice_sdf`)
    pub slice_summary: Option<SliceSummary>,
    /// Handle to the just-generated mesh, for the in-app preview viewer
    ///
    /// Populated by the 3MF export path — the same mesh that was written
    /// to disk is shared here via `Arc` so the viewer can render it
    /// without re-running marching cubes Other export formats leave this
    /// `None`
    pub preview_mesh: Option<std::sync::Arc<alice_sdf::mesh::Mesh>>,
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

/// Compact DfAM report kept in `MeshStats` for UI display + LoRA share
///
/// findings は重大度順 (watertight → scale → 壁厚 → 突起 → 穴 → ブリッジ →
/// サポート) 各行は `"DfAM {check}: {message}"` 形式で、`fix_prompt` の
/// `from_message` がこの prefix で分類する
#[derive(Debug, Clone)]
pub struct DfamSummary {
    /// `Fail` が 0 件
    pub ok: bool,
    /// 単位疑義 (true なら寸法系判定は保留されている)
    pub units_suspect: bool,
    /// 壁厚 p05 (mm)
    pub wall_p05_mm: Option<f32>,
    /// 最小囲われ空隙 (mm)
    pub min_hole_mm: Option<f32>,
    /// 最大下向き span (mm)
    pub max_bridge_mm: f32,
    /// サポート面積比 `[0,1]`
    pub support_ratio: f32,
    /// `(verdict label, "DfAM {check}: {message}")` 全 finding
    pub findings: Vec<(String, String)>,
    /// `Fail` の finding メッセージのみ (retry / share 用)
    pub fail_messages: Vec<String>,
    /// 向き探索で現在より有意 (20 %+) にサポートが減る候補があれば
    /// `"rotate: -Z up → support 312 → 0 mm² (-100%), height 10.0 mm"` 形式のヒント
    pub orientation_hint: Option<String>,
}

impl DfamSummary {
    fn orientation_hint(report: &OrientationReport) -> Option<String> {
        if !report.materially_better {
            return None;
        }
        let cur = report.current.support_area_mm2;
        let best = &report.best;
        let pct = if cur > 0.0 {
            (1.0 - best.support_area_mm2 / cur) * 100.0
        } else {
            0.0
        };
        Some(format!(
            "rotate: {} → support {:.0} → {:.0} mm² (-{:.0}%), height {:.1} mm",
            best.label, cur, best.support_area_mm2, pct, best.build_height_mm
        ))
    }

    fn from_report(report: &DfamReport, orientation: &OrientationReport) -> Self {
        let findings: Vec<(String, String)> = report
            .findings
            .iter()
            .map(|f| {
                (
                    f.verdict.to_string(),
                    format!("DfAM {}: {}", f.check, f.message),
                )
            })
            .collect();
        let fail_messages = report
            .findings
            .iter()
            .filter(|f| f.verdict == Verdict::Fail)
            .map(|f| format!("DfAM {}: {}", f.check, f.message))
            .collect();
        Self {
            ok: report.ok,
            units_suspect: report.facts.scale.units_suspect,
            wall_p05_mm: report.facts.thickness.map(|t| t.p05_mm),
            min_hole_mm: report.facts.hole.min_diameter_mm,
            max_bridge_mm: report.facts.bridge.max_span_mm,
            support_ratio: report.facts.support.ratio,
            findings,
            fail_messages,
            orientation_hint: Self::orientation_hint(orientation),
        }
    }
}

/// text-to-print の DfAM 設定 (Bambu H2D = FDM、Z-up)
fn dfam_config() -> DfamConfig {
    DfamConfig::for_process(Process::Fdm)
}

/// 三値判定の結果ラベル (合格 / 違反 / 未決定)
///
/// 「標本で見つからなかった」を合格に繰り上げないために、証明が取れた合格と
/// 判定できなかった状態を別のラベルにする ([`alice_sdf::validity`] /
/// `alice_bamboo::law` の三値をそのまま持ち上げる)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProofVerdict {
    /// 証明付きで合格
    Proved,
    /// 反例付きで違反
    Violated,
    /// 検証器の解像度 / 区間演算の精度が足りず決まらなかった (合格ではない)
    Undecided,
    /// 前提が揃わず実施していない (内部点が 2 個未満 等)
    NotRun,
}

impl ProofVerdict {
    /// **証明付きで合格した時だけ** true
    ///
    /// 未決定 / 未実施は合格ではない ここを `!matches!(self, Violated)` に
    /// 緩めると「検証器が決められなかった」が「問題なし」に化けるので、この
    /// 関数が本 judge の要点 (`tests::only_proved_verdicts_are_acceptable`)
    #[must_use]
    pub const fn is_proved(self) -> bool {
        matches!(self, Self::Proved)
    }

    /// UI / JSON 用の安定した slug
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Proved => "proved",
            Self::Violated => "violated",
            Self::Undecided => "undecided",
            Self::NotRun => "not_run",
        }
    }
}

/// 印刷可能性の証明ベース判定 (`alice_sdf::validity` + `alice_bamboo::law`)
///
/// [`DfamSummary`] との役割分担 (canonical source を 1 つにする):
///
/// | 量 | canonical source | 理由 |
/// |---|---|---|
/// | 肉厚 | 本 struct (`erosion` + `min_local_thickness_mm`) | 区間演算の証明 + 三角形ごとの厳密 march、標本の隙間で薄壁を見逃さない |
/// | 連結性 | 本 struct (`connectivity`) | 「2 つに分かれた造形物」= 印刷すると分解する |
/// | 穴径 / 突起 / ブリッジ / サポート比 / 単位疑義 | [`DfamSummary`] | 証明版が無い量 (mesh 由来の統計) |
/// | overhang 角 | 両方 (本 struct は閉形式、DfAM は面積比) | 閉形式は判定に使わない (曲面形状で常時発火するため記録のみ) |
///
/// 各 fail 行は `"Printability {check}: {message}"` 形式で、`fix_prompt` の
/// `from_message` がこの prefix で分類する
#[derive(Debug, Clone)]
pub struct PrintabilitySummary {
    /// 肉厚 / 連結性の判定が全て `Proved` かつ薄い三角形 0 枚
    pub ok: bool,
    /// 判定に使った最小肉厚 (mm、`ProcessLimits::min_unsupported_wall_mm`)
    pub min_wall_mm: f32,
    /// A — erosion による大域判定 (三値)
    pub erosion: ProofVerdict,
    /// B — 三角形ごとの厳密な局所肉厚の最小値 (mm)
    pub min_local_thickness_mm: Option<f32>,
    /// B — `min_wall_mm` を下回る三角形の枚数
    pub thin_triangles: usize,
    /// 2 点間到達性による連結性判定 (三値)
    pub connectivity: ProofVerdict,
    /// 最大 overhang 角 (deg、閉形式 `asin(-n·b)`) 記録のみ
    pub max_overhang_deg: f32,
    /// `max_overhang` を超える三角形の枚数 記録のみ
    pub overhang_triangles: usize,
    /// retry / share に流す fail メッセージ
    pub fail_messages: Vec<String>,
    /// 未決定 (証明できなかった) の注記 retry trigger にはしない
    pub notes: Vec<String>,
}

/// 連結性判定の格子解像度 (`Reachable` のセル分類、軸あたり)
///
/// `Reachable` の三値は解像度に依存する: 通路がセル幅より細いと内部確定セルが
/// 0 個になり「未定」が返る (合格でも違反でもない) 未定は検証器の解像度不足で
/// あって繋がっている証拠ではないので、**粗い方から順に上げて未定が消えるかを
/// 見る** (ALICE-LOL 実装者からの助言、2026-09-28) 最後まで未定なら未定を報告
/// する
const CONNECTIVITY_RESOLUTIONS: [usize; 2] = [16, 32];

/// 「造形物が 1 つに繋がっているか」を 2 点間到達性で判定する
///
/// 内部点のうち最も離れた 2 点を取り、`alice_bamboo::law` の `Reachable` に
/// 掛ける 内部と確定したセルだけの flood fill で届けば合格 (経路が証拠)、
/// 外部と確定していないセル全部でも届かなければ違反 (= 2 つ以上に分かれて
/// いる証明) その間は未決定
///
/// 全解像度で端点が取れなければ未実施 ([`ProofVerdict::NotRun`])
/// 戻り値 = (三値, 未決定セル数, 決着した解像度)
fn connectivity_verdict(
    sdf: &alice_sdf::SdfNode,
    bmin: Vec3,
    bmax: Vec3,
) -> (ProofVerdict, usize, usize) {
    let mut last = (ProofVerdict::NotRun, 0, 0);
    for resolution in CONNECTIVITY_RESOLUTIONS {
        // 端点は判定と **同じ格子・同じ基準** で選ぶ 解像度を上げるとセルが
        // 薄くなって内部確定セルが現れるので、粗い側で取れなくても次を試す
        let Some((from, to)) = farthest_interior_pair(sdf, bmin, bmax, resolution) else {
            continue;
        };
        let report = alice_bamboo::law::LawSet::new()
            .reachable("connectivity", sdf.clone(), from, to)
            .check(&alice_bamboo::law::CheckConfig {
                aabb_min: bmin,
                aabb_max: bmax,
                resolution,
            });
        if !report.violations.is_empty() {
            return (ProofVerdict::Violated, 0, resolution);
        }
        if !report.has_unresolved() {
            return (ProofVerdict::Proved, 0, resolution);
        }
        last = (ProofVerdict::Undecided, report.unresolved.len(), resolution);
    }
    last
}

/// 区間演算で **セル全体が内部と確定した** セルの中心から、互いに最も離れた
/// 2 点を近似で取る
///
/// 重心から最遠の点 → その点から最遠の点 の 2 pass (直径の標準近似) 2 つに
/// 分かれた形なら別々の塊から 1 点ずつ選ばれるので、到達性判定の入力として
/// 意味がある
///
/// 端点の選び方を判定器 (`law` の `Reachable`) と同じ基準に揃えるのが要点
/// `Reachable` はセル単位で内外を確定させるので、端点は「内部確定セルの中」に
/// なければ flood fill が始点にも終点にも到達できない 以前は「bound 全体の
/// セル**対角**より深い格子点」という等方の距離ヒューリスティクスで代用して
/// いたが、これは**薄い方向に薄いセル**を拾えず、Z 厚 0.8mm の板でも肉厚 5mm
/// の球殻でも端点 0 個 = 判定を素通り (`NotRun`) していた (2026-09-28 実測)
/// `eval_interval(cell).hi < 0` はセル全体が内部であることの厳密な十分条件
/// なので、非等方セルでもそのまま効く
fn farthest_interior_pair(
    sdf: &alice_sdf::SdfNode,
    bmin: Vec3,
    bmax: Vec3,
    resolution: usize,
) -> Option<(Vec3, Vec3)> {
    use alice_sdf::interval::{Vec3Interval, eval_interval};

    #[allow(clippy::cast_precision_loss)]
    let cell = (bmax - bmin) / resolution as f32;
    let mut inside: Vec<Vec3> = Vec::new();
    for ix in 0..resolution {
        for iy in 0..resolution {
            for iz in 0..resolution {
                #[allow(clippy::cast_precision_loss)]
                let lo = bmin + cell * Vec3::new(ix as f32, iy as f32, iz as f32);
                let hi = lo + cell;
                // セル全体が内部にあることの **厳密な十分条件** (区間の上限が負)
                if eval_interval(sdf, Vec3Interval::from_bounds(lo, hi)).hi < 0.0 {
                    inside.push((lo + hi) * 0.5);
                }
            }
        }
    }
    if inside.len() < 2 {
        return None;
    }
    let centroid = inside.iter().fold(Vec3::ZERO, |a, b| a + *b) / inside.len() as f32;
    let far = |origin: Vec3| -> Vec3 {
        inside
            .iter()
            .copied()
            .fold((f32::NEG_INFINITY, inside[0]), |(best, bp), p| {
                let d = origin.distance_squared(p);
                if d > best { (d, p) } else { (best, bp) }
            })
            .1
    };
    let a = far(centroid);
    let b = far(a);
    if a.distance_squared(b) <= f32::EPSILON {
        return None;
    }
    Some((a, b))
}

/// 印刷可能性を証明ベースで判定する ([`PrintabilitySummary`])
///
/// `bmin` / `bmax` は mesh 化に使った padding 済みの bound (形状を包む)
fn printability_summary(
    sdf: &alice_sdf::SdfNode,
    mesh: &alice_sdf::mesh::Mesh,
    bmin: Vec3,
    bmax: Vec3,
) -> PrintabilitySummary {
    use alice_sdf::validity::{ErosionVerdict, PrintRequirements, validate_for_printing};

    let limits = dfam_config().limits;
    // 肉厚の閾値は DfAM と同じ値を使う (canonical source は 1 つ、
    // `SafetyViolationKind::WallTooThin` の LLM 向け directive も 1.6mm)
    let req = PrintRequirements {
        min_wall: limits.min_unsupported_wall_mm,
        max_overhang: limits
            .self_supporting_angle_deg
            .unwrap_or(45.0)
            .to_radians(),
        build_direction: Vec3::Z,
        ..PrintRequirements::fdm_0_4_nozzle()
    };
    let report = validate_for_printing(sdf, mesh, bmin, bmax, req);

    let erosion = match report.erosion {
        ErosionVerdict::HasThickEnoughRegion { .. } => ProofVerdict::Proved,
        ErosionVerdict::EntirelyTooThin => ProofVerdict::Violated,
        ErosionVerdict::Undecided => ProofVerdict::Undecided,
    };
    let (connectivity, undecided_cells, decided_at) = connectivity_verdict(sdf, bmin, bmax);

    let mut fail_messages = Vec::new();
    let mut notes = Vec::new();
    if erosion == ProofVerdict::Violated {
        fail_messages.push(format!(
            "Printability wall thickness: 造形物のどこも {:.1}mm 未満 (erosion で残る材料なしを証明)",
            req.min_wall
        ));
    }
    if report.thin_triangles > 0 {
        let min = report
            .min_local_thickness
            .map_or_else(|| "計測不能".to_string(), |t| format!("{t:.2}mm"));
        fail_messages.push(format!(
            "Printability wall thickness: 最小局所肉厚 {min} < {:.1}mm (薄い三角形 {} 枚、三角形ごとの厳密 march)",
            req.min_wall, report.thin_triangles
        ));
    }
    if connectivity == ProofVerdict::Violated {
        fail_messages.push(
            "Printability connectivity: 造形物が 2 つ以上に分かれている (2 点間の到達不能を証明)"
                .to_string(),
        );
    }
    if erosion == ProofVerdict::Undecided {
        notes.push(format!(
            "肉厚の大域判定が未決定 (erosion octree 深さ {} で決着せず、合格ではない)",
            req.erosion_depth
        ));
    }
    match connectivity {
        ProofVerdict::Undecided => notes.push(format!(
            "連結性が未決定 (未決定セル {undecided_cells} 件を経由すれば繋がる、格子解像度 {decided_at} まで上げても決着せず) 未決定は「繋がっている証拠」ではない"
        )),
        ProofVerdict::NotRun => {
            notes.push(format!(
                "連結性は未実施 (解像度 {CONNECTIVITY_RESOLUTIONS:?} のどれでも内部と確定したセルが 2 個未満 = 形状が bound に対して細すぎる) 未実施は「繋がっている証拠」ではない"
            ));
        }
        ProofVerdict::Proved | ProofVerdict::Violated => {}
    }

    PrintabilitySummary {
        // fail の不在だけでは合格にしない (未決定 / 未実施は証明ではない)
        ok: fail_messages.is_empty() && erosion.is_proved() && connectivity.is_proved(),
        min_wall_mm: req.min_wall,
        erosion,
        min_local_thickness_mm: report.min_local_thickness,
        thin_triangles: report.thin_triangles,
        connectivity,
        max_overhang_deg: report.max_overhang.to_degrees(),
        overhang_triangles: report.overhang_triangles,
        fail_messages,
        notes,
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ExportFormat {
    ThreeMf,
    Fbx,
    Stl,
    /// STEP AP214 (ISO 10303-21) — CAD kernel neutral format Backed by
    /// `alice_sdf::io::step::export_step`, which writes a faceted BREP
    /// (`CLOSED_SHELL` → `MANIFOLD_SOLID_BREP` → shape representation,
    /// mm units) — a box goes out as 6 exact planar faces, everything
    /// else is tessellated 三角形 1 枚が平面なので、球は
    /// `SPHERICAL_SURFACE` ではなく分割として届く 構造は ALICE-SDF 側の
    /// 読み戻し oracle で検証済、**実 CAD での import は未検証**
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
    ///
    /// Preview は Bamboo canonical (`~/ALICE-Bamboo/examples/compute_pattern_scores.rs`
    /// 全 13 pattern で `resolution: 96` 統一) と同じ 96 に揃え、大型 template
    /// (SKADIS panel 300×300 等) の mesh gen 時間を短縮 (128³ = 2.1M → 96³ = 885K、
    /// 2.4x sample 削減)
    pub fn mesh_resolution(self) -> usize {
        match self {
            Self::Preview => 96,
            Self::High => 192,
            Self::Ultra => 256,
        }
    }
}

/// LOL ソースを検証（パースできるか）
pub fn validate_lol(lol_source: &str) -> Result<()> {
    alice_bamboo::parse_lol(lol_source)
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("LOL parse error: {}", e.message))
}

/// Strip repeated `"LOL parse error:"` prefixes from a nested error string
///
/// `alice_bamboo::lol_to_sdf` internally wraps `parse_lol` with a
/// `"LOL parse error: {e}"` prefix, and callers here historically wrapped
/// again — producing `"LOL parse error: LOL parse error: LOL parse error at
/// pos N: ..."` triple-nested strings that are hard to read in UI toasts
/// and log tails Normalise to a single leading `"LOL parse error:"`
/// followed by the raw `ParseError` `Display` output (2026-09-04 fix)
fn dedup_lol_parse_prefix<D: std::fmt::Display>(err: D) -> String {
    let raw = err.to_string();
    let mut s = raw.trim();
    let prefix = "LOL parse error:";
    // Strip 0-many leading `LOL parse error:` occurrences
    while let Some(rest) = s.strip_prefix(prefix) {
        s = rest.trim_start();
    }
    format!("{prefix} {s}")
}

/// Compute an empirical AABB from a mesh's vertex positions
///
/// Used as a 2-pass sanity check against `compute_tight_aabb_with_config`
/// which uses interval arithmetic and can massively overestimate for
/// rotated shapes (`rotate(θ, 0, 0, ...)` widens Y/Z intervals even for
/// shapes with tight actual bounds) The empirical AABB from an initial
/// coarse pass gives us the true geometric extent, which the caller can
/// use to re-mesh at proper cell size (2026-09-04 fix)
///
/// Returns `None` if the mesh has no vertices (SDF empty at given bounds)
fn compute_empirical_aabb(mesh: &alice_sdf::mesh::Mesh) -> Option<(Vec3, Vec3)> {
    let first = mesh.vertices.first()?;
    let mut min = first.position;
    let mut max = first.position;
    for v in &mesh.vertices[1..] {
        min = min.min(v.position);
        max = max.max(v.position);
    }
    Some((min, max))
}

/// Ratio at which to trigger a 2nd-pass re-mesh Empirical AABB below this
/// fraction of tight_aabb triggers re-mesh at empirical bounds Set to 2.0
/// so a shape reporting 8x-15x inflated Y/Z (rotate around X pattern)
/// always re-meshes, but small over-estimates (< 2x) don't pay 2x cost
const AABB_REMESH_RATIO: f32 = 2.0;

/// Padding for the 2nd-pass re-mesh (2026-09-06 fix、5mm)
///
/// Must exceed the 1st-pass cell size so features under-sampled by the
/// coarse 1st pass can be recovered 1st pass at tight_aabb (~500mm) with
/// res 96 gives cell size ~5.2mm on inflated axes, so features within
/// ~5mm of the empirical mesh AABB may have been dropped Padding must
/// give the 2nd pass enough room to re-scan those under-sampled edges
///
/// Setting to 1mm (initial 2-pass fix) was too tight — thin tilted walls
/// lost their top ~4mm even after re-mesh (2026-09-06 スマホスタンド事案)
const AABB_2ND_PASS_PADDING_MM: f32 = 5.0;

/// Decide whether the tight_aabb result is significantly inflated vs
/// the empirical mesh AABB, warranting a 2nd-pass re-mesh
///
/// Returns `true` when any axis dimension is >2x the empirical extent
/// (indicates interval-arithmetic loss on rotated shapes)
fn aabb_significantly_inflated(
    tight_min: Vec3,
    tight_max: Vec3,
    empirical_min: Vec3,
    empirical_max: Vec3,
) -> bool {
    let tight_dims = tight_max - tight_min;
    let emp_dims = empirical_max - empirical_min;
    for axis in 0..3 {
        let t = tight_dims[axis].max(1e-3);
        let e = emp_dims[axis].max(1e-3);
        if t / e > AABB_REMESH_RATIO {
            return true;
        }
    }
    false
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
            let stats = alice_bamboo::print_export::lol_to_fbx(lol_source, &output_path, &config)?;
            Ok(to_mesh_stats(&stats))
        }
        ExportFormat::Stl => {
            let config = quality.to_print_config();
            let stats = alice_bamboo::print_export::lol_to_stl(lol_source, &output_path, &config)?;
            Ok(to_mesh_stats(&stats))
        }
        ExportFormat::Step => export_step_via_alice_sdf(lol_source, &output_path, quality),
        ExportFormat::Gcode => export_gcode_via_alice_print(lol_source, &output_path),
    }
}

/// Preview-only LOL → Mesh path (Gallery Phase 3 preview stub 解除)
///
/// Skips safety validation, overhang analysis, and 3MF export — returns
/// just the mesh for in-app viewer display Uses the same DC/MC aspect
/// ratio branching as `export_3mf_via_bamboo` so preview matches export
///
/// # Errors
/// - LOL parse error (invalid DSL from the shared post)
pub fn preview_lol_to_mesh(
    lol_source: &str,
    quality: Quality,
) -> Result<std::sync::Arc<alice_sdf::mesh::Mesh>> {
    let sdf = alice_bamboo::lol_to_sdf(lol_source)
        .map_err(|e| anyhow::anyhow!("{}", dedup_lol_parse_prefix(e)))?;
    let aabb_config = TightAabbConfig::preset_large();
    let aabb = compute_tight_aabb_with_config(&sdf, &aabb_config);
    let (mesh, _bounds) = mesh_with_remesh(&sdf, aabb.min, aabb.max, quality)?;
    let mesh = repair_mesh(&mesh);
    Ok(std::sync::Arc::new(mesh))
}

/// 固体探索 (区間評価の八分木) の `eval_interval` 回数の上限 (超えたら実測方式に退避する)
const SOLID_SEARCH_BUDGET: usize = 400_000;
/// 探索 1 段の分割 (各軸 2^6 = 64 分割、単位は最大 64^3 個)
const SOLID_SEARCH_LEVELS: u32 = 6;
/// 成分の AABB で再探索して絞り込む段数の上限
const SOLID_SEARCH_MAX_REFINE: usize = 3;
/// 別々にメッシュする成分数の上限 (成分ごとに格子を張るので、超えたら 1 つにまとめる)
const SOLID_SEARCH_MAX_CLUSTERS: usize = 8;
/// 成分ごとのメッシュ bounds の padding (mm)
const CLUSTER_PADDING_MM: f32 = 1.0;

/// 区間 `iv` の箱が固体を含まないと **証明できる** か (`f > 0` が箱の全域で成り立つ)
///
/// 下限が NaN (評価不能) のときは証明できないので排除しない (保守側)
fn interval_excludes(iv: alice_sdf::interval::Interval) -> bool {
    iv.lo > 0.0
}

/// 領域 `[rmin, rmax]` で、`f <= 0` の固体が存在しうる単位格子を八分木で求める
///
/// `eval_interval(box)` は箱内の全点で `lo <= f <= hi` を保証するので、`lo > 0` の箱は
/// 固体を含まない (排除) `hi < 0` の箱は全域が内部なので分割せず採用する
/// 区間が NaN のときは排除しない (保守側)
///
/// 戻り値は `64^3` 個の占有ビット (`index = (x * 64 + y) * 64 + z`)  評価回数が予算を超えたら `None`
fn solid_units(
    sdf: &alice_sdf::SdfNode,
    rmin: Vec3,
    rmax: Vec3,
    evals: &mut usize,
) -> Option<Vec<bool>> {
    use alice_sdf::interval::{Vec3Interval, eval_interval};
    let n: u32 = 1 << SOLID_SEARCH_LEVELS;
    let nu = n as usize;
    let unit = (rmax - rmin) / n as f32;
    let mut marked = vec![false; nu * nu * nu];
    let mut stack: Vec<([u32; 3], u32)> = vec![([0, 0, 0], n)];
    while let Some((idx, size)) = stack.pop() {
        *evals += 1;
        if *evals > SOLID_SEARCH_BUDGET {
            return None;
        }
        let lo = rmin + unit * Vec3::new(idx[0] as f32, idx[1] as f32, idx[2] as f32);
        let hi = rmin
            + unit
                * Vec3::new(
                    (idx[0] + size) as f32,
                    (idx[1] + size) as f32,
                    (idx[2] + size) as f32,
                );
        let iv = eval_interval(sdf, Vec3Interval::from_bounds(lo, hi));
        if interval_excludes(iv) {
            continue;
        }
        if size == 1 || iv.hi < 0.0 {
            for x in idx[0]..idx[0] + size {
                for y in idx[1]..idx[1] + size {
                    for z in idx[2]..idx[2] + size {
                        marked[(x as usize * nu + y as usize) * nu + z as usize] = true;
                    }
                }
            }
            continue;
        }
        let half = size / 2;
        for dx in [0, half] {
            for dy in [0, half] {
                for dz in [0, half] {
                    stack.push(([idx[0] + dx, idx[1] + dy, idx[2] + dz], half));
                }
            }
        }
    }
    Some(marked)
}

/// 占有単位を 26 近傍の連結成分に分け、成分ごとの world 軸 AABB を返す
fn unit_components(marked: &[bool], rmin: Vec3, rmax: Vec3) -> Vec<(Vec3, Vec3)> {
    let nu = 1usize << SOLID_SEARCH_LEVELS;
    let unit = (rmax - rmin) / nu as f32;
    let mut seen = vec![false; marked.len()];
    let mut out = Vec::new();
    for start in 0..marked.len() {
        if !marked[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        let mut queue = vec![start];
        let (mut lo, mut hi) = ([usize::MAX; 3], [0usize; 3]);
        while let Some(u) = queue.pop() {
            let c = [u / (nu * nu), (u / nu) % nu, u % nu];
            for a in 0..3 {
                lo[a] = lo[a].min(c[a]);
                hi[a] = hi[a].max(c[a]);
            }
            for dx in -1i64..=1 {
                for dy in -1i64..=1 {
                    for dz in -1i64..=1 {
                        let (x, y, z) = (c[0] as i64 + dx, c[1] as i64 + dy, c[2] as i64 + dz);
                        if [x, y, z].iter().any(|&v| v < 0 || v >= nu as i64) {
                            continue;
                        }
                        let v = (x as usize * nu + y as usize) * nu + z as usize;
                        if marked[v] && !seen[v] {
                            seen[v] = true;
                            queue.push(v);
                        }
                    }
                }
            }
        }
        let f = |c: [usize; 3], off: usize| {
            rmin + unit
                * Vec3::new(
                    (c[0] + off) as f32,
                    (c[1] + off) as f32,
                    (c[2] + off) as f32,
                )
        };
        out.push((f(lo, 0), f(hi, 1)));
    }
    out
}

/// 重なる (padding 込みで接する) AABB を 1 つにまとめる 別々の格子が同じ固体を二重に
/// メッシュしないよう、残る AABB は互いに padding 込みで重ならない
fn merge_overlapping(mut boxes: Vec<(Vec3, Vec3)>) -> Vec<(Vec3, Vec3)> {
    let pad = Vec3::splat(CLUSTER_PADDING_MM);
    'again: loop {
        for i in 0..boxes.len() {
            for j in i + 1..boxes.len() {
                let (a, b) = (boxes[i], boxes[j]);
                let overlap =
                    (a.0 - pad).cmple(b.1 + pad).all() && (b.0 - pad).cmple(a.1 + pad).all();
                if overlap {
                    boxes[i] = (a.0.min(b.0), a.1.max(b.1));
                    boxes.swap_remove(j);
                    continue 'again;
                }
            }
        }
        return boxes;
    }
}

fn box_volume(b: (Vec3, Vec3)) -> f32 {
    let d = (b.1 - b.0).max(Vec3::splat(1e-3));
    d.x * d.y * d.z
}

/// 固体が存在しうる領域を、連結成分ごとの AABB として **証明ベース** で求める
///
/// 区間評価の八分木 (`solid_units`) で `f > 0` の箱を排除し、残った単位を連結成分に分ける
/// 成分の AABB が領域より十分小さければ、その AABB で再探索して絞り込む
/// mesh の実測 AABB と違い、1 回目の mesh が捉え損ねた離れた部品も落とさず、回転で膨張しない
/// (健全性は `eval_interval` の包含保証のみに依る)
///
/// 評価回数が予算を超えたとき、領域が不正なときは `None` (呼び出し側は実測方式に退避する)
fn solid_clusters(sdf: &alice_sdf::SdfNode, rmin: Vec3, rmax: Vec3) -> Option<Vec<(Vec3, Vec3)>> {
    let d = rmax - rmin;
    if !(d.is_finite() && d.min_element() > 0.0) {
        return None;
    }
    fn refine(
        sdf: &alice_sdf::SdfNode,
        rmin: Vec3,
        rmax: Vec3,
        depth: usize,
        evals: &mut usize,
    ) -> Option<Vec<(Vec3, Vec3)>> {
        let marked = solid_units(sdf, rmin, rmax, evals)?;
        let comps = unit_components(&marked, rmin, rmax);
        let region = box_volume((rmin, rmax));
        let mut out = Vec::new();
        for c in comps {
            if depth < SOLID_SEARCH_MAX_REFINE && box_volume(c) < 0.5 * region {
                out.extend(refine(sdf, c.0, c.1, depth + 1, evals)?);
            } else {
                out.push(c);
            }
        }
        Some(out)
    }
    let mut evals = 0usize;
    let boxes = refine(sdf, rmin, rmax, 0, &mut evals)?;
    let mut merged = merge_overlapping(boxes);
    if merged.len() > SOLID_SEARCH_MAX_CLUSTERS {
        let all = merged.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |a, b| (a.0.min(b.0), a.1.max(b.1)),
        );
        merged = vec![all];
    }
    Some(merged)
}

/// 成分ごとの AABB を専用の格子でメッシュして連結する (返す bounds は全成分の外接)
fn mesh_clusters(
    sdf: &alice_sdf::SdfNode,
    clusters: &[(Vec3, Vec3)],
    quality: Quality,
) -> (alice_sdf::mesh::Mesh, (Vec3, Vec3)) {
    let pad = Vec3::splat(CLUSTER_PADDING_MM);
    let mut merged = alice_sdf::mesh::Mesh::new();
    let mut union = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for &(cmin, cmax) in clusters {
        let (lo, hi) = (cmin - pad, cmax + pad);
        let d = cmax - cmin;
        let part = generate_mesh(
            sdf,
            lo,
            hi,
            quality,
            should_use_dual_contouring((d.x, d.y, d.z)),
        );
        let base = u32::try_from(merged.vertices.len()).unwrap_or(u32::MAX);
        merged.vertices.extend(part.vertices);
        merged
            .indices
            .extend(part.indices.iter().map(|&i| i + base));
        union = (union.0.min(lo), union.1.max(hi));
    }
    (merged, union)
}

/// 再メッシュで bounds を広げる反復の上限 (毎回 extent を倍にし、1 回目の bounds で頭打ち)
const AABB_REMESH_MAX_GROWTH: usize = 6;

/// tight AABB から mesh を作る (膨張 / 離れた部品に対応する)  返すのは最終 mesh と、それを作った bounds
///
/// 1. `solid_clusters` が区間評価の八分木で固体の存在しうる領域を **証明** する
///    単一成分で tight AABB が膨らんでいなければ従来どおり tight AABB で 1 回メッシュする
///    それ以外 (回転で膨らむ / 離れた部品がある) は成分ごとに専用の格子でメッシュして連結する
///    (離れた部品を落とさず、長い bbox でも薄板を取りこぼさない)
/// 2. 八分木が予算を超えたときは、1 回目の mesh の実測 AABB で再メッシュする方式に退避する (下記)
///
/// tight AABB は区間演算で回転後の軸を大きく過大評価する (X 軸 30 度回転で 612 x 578 mm)
/// その cell で切った 1 回目の mesh は薄板をほとんど捉えられず、実測 AABB が実寸より遥かに
/// 小さくなる 固定 5mm の padding ではその分を回収できず、2 回目の bounds が板を切り落として
/// 体積の 73% を失った (2026-10-02 実測)  そのため
/// - 再メッシュした mesh が bounds に接している (= 切り落とされている) 間は bounds を倍々に広げる
///   (上限は 1 回目の bounds、tight AABB は保守的なので形状はその内側に収まる)
///
/// # Errors
/// 反復の上限まで広げても mesh が bounds に接したままのとき (欠損した mesh を黙って返さない)
fn mesh_with_remesh(
    sdf: &alice_sdf::SdfNode,
    tight_min: Vec3,
    tight_max: Vec3,
    quality: Quality,
) -> Result<(alice_sdf::mesh::Mesh, (Vec3, Vec3))> {
    let padding = Vec3::splat(1.0);
    let (first_min, first_max) = (tight_min - padding, tight_max + padding);
    let dims = tight_max - tight_min;
    let use_dc = should_use_dual_contouring((dims.x, dims.y, dims.z));

    // 固体が存在しうる領域を区間評価で **証明** する (mesh の実測に頼らない)
    // 単一成分で tight AABB が膨らんでいなければ従来どおり tight AABB で 1 回メッシュする
    // (通常の形状の出力は変えない)  それ以外 (回転で膨らむ / 離れた部品がある) は
    // 成分ごとに専用の格子でメッシュして連結する
    if let Some(clusters) = solid_clusters(sdf, tight_min, tight_max) {
        let unchanged = clusters.len() == 1
            && !aabb_significantly_inflated(tight_min, tight_max, clusters[0].0, clusters[0].1);
        if !clusters.is_empty() && !unchanged {
            return Ok(mesh_clusters(sdf, &clusters, quality));
        }
    }
    let mesh1 = generate_mesh(sdf, first_min, first_max, quality, use_dc);
    let Some((emp_min, emp_max)) = compute_empirical_aabb(&mesh1) else {
        return Ok((mesh1, (first_min, first_max)));
    };
    if !aabb_significantly_inflated(tight_min, tight_max, emp_min, emp_max) {
        return Ok((mesh1, (first_min, first_max)));
    }

    let res = quality.mesh_resolution().max(1) as f32;
    // padding を 1 回目の cell に比例させると 2 回目の格子が粗くなって薄板を取りこぼす
    // (45 度の 0.8mm 板で padding 21mm → 体積 -5%)  固定 5mm のままにして、
    // 切り落としは下の反復 (bounds を倍々に広げる) で回収する
    let pad = Vec3::splat(AABB_2ND_PASS_PADDING_MM);
    let (first_lo, first_hi) = (first_min.to_array(), first_max.to_array());
    let mut lo = (emp_min - pad).max(first_min).to_array();
    let mut hi = (emp_max + pad).min(first_max).to_array();
    let mut extent = emp_max - emp_min;
    info!(
        padding_mm = pad.max_element(),
        "tight_aabb inflated → re-mesh at empirical bounds"
    );
    for round in 0..=AABB_REMESH_MAX_GROWTH {
        let (lo_v, hi_v) = (Vec3::from_array(lo), Vec3::from_array(hi));
        let use_dc2 = should_use_dual_contouring((extent.x, extent.y, extent.z));
        let mesh = generate_mesh(sdf, lo_v, hi_v, quality, use_dc2);
        let Some((e_min, e_max)) = compute_empirical_aabb(&mesh) else {
            return Ok((mesh, (lo_v, hi_v)));
        };
        let (e_lo, e_hi) = (e_min.to_array(), e_max.to_array());
        let cell = ((hi_v - lo_v) / res).to_array();
        let (mut new_lo, mut new_hi) = (lo, hi);
        let mut clipped = false;
        for a in 0..3 {
            let step = (hi[a] - lo[a]) * 0.5;
            // 広げる余地のある側で、mesh が bounds の 1 cell 以内に接していれば切り落とし
            if lo[a] > first_lo[a] && e_lo[a] <= lo[a] + cell[a] {
                new_lo[a] = (lo[a] - step).max(first_lo[a]);
                clipped = true;
            }
            if hi[a] < first_hi[a] && e_hi[a] >= hi[a] - cell[a] {
                new_hi[a] = (hi[a] + step).min(first_hi[a]);
                clipped = true;
            }
        }
        if !clipped {
            return Ok((mesh, (lo_v, hi_v)));
        }
        if round == AABB_REMESH_MAX_GROWTH {
            anyhow::bail!(
                "mesh が bounds で切り落とされたまま収束しなかった (bounds {lo_v:?} .. {hi_v:?}、形状の一部が欠けている)"
            );
        }
        extent = e_max - e_min;
        (lo, hi) = (new_lo, new_hi);
    }
    unreachable!("loop は round == AABB_REMESH_MAX_GROWTH で必ず return / bail する")
}

/// 境界エッジ (1 枚の三角形にしか共有されない無向エッジ) の本数
fn count_boundary_edges(mesh: &alice_sdf::mesh::Mesh) -> usize {
    let mut edges: std::collections::HashMap<(u32, u32), u32> = std::collections::HashMap::new();
    for t in mesh.indices.as_chunks::<3>().0 {
        for (u, v) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *edges.entry((u.min(v), u.max(v))).or_insert(0) += 1;
        }
    }
    edges.values().filter(|&&n| n == 1).count()
}

/// `MeshRepair::repair_all` を掛けるが、境界エッジを増やしたら修復前の mesh を採用する
///
/// 薄板では `repair_all` が水密な mesh の三角形を落として穴を開けることがある
/// (X 軸 45 度回転の 0.8mm 板で 845 三角形を落として境界エッジ 567 本、2026-10-02 実測)
fn repair_mesh(mesh: &alice_sdf::mesh::Mesh) -> alice_sdf::mesh::Mesh {
    let repaired = MeshRepair::repair_all(mesh, 5e-3);
    let (before, after) = (count_boundary_edges(mesh), count_boundary_edges(&repaired));
    if after > before {
        tracing::warn!(
            before,
            after,
            "repair_all が境界エッジを増やしたので修復前の mesh を採用する"
        );
        return mesh.clone();
    }
    repaired
}

/// Mesh generation helper (MC or DC dispatch based on `use_dc`)
///
/// Shared between preview_lol_to_mesh + export_3mf_via_bamboo so the
/// 2-pass empirical AABB re-mesh logic works identically in both paths
fn generate_mesh(
    sdf: &alice_sdf::SdfNode,
    min_bounds: Vec3,
    max_bounds: Vec3,
    quality: Quality,
    use_dc: bool,
) -> alice_sdf::mesh::Mesh {
    if use_dc {
        let cfg = DualContouringConfig {
            resolution: quality.mesh_resolution(),
            compute_normals: true,
            ..DualContouringConfig::default()
        };
        dual_contouring(sdf, min_bounds, max_bounds, &cfg)
    } else {
        let cfg = MarchingCubesConfig {
            resolution: quality.mesh_resolution(),
            ..Default::default()
        };
        sdf_to_mesh(sdf, min_bounds, max_bounds, &cfg)
    }
}

/// mesh 生成経路 (Dual Contouring vs Marching Cubes) を bbox 寸法から判定
///
/// 判定基準 (OR 論理):
/// - `aspect_ratio (= max_dim / min_dim) > 5.0`: 板状 / 棒状 (peg 穴等の小 feature を保存)
/// - `min_dim <= 5.0mm`: 薄物 (Hermite data で watertight 保証)
///
/// Bamboo canonical (`~/ALICE-Bamboo/pattern_scores.json`) の route 判定と一致
/// SKADIS panel 300×300 (bbox 312×17×312) は aspect 18.4 → DC 経路に落ちる
fn should_use_dual_contouring(dims: (f32, f32, f32)) -> bool {
    let max_dim = dims.0.max(dims.1).max(dims.2);
    let min_dim = dims.0.min(dims.1).min(dims.2);
    let aspect_ratio = max_dim / min_dim.max(1e-3);
    aspect_ratio > 5.0 || min_dim <= 5.0
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
/// 5. `alice_bamboo::bambu_3mf::export_bambu_3mf` — write Bambu template-embedded 3MF
///    (MakerWorld 対応、Phase 5.4 で `alice_sdf::io::threemf::export_3mf` から切替、
///    Phase 3''.2 実測に基づき薄物 (< 5mm) は Dual Contouring、厚物は MC 自動判定)
fn export_3mf_via_bamboo(
    lol_source: &str,
    output_path: &Path,
    quality: Quality,
) -> Result<MeshStats> {
    let sdf = alice_bamboo::lol_to_sdf(lol_source)
        .map_err(|e| anyhow::anyhow!("{}", dedup_lol_parse_prefix(e)))?;

    let safety_report = safety_validate(&sdf, "PLA", None);
    if !safety_report.is_safe {
        info!(
            material = safety_report.material_name,
            warp_category = ?safety_report.warp.category,
            "safety report flagged issues (informational, export continues)"
        );
    }
    let safety_summary = SafetySummary::from_report(&safety_report);

    // 2026-08-23: alice-sdf 1.7.7 breaking change (preset + try_new に統一、Default 削除)
    // 500mm bbox / iter 24 / subdivisions 16 は preset_large() の canonical 値
    let aabb_config = TightAabbConfig::preset_large();
    let aabb = compute_tight_aabb_with_config(&sdf, &aabb_config);

    // Phase 5.4 → 5.5 (2026-08-08): 経路判定を AABB Y 軸単独から aspect ratio ベースに拡張
    // 「薄物 vs 厚物」は AABB Y だけでなく shape aspect ratio で決定 (SKADIS panel 300×300 は
    // 本体 5mm + peg 補強で AABB Y=17mm、旧判定では MC 経路に落ちて Ø5mm peg 穴が解像度不足で
    // 消失した事案) 詳細判定ロジック: `should_use_dual_contouring`
    let dims = (
        aabb.max.x - aabb.min.x,
        aabb.max.y - aabb.min.y,
        aabb.max.z - aabb.min.z,
    );
    let use_dc = should_use_dual_contouring(dims);
    let (max_dim, min_dim) = (
        dims.0.max(dims.1).max(dims.2),
        dims.0.min(dims.1).min(dims.2),
    );
    info!(
        thickness_y = dims.1,
        max_dim = max_dim,
        min_dim = min_dim,
        aspect_ratio = max_dim / min_dim.max(1e-3),
        use_dc = use_dc,
        resolution = quality.mesh_resolution(),
        "mesh route selected (aspect_ratio > 5.0 OR min_dim <= 5.0 → DC)"
    );

    // 1st pass + 必要なら再メッシュ (`mesh_with_remesh`)
    // `mesh_bounds` = 最終 mesh を作った bound 証明ベースの印刷可能性判定
    // (`printability_summary`) は「この bound の外は見ない」ので、mesh と同じ
    // bound を渡さないと形状の一部を検査しないことになる
    let (mesh, mesh_bounds) = mesh_with_remesh(&sdf, aabb.min, aabb.max, quality)?;
    let mesh = repair_mesh(&mesh);

    let overhang_report = analyze_overhang(&mesh, &OverhangConfig::default());
    let overhang_summary = OverhangSummary::from_report(&overhang_report);

    // DfAM 測定 + 判定 (2026-09-14、text-to-cad dfam-check 吸収 + SDF 実測 3 項目)
    let dfam_report = alice_bamboo::dfam::analyze(&sdf, &mesh, &dfam_config());
    if !dfam_report.ok {
        info!(
            fails = dfam_report
                .findings
                .iter()
                .filter(|f| f.verdict == Verdict::Fail)
                .count(),
            "DfAM report flagged failures (informational, export continues)"
        );
    }
    // 向き探索: 軸整列 6 + 球面 32 (mesh は回さない、軸射影のみ)
    let orientation =
        evaluate_orientations(&mesh, &OrientationConfig::from_dfam(&dfam_config(), 32));
    if orientation.materially_better {
        info!(
            best = %orientation.best.label,
            current_support_mm2 = orientation.current.support_area_mm2,
            best_support_mm2 = orientation.best.support_area_mm2,
            "orientation search found a materially better build direction"
        );
    }
    let dfam_summary = DfamSummary::from_report(&dfam_report, &orientation);

    // 証明ベースの印刷可能性判定 (肉厚 = erosion 証明 + 厳密局所 march、連結性 =
    // 2 点間到達性) mesh と同じ bound を渡す
    let printability = printability_summary(&sdf, &mesh, mesh_bounds.0, mesh_bounds.1);
    if !printability.ok {
        info!(
            erosion = printability.erosion.slug(),
            connectivity = printability.connectivity.slug(),
            thin_triangles = printability.thin_triangles,
            notes = printability.notes.len(),
            "printability proof did not pass (informational, export continues)"
        );
    }

    let vertex_count = mesh.vertices.len();
    let triangle_count = mesh.indices.len() / 3;

    // 2026-08-07 revert: 一律 Y-up→Z-up 変換を試みたが、hand-crafted skadis panel
    // 等の Z-up 前提 LOL (rounded_box(150,150,2.5) 型 = Z が厚さ) を全て regress
    // させるため撤回 真因は LOL grammar 内で Z-up と Y-up 慣習が混在すること、
    // 修正は LLM 側 (system_prompt.md に Z-up 慣習明示) で対処 skadis / coin 等の
    // hand-crafted example は変換なしで既に Bambu Z-up と一致する
    //
    // Phase 5.4: Bambu template embedded 3MF (MakerWorld 対応)、素の 3MF から切替
    let name = output_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("model");
    export_bambu_3mf(&mesh, output_path, name)
        .map_err(|e| anyhow::anyhow!("Bambu 3MF export error: {e}"))?;

    let preview_mesh = std::sync::Arc::new(mesh);
    Ok(MeshStats {
        vertex_count,
        triangle_count,
        path: output_path.to_string_lossy().into_owned(),
        overhang_summary: Some(overhang_summary),
        safety_summary: Some(safety_summary),
        dfam_summary: Some(dfam_summary),
        printability_summary: Some(printability),
        slice_summary: None,
        preview_mesh: Some(preview_mesh),
    })
}

fn to_mesh_stats(stats: &ExportStats) -> MeshStats {
    MeshStats {
        vertex_count: stats.vertex_count,
        triangle_count: stats.triangle_count,
        path: stats.path.clone(),
        overhang_summary: None,
        safety_summary: None,
        dfam_summary: None,
        printability_summary: None,
        slice_summary: None,
        preview_mesh: None,
    }
}

/// LOL → STEP (ISO 10303-21 AP214) via `alice_sdf::io::step::export_step`
///
/// The SDF is tessellated internally by `export_step` using its own
/// marching-cubes pass then written as a faceted BREP solid We rebuild
/// the mesh separately to surface a vertex / triangle count in
/// `MeshStats` for the UI That path is small (order of megabytes) so
/// the duplicate work is acceptable
fn export_step_via_alice_sdf(
    lol_source: &str,
    output_path: &Path,
    quality: Quality,
) -> Result<MeshStats> {
    let sdf = alice_bamboo::lol_to_sdf(lol_source)
        .map_err(|e| anyhow::anyhow!("{}", dedup_lol_parse_prefix(e)))?;

    // Vertex / triangle counts via a preview-quality mesh — the exported
    // STEP file uses its own internal tessellation but this at least
    // gives the UI a rough size estimate
    // 2026-08-23: alice-sdf 1.7.7 breaking change (preset + try_new に統一、Default 削除)
    // 500mm bbox / iter 24 / subdivisions 16 は preset_large() の canonical 値
    let aabb_config = TightAabbConfig::preset_large();
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
        dfam_summary: None,
        printability_summary: None,
        slice_summary: None,
        preview_mesh: None,
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
        .map_err(|e| anyhow::anyhow!("{}", dedup_lol_parse_prefix(e)))?;
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
        dfam_summary: None,
        printability_summary: None,
        slice_summary: Some(SliceSummary {
            layer_count: slice.layer_count,
            filament_meters: slice.filament_meters,
            print_time_seconds: slice.print_time_seconds,
        }),
        preview_mesh: None,
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
/// `text_to_print_llm::backend::generate_with_retry`
///
/// Behavior:
/// - **LOL parse error** returns a single-message violation vector so the
///   retry loop can request a syntactically valid LOL DSL
/// - **`safety_validate.is_safe == true`** returns an empty vector — the
///   caller is expected to interpret `[]` as "no retry needed"
/// - **`safety_validate.is_safe == false`** returns
///   `safety_validate.messages` verbatim The messages already come from
///   `alice_bamboo::safety::SafetyReport` so
///   `text_to_print_llm::fix_prompt::SafetyViolationKind::from_message`
///   can classify them into `fix_directive` instructions
///
/// This helper deliberately skips the mesh build (marching cubes) so it
/// stays cheap enough to be invoked between LLM retries — the actual
/// mesh gets built later during the final `export_mesh` call
#[must_use]
pub fn safety_check_lol(lol_source: &str) -> Vec<String> {
    let sdf = match alice_bamboo::lol_to_sdf(lol_source) {
        Ok(sdf) => sdf,
        Err(e) => return vec![dedup_lol_parse_prefix(e)],
    };
    let report = alice_bamboo::safety::safety_validate(&sdf, "PLA", None);
    let mut violations = if report.is_safe {
        Vec::new()
    } else {
        report.messages
    };

    // DfAM (標本測定) + Printability (証明) の fail を retry loop に流す
    // LLM が LOL 側で直せる種類の違反に限定 低解像度 mesh (Preview) で十分
    // (LLM 1 回の推論が分単位なのに対し、この測定は秒未満)
    violations.extend(printability_fail_messages_for_retry(&sdf));
    violations
}

/// retry loop 用の軽量判定 — Preview 解像度で mesh を 1 回作り、DfAM の `Fail`
/// (穴径 / 突起) と Printability の fail (肉厚 / 連結性) を返す
fn printability_fail_messages_for_retry(sdf: &alice_sdf::SdfNode) -> Vec<String> {
    let aabb = compute_tight_aabb_with_config(sdf, &TightAabbConfig::preset_large());
    let dims = (
        aabb.max.x - aabb.min.x,
        aabb.max.y - aabb.min.y,
        aabb.max.z - aabb.min.z,
    );
    if !(dims.0.is_finite() && dims.1.is_finite() && dims.2.is_finite())
        || dims.0 <= 0.0
        || dims.1 <= 0.0
        || dims.2 <= 0.0
    {
        return Vec::new();
    }
    let Ok((mesh, bounds)) = mesh_with_remesh(sdf, aabb.min, aabb.max, Quality::Preview) else {
        return Vec::new();
    };
    let mesh = repair_mesh(&mesh);
    let mut cfg = dfam_config();
    cfg.grid_resolution = 48;
    cfg.max_thickness_samples = 20_000;
    let report = alice_bamboo::dfam::analyze(sdf, &mesh, &cfg);
    let mut out: Vec<String> = report
        .findings
        .iter()
        .filter(|f| f.verdict == Verdict::Fail && is_retryable_dfam_check(f.check))
        .map(|f| format!("DfAM {}: {}", f.check, f.message))
        .collect();

    // 肉厚 / 連結性は証明ベース側が canonical (`PrintabilitySummary` の表参照)
    out.extend(printability_summary(sdf, &mesh, bounds.0, bounds.1).fail_messages);
    out
}

/// LLM retry に流す DfAM 項目
///
/// LOL 側で局所的に直せるもの (穴を大きく / 突起を太く) に限定する
///
/// - **wall thickness**: 2026-09-28 に除外 肉厚の canonical source は
///   `alice_sdf::validity` 側 (erosion の証明 + 三角形ごとの厳密 march) に
///   移した DfAM の壁厚は mesh 頂点からのレイキャスト標本 p05 なので、
///   標本の隙間にある薄壁を「見つからなかった = 合格」にできる 同じ量を
///   2 つの source で判定すると片方が緩い方に倒れるため 1 つに寄せる
/// - bridge: 丸い形状 (球 / ドーム) の下半分が常に >45° 下向き = span が出る
///   ため、retry trigger にすると曲面を持つ形状すべてで再生成が走る
/// - watertight: MC / DC / `repair_all` という **mesher の性質** であって LOL
///   設計の問題ではない (`sphere(10)` を Preview 解像度 MC で mesh 化しても
///   非多様体辺が残る実測あり) LLM に LOL を書き換えさせても解決しない
///
/// bridge / watertight は UI + manifest で示すに留める
fn is_retryable_dfam_check(check: alice_bamboo::dfam::Check) -> bool {
    use alice_bamboo::dfam::Check;
    matches!(check, Check::PositiveFeature | Check::HoleDiameter)
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
    dfam: Option<&DfamSummary>,
    printability: Option<&PrintabilitySummary>,
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
    if let Some(d) = dfam {
        for msg in &d.fail_messages {
            if !caller.contains(msg) {
                caller.push(msg.clone());
            }
        }
    }
    if let Some(p) = printability {
        for msg in &p.fail_messages {
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
    let safety_violations = merge_safety_violations(
        meta.safety_violations,
        stats.safety_summary.as_ref(),
        stats.dfam_summary.as_ref(),
        stats.printability_summary.as_ref(),
    );

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
        .map_err(|e| anyhow::anyhow!("{}", dedup_lol_parse_prefix(e)))?;

    // 2026-08-23: alice-sdf 1.7.7 breaking change (preset + try_new に統一、Default 削除)
    // 500mm bbox / iter 24 / subdivisions 16 は preset_large() の canonical 値
    let aabb_config = TightAabbConfig::preset_large();
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
///
/// v0.1.0-beta.1 (2026-08-07): grammar constrained decoding OFF 時、
/// LLM は自由 format で返すため fence tag が多様 (`lol` / `rust` / `code`
/// / 無し / json wrap) 対応:
///   1. ` ```lol ` block (system prompt が要求する canonical form)
///   2. ` ```rust ` / ` ``` ` (fence tag なし) block
///   3. JSON `{"code": "..."}` wrap の抽出
///   4. どれも無ければ raw を返し、caller の `parse_lol` に判定を委ねる
pub fn extract_lol(llm_response: &str) -> Option<String> {
    // 1. ```lol ... ``` (canonical)
    if let Some(extracted) = extract_fenced(llm_response, "```lol") {
        return Some(balance_parens(&extracted));
    }
    // 2. ```rust / ```code / bare ``` (LLM が fence tag を省略/変更した場合)
    for fence in ["```rust", "```code", "```lol\n", "```"] {
        if let Some(extracted) = extract_fenced(llm_response, fence) {
            return Some(balance_parens(&extracted));
        }
    }
    // 3. JSON `{"code": "..."}` or `{"lol": "..."}` wrap
    if let Some(extracted) = extract_json_code(llm_response) {
        return Some(balance_parens(&extracted));
    }
    None
}

/// LLM 出力の paren balance 自動修復
///
/// Qwen 2.5 3B / 小型 LLM は nested paren の close 数を 1-2 個外す
/// 実測傾向あり (2026-08-07 iGPU + 3B model で trailing `)` 過剰 fail)
///
/// 修復方針:
/// - `)` 過剰 (close > open): 末尾の余分な `)` を strip
/// - `(` 過剰 (open > close): 不足分の `)` を末尾に append
/// - equal: 無変更
///
/// LOL DSL は string literal を持たないので naive count で十分
/// LOL 文法違反自体は救えない (`translate(0 0 0 sphere(5))` のような
/// comma 抜けは balance でも救えず、parser で reject)
pub fn balance_parens(source: &str) -> String {
    let trimmed = source.trim();
    let open = trimmed.chars().filter(|c| *c == '(').count();
    let close = trimmed.chars().filter(|c| *c == ')').count();

    match open.cmp(&close) {
        std::cmp::Ordering::Equal => trimmed.to_string(),
        std::cmp::Ordering::Less => {
            // close 過剰: 末尾から (close - open) 個の `)` を落とす
            // 末尾 whitespace / 途中 `)` に挟まれた non-`)` は保持
            let excess = close - open;
            let mut removed = 0_usize;
            let rescued: String = trimmed
                .chars()
                .rev()
                .filter(|c| {
                    if *c == ')' && removed < excess {
                        removed += 1;
                        false
                    } else {
                        true
                    }
                })
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            tracing::warn!(
                excess_close = excess,
                "balance_parens: stripped {} trailing ')' from LLM output",
                excess
            );
            rescued
        }
        std::cmp::Ordering::Greater => {
            // open 過剰: 末尾に (open - close) 個の `)` を追加
            let missing = open - close;
            tracing::warn!(
                missing_close = missing,
                "balance_parens: appended {} ')' to LLM output",
                missing
            );
            format!("{trimmed}{}", ")".repeat(missing))
        }
    }
}

fn extract_fenced(text: &str, fence: &str) -> Option<String> {
    let start = text.find(fence)?;
    let code_start = text[start + fence.len()..]
        .find('\n')
        .map(|i| i + start + fence.len() + 1)?;
    let end = text[code_start..].find("```")? + code_start;
    Some(text[code_start..end].trim().to_string())
}

/// JSON `{"code": "..."}` / `{"lol": "..."}` から LOL DSL を抽出
///
/// Qwen 3B が Text-to-CAD prompt に対して JSON wrap で返してくる pattern
/// (system prompt に反するが実測で発生) の救済
fn extract_json_code(text: &str) -> Option<String> {
    let brace_start = text.find('{')?;
    let brace_end = text.rfind('}')?;
    if brace_end <= brace_start {
        return None;
    }
    let json_slice = &text[brace_start..=brace_end];
    let value: serde_json::Value = serde_json::from_str(json_slice).ok()?;
    for key in ["code", "lol", "dsl", "output", "result"] {
        if let Some(v) = value.get(key).and_then(|v| v.as_str()) {
            return Some(v.trim().to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- 固体領域の証明 (solid_clusters) と成分ごとのメッシュ (mesh_clusters) ----
    // 独立の参照は閉形式の幾何: X 軸まわりに θ 度傾けた薄板 box3d(15, 15, 0.4) の内部の点は
    // (x, u, w) in [-15, 15] x [-15, 15] x [-0.4, 0.4] から y = u cosθ - w sinθ、z = u sinθ + w cosθ で作れる

    fn plate_tilted(deg: f32) -> alice_sdf::SdfNode {
        alice_bamboo::lol_to_sdf(&format!(
            "rotate({deg:.1}, 0.0, 0.0, box3d(15.0, 15.0, 0.4))"
        ))
        .expect("parse")
    }

    fn tight_of(sdf: &alice_sdf::SdfNode) -> (Vec3, Vec3) {
        let a = compute_tight_aabb_with_config(sdf, &TightAabbConfig::preset_large());
        (a.min, a.max)
    }

    fn contains(b: (Vec3, Vec3), p: Vec3) -> bool {
        p.cmpge(b.0).all() && p.cmple(b.1).all()
    }

    #[test]
    fn solid_clusters_never_exclude_a_solid_point_of_a_tilted_plate() {
        // 健全性: 板の内部の点 (閉形式で生成) は必ずいずれかの成分 AABB に入る
        for deg in [15.0_f32, 30.0, 45.0, 60.0] {
            let sdf = plate_tilted(deg);
            let (tmin, tmax) = tight_of(&sdf);
            let clusters = solid_clusters(&sdf, tmin, tmax).expect("search within budget");
            let (s, c) = deg.to_radians().sin_cos();
            let mut checked = 0;
            for xi in 0..=6 {
                for ui in 0..=12 {
                    for wi in 0..=2 {
                        let x = -15.0 + 30.0 * xi as f32 / 6.0;
                        let u = -15.0 + 30.0 * ui as f32 / 12.0;
                        let w = -0.4 + 0.8 * wi as f32 / 2.0;
                        let p = Vec3::new(x, u * c - w * s, u * s + w * c);
                        // 参照の自己検査: 生成した点は本当に f <= 0 (境界上は丸めの余裕を見る)
                        assert!(alice_sdf::eval(&sdf, p) <= 1e-3, "{deg}deg: 生成点が板の外");
                        assert!(
                            clusters.iter().any(|&b| contains(b, p)),
                            "{deg}deg: 固体の点 {p:?} がどの成分 AABB にも入らない (排除が不健全)"
                        );
                        checked += 1;
                    }
                }
            }
            assert_eq!(checked, 7 * 13 * 3);
        }
    }

    #[test]
    fn solid_clusters_of_a_tilted_plate_is_one_tight_box() {
        // tight 性: 成分は 1 つで、閉形式の半 extent を含み、余りは小さい (tight AABB は数百 mm に膨らむ)
        for deg in [15.0_f32, 30.0, 45.0, 60.0] {
            let sdf = plate_tilted(deg);
            let (tmin, tmax) = tight_of(&sdf);
            let clusters = solid_clusters(&sdf, tmin, tmax).expect("within budget");
            assert_eq!(clusters.len(), 1, "{deg}deg: 板は 1 成分のはず");
            let (cmin, cmax) = clusters[0];
            let (s, c) = deg.to_radians().sin_cos();
            let want = [
                15.0,
                15.0 * c.abs() + 0.4 * s.abs(),
                15.0 * s.abs() + 0.4 * c.abs(),
            ];
            for (a, &w) in want.iter().enumerate() {
                let (lo, hi) = (cmin.to_array()[a], cmax.to_array()[a]);
                assert!(
                    lo <= -w + 1e-3 && hi >= w - 1e-3,
                    "{deg}deg axis {a}: [{lo:.3}, {hi:.3}] が閉形式 ±{w:.3} を含まない"
                );
                assert!(
                    -w - lo < 1.5 && hi - w < 1.5,
                    "{deg}deg axis {a}: [{lo:.3}, {hi:.3}] の余りが大きい (閉形式 ±{w:.3})"
                );
            }
        }
    }

    #[test]
    fn solid_clusters_separates_far_parts_and_merges_overlapping_ones() {
        let parse = |lol: &str| alice_bamboo::lol_to_sdf(lol).expect("parse");
        let count = |lol: &str| {
            let sdf = parse(lol);
            let (tmin, tmax) = tight_of(&sdf);
            solid_clusters(&sdf, tmin, tmax).expect("within budget")
        };
        let plate = "box3d(15.0, 15.0, 0.4)";
        // 薄板 + 120mm 離れた球 = 2 成分、球の成分は球 (中心 y=120、半径 3) を含む
        let far = count(&format!(
            "union({plate}, translate(0.0, 120.0, 0.0, sphere(3.0)))"
        ));
        assert_eq!(far.len(), 2, "離れた部品は別成分: {far:?}");
        assert!(
            far.iter().any(|&b| contains(b, Vec3::new(0.0, 123.0, 0.0))
                && contains(b, Vec3::new(0.0, 117.0, 0.0))),
            "球を含む成分が無い: {far:?}"
        );
        // 板に食い込む球 = 1 成分
        let near = count(&format!(
            "union({plate}, translate(0.0, 0.0, 0.5, sphere(3.0)))"
        ));
        assert_eq!(near.len(), 1, "重なる部品は 1 成分: {near:?}");
        // 板のみ = 1 成分
        assert_eq!(count(plate).len(), 1);
    }

    #[test]
    fn interval_excludes_only_a_box_proven_strictly_positive() {
        use alice_sdf::interval::Interval;
        let iv = |lo: f32, hi: f32| Interval { lo, hi };
        assert!(interval_excludes(iv(0.001, 5.0)), "全域で f > 0 は排除");
        assert!(
            !interval_excludes(iv(0.0, 5.0)),
            "下限 0 は f = 0 の点を含みうる"
        );
        assert!(
            !interval_excludes(iv(-1.0, 1.0)),
            "符号をまたぐ箱は排除しない"
        );
        assert!(!interval_excludes(iv(-5.0, -1.0)), "内部の箱は排除しない");
        assert!(
            !interval_excludes(iv(f32::NAN, 1.0)),
            "NaN は証明にならないので排除しない"
        );
        assert!(interval_excludes(iv(f32::INFINITY, f32::INFINITY)));
    }

    #[test]
    fn solid_units_gives_up_when_the_evaluation_budget_is_spent() {
        let sdf = plate_tilted(30.0);
        let mut evals = SOLID_SEARCH_BUDGET;
        assert!(solid_units(&sdf, Vec3::splat(-20.0), Vec3::splat(20.0), &mut evals).is_none());
        let mut fresh = 0;
        assert!(solid_units(&sdf, Vec3::splat(-20.0), Vec3::splat(20.0), &mut fresh).is_some());
    }

    #[test]
    fn solid_clusters_rejects_a_degenerate_region() {
        let sdf = plate_tilted(30.0);
        assert!(solid_clusters(&sdf, Vec3::ZERO, Vec3::ZERO).is_none());
        assert!(solid_clusters(&sdf, Vec3::ZERO, Vec3::new(1.0, 0.0, 1.0)).is_none());
        assert!(solid_clusters(&sdf, Vec3::ZERO, Vec3::splat(f32::NAN)).is_none());
    }

    #[test]
    fn unit_components_use_26_connectivity() {
        let n = 1usize << SOLID_SEARCH_LEVELS;
        let at = |x: usize, y: usize, z: usize| (x * n + y) * n + z;
        let (rmin, rmax) = (Vec3::ZERO, Vec3::splat(n as f32)); // 1 単位 = 1mm
        // 角でだけ接する 2 単位 (対角) は 1 成分
        let mut g = vec![false; n * n * n];
        g[at(1, 1, 1)] = true;
        g[at(2, 2, 2)] = true;
        let c = unit_components(&g, rmin, rmax);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0], (Vec3::splat(1.0), Vec3::splat(3.0)));
        // 1 単位空けると 2 成分
        let mut g = vec![false; n * n * n];
        g[at(1, 1, 1)] = true;
        g[at(3, 1, 1)] = true;
        assert_eq!(unit_components(&g, rmin, rmax).len(), 2);
        // 何も無ければ成分なし
        assert!(unit_components(&vec![false; n * n * n], rmin, rmax).is_empty());
    }

    #[test]
    fn merge_overlapping_joins_boxes_that_touch_within_the_padding() {
        let b = |a: f32, c: f32| (Vec3::new(a, 0.0, 0.0), Vec3::new(c, 1.0, 1.0));
        // padding 1mm: 間隔 1.5mm (< 2mm) は接する、間隔 3mm は別
        let joined = merge_overlapping(vec![b(0.0, 1.0), b(2.5, 3.5)]);
        assert_eq!(
            joined,
            vec![(Vec3::new(0.0, 0.0, 0.0), Vec3::new(3.5, 1.0, 1.0))]
        );
        assert_eq!(merge_overlapping(vec![b(0.0, 1.0), b(4.0, 5.0)]).len(), 2);
        // 連鎖: A-B が接し B-C が接すれば 3 つとも 1 つ
        assert_eq!(
            merge_overlapping(vec![b(0.0, 1.0), b(2.5, 3.5), b(5.0, 6.0)]).len(),
            1
        );
    }

    #[test]
    fn mesh_clusters_concatenates_parts_with_correct_index_offsets() {
        let sdf =
            alice_bamboo::lol_to_sdf("union(sphere(5.0), translate(40.0, 0.0, 0.0, sphere(5.0)))")
                .expect("parse");
        let (tmin, tmax) = tight_of(&sdf);
        let clusters = solid_clusters(&sdf, tmin, tmax).expect("within budget");
        assert_eq!(clusters.len(), 2);
        let (mesh, bounds) = mesh_clusters(&sdf, &clusters, Quality::Preview);
        assert!(
            mesh.indices
                .iter()
                .all(|&i| (i as usize) < mesh.vertices.len())
        );
        assert_eq!(count_boundary_edges(&mesh), 0, "連結後も各球は水密");
        // 体積 = 2 球 (4/3 π 5³ x 2 = 1047.2)
        let vol: f64 = mesh
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| {
                let p = |k: usize| mesh.vertices[t[k] as usize].position.as_dvec3();
                p(0).dot(p(1).cross(p(2))) / 6.0
            })
            .sum();
        let want = 2.0 * 4.0 / 3.0 * std::f64::consts::PI * 125.0;
        assert!(
            (vol - want).abs() / want < 0.03,
            "体積 {vol:.2} (期待 {want:.2})"
        );
        // bounds は両方の球を含む
        assert!(
            contains(bounds, Vec3::new(-5.0, 0.0, 0.0))
                && contains(bounds, Vec3::new(45.0, 0.0, 0.0))
        );
    }

    // ---- 再メッシュの切り落とし回収 (mesh_with_remesh) と repair の水密維持 (repair_mesh) ----
    // X 軸まわりに θ 度傾けた薄板 box3d(15, 15, 0.4) の半 extent は閉形式で
    // y = 15|cosθ| + 0.4|sinθ|、z = 15|sinθ| + 0.4|cosθ| (x は 15)
    // tight AABB が膨らんで 1 回目の mesh が板を捉えられなくても、最終 mesh の AABB は
    // この閉形式に cell size 以内で届かなければならない (切り落とされていれば届かない)

    fn tilted_plate_half_extents(deg: f32) -> [f32; 3] {
        let (s, c) = deg.to_radians().sin_cos();
        [
            15.0,
            15.0 * c.abs() + 0.4 * s.abs(),
            15.0 * s.abs() + 0.4 * c.abs(),
        ]
    }

    #[test]
    fn remeshed_tilted_plate_reaches_its_analytic_extent() {
        for deg in [15.0_f32, 30.0, 45.0, 60.0] {
            let lol = format!("rotate({deg:.1}, 0.0, 0.0, box3d(15.0, 15.0, 0.4))");
            let mesh = preview_lol_to_mesh(&lol, Quality::Preview).expect("preview mesh");
            let (min, max) = compute_empirical_aabb(&mesh).expect("non-empty mesh");
            let want = tilted_plate_half_extents(deg);
            for (axis, w) in want.iter().enumerate() {
                let (lo, hi) = (min.to_array()[axis], max.to_array()[axis]);
                // 表面の頂点は真の extent の内側に cell 以内で届く (上側は丸め程度の超過のみ)
                assert!(
                    (hi - w).abs() < 0.5 && (lo + w).abs() < 0.5,
                    "{deg}deg axis {axis}: [{lo:.3}, {hi:.3}] が閉形式 ±{w:.3} に届かない (切り落とし)"
                );
            }
        }
    }

    #[test]
    fn count_boundary_edges_matches_hand_counted_topologies() {
        let mut m = alice_sdf::mesh::Mesh::new();
        m.vertices = [Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z]
            .iter()
            .map(|&p| alice_sdf::mesh::Vertex::new(p, Vec3::Z))
            .collect();
        // 単独の三角形: 3 辺すべてが境界
        m.indices = vec![0, 1, 2];
        assert_eq!(count_boundary_edges(&m), 3);
        // 辺 (1, 2) を共有する 2 枚: 共有辺を除く 4 本が境界
        m.indices = vec![0, 1, 2, 1, 3, 2];
        assert_eq!(count_boundary_edges(&m), 4);
        // 閉じた四面体: 境界なし (全 6 辺がちょうど 2 枚に共有される)
        m.indices = vec![0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3];
        assert_eq!(count_boundary_edges(&m), 0);
        // 空 mesh
        assert_eq!(count_boundary_edges(&alice_sdf::mesh::Mesh::new()), 0);
    }

    #[test]
    fn repair_mesh_never_adds_boundary_edges() {
        // 45 度の薄板を、固定 5mm padding の旧来の 2 回目の bounds (実測 AABB ± 5mm) で
        // 直接メッシュすると、水密な mesh (境界 0) を `repair_all` が開く (境界 567 本)
        // 再メッシュの bounds を直した現在は `mesh_with_remesh` 経由ではこの mesh は出ないので、
        // `repair_mesh` のガードはこの入力でしか実効が出ない (経路を直しても防御は残す)
        let sdf = alice_bamboo::lol_to_sdf("rotate(45.0, 0.0, 0.0, box3d(15.0, 15.0, 0.4))")
            .expect("parse");
        let aabb = compute_tight_aabb_with_config(&sdf, &TightAabbConfig::preset_large());
        let pad1 = Vec3::splat(1.0);
        let mesh1 = generate_mesh(
            &sdf,
            aabb.min - pad1,
            aabb.max + pad1,
            Quality::Preview,
            true,
        );
        let (emp_min, emp_max) = compute_empirical_aabb(&mesh1).expect("1st pass mesh");
        let pad2 = Vec3::splat(AABB_2ND_PASS_PADDING_MM);
        // 経路は旧来どおり実測の寸法で決める (45 度の板は 30 x 21.8 x 21.8 mm なので MC)
        let dims = emp_max - emp_min;
        let use_dc2 = should_use_dual_contouring((dims.x, dims.y, dims.z));
        let mesh = generate_mesh(
            &sdf,
            emp_min - pad2,
            emp_max + pad2,
            Quality::Preview,
            use_dc2,
        );
        assert_eq!(count_boundary_edges(&mesh), 0, "再メッシュ直後は水密のはず");
        // 前提: repair_all 単体は水密な mesh を開く (これが成り立たなくなったら、
        // ALICE-SDF 側で修正された可能性があるので、本 test と repair_mesh の要否を見直す)
        let raw = MeshRepair::repair_all(&mesh, 5e-3);
        assert!(
            count_boundary_edges(&raw) > 0,
            "repair_all が水密な mesh を開かなくなった (ALICE-SDF で修正された?)"
        );
        let after = count_boundary_edges(&repair_mesh(&mesh));
        assert_eq!(
            after, 0,
            "repair_mesh が水密な mesh を開いた (境界エッジ {after} 本)"
        );
    }

    // ---- 2-pass remesh 判定 (compute_empirical_aabb / aabb_significantly_inflated) ----
    // 仕様は関数の doc: 前者は頂点位置の軸ごとの min / max (空 mesh は None)、
    // 後者は「いずれかの軸で tight / empirical > 2.0 (厳密に超える)」で真
    // 2 回目の再メッシュに入るかの分岐で、閾値の取り違えは回転形状の欠けに直結する

    fn mesh_of(points: &[Vec3]) -> alice_sdf::mesh::Mesh {
        let mut mesh = alice_sdf::mesh::Mesh::new();
        mesh.vertices = points
            .iter()
            .map(|&p| alice_sdf::mesh::Vertex::new(p, Vec3::Z))
            .collect();
        mesh
    }

    #[test]
    fn empirical_aabb_of_an_empty_mesh_is_none() {
        assert!(compute_empirical_aabb(&mesh_of(&[])).is_none());
    }

    #[test]
    fn empirical_aabb_of_a_single_vertex_is_that_point() {
        let p = Vec3::new(-3.5, 0.0, 7.25);
        let (min, max) = compute_empirical_aabb(&mesh_of(&[p])).expect("one vertex");
        assert_eq!((min, max), (p, p));
    }

    #[test]
    fn empirical_aabb_matches_an_independent_per_axis_reference() {
        // 決定論的な擬似乱数 (LCG) で負値を含む点群を作り、軸ごとの fold と突合する
        let mut state: u32 = 0x1234_5678;
        let mut next = || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 8) as f32 / (1u32 << 24) as f32 * 200.0 - 100.0
        };
        let pts: Vec<Vec3> = (0..257)
            .map(|_| Vec3::new(next(), next(), next()))
            .collect();
        let want_min = [
            pts.iter().map(|p| p.x).fold(f32::INFINITY, f32::min),
            pts.iter().map(|p| p.y).fold(f32::INFINITY, f32::min),
            pts.iter().map(|p| p.z).fold(f32::INFINITY, f32::min),
        ];
        let want_max = [
            pts.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max),
            pts.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max),
            pts.iter().map(|p| p.z).fold(f32::NEG_INFINITY, f32::max),
        ];
        let (min, max) = compute_empirical_aabb(&mesh_of(&pts)).expect("non-empty");
        assert_eq!(min.to_array(), want_min);
        assert_eq!(max.to_array(), want_max);
        // 頂点の順序に依らない (先頭頂点だけで初期化する実装の取りこぼしを検出)
        let mut rev = pts.clone();
        rev.reverse();
        let (rmin, rmax) = compute_empirical_aabb(&mesh_of(&rev)).expect("non-empty");
        assert_eq!((rmin, rmax), (min, max));
    }

    /// tight は原点から `t`、empirical は原点から `e` の AABB (各軸の寸法 = t, e)
    fn inflated(t: Vec3, e: Vec3) -> bool {
        aabb_significantly_inflated(Vec3::ZERO, t, Vec3::ZERO, e)
    }

    #[test]
    fn inflation_threshold_is_strictly_greater_than_two() {
        let one = Vec3::ONE;
        // 比がちょうど 2.0 は再メッシュしない (`>`、`>=` ではない)
        assert!(!inflated(Vec3::new(2.0, 1.0, 1.0), one));
        // f32 で 2.0 の次の値は再メッシュする
        let just_above = f32::from_bits(2.0_f32.to_bits() + 1);
        assert!(inflated(Vec3::new(just_above, 1.0, 1.0), one));
        // 比が 1 (膨らみなし) は再メッシュしない
        assert!(!inflated(one, one));
    }

    #[test]
    fn inflation_on_any_single_axis_triggers_the_remesh() {
        let one = Vec3::ONE;
        assert!(inflated(Vec3::new(3.0, 1.0, 1.0), one), "x のみ");
        assert!(inflated(Vec3::new(1.0, 3.0, 1.0), one), "y のみ");
        assert!(inflated(Vec3::new(1.0, 1.0, 3.0), one), "z のみ");
    }

    #[test]
    fn a_tight_aabb_smaller_than_the_empirical_one_does_not_trigger() {
        // tight < empirical (比 < 1) は膨らみではない
        assert!(!inflated(Vec3::ONE, Vec3::new(5.0, 5.0, 5.0)));
    }

    #[test]
    fn a_zero_thickness_empirical_axis_is_floored_at_1e_3() {
        // empirical 0 厚の軸は 1e-3 に floor されるので、tight が 1mm 以上あれば
        // 比 1000 で再メッシュ、tight も 0 厚なら比 1 で再メッシュしない
        assert!(inflated(Vec3::new(1.0, 1.0, 1.0), Vec3::new(1.0, 1.0, 0.0)));
        assert!(!inflated(
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0)
        ));
    }

    #[test]
    fn safety_check_lol_returns_empty_for_small_pla_sphere() {
        // small sphere with PLA material → safety_validate.is_safe == true
        // DfAM: 壁 20mm / 穴なし → retry 対象の Fail なし
        // (bridge は下半球で、watertight は MC 非多様体辺で Fail するが
        //  どちらも retry 対象外、is_retryable_dfam_check)
        let violations = safety_check_lol("sphere(10.0)");
        assert!(
            violations.is_empty(),
            "expected no violations for sphere(10), got {violations:?}"
        );
    }

    #[test]
    fn safety_check_lol_flags_thin_plate_via_printability_proof() {
        // 0.8mm 板 (box3d は half extents なので厚さ 0.8mm) → FDM min unsupported
        // wall 1.6mm を割る 2026-09-28 に肉厚の canonical source を
        // `alice_sdf::validity` に移したので prefix は "Printability wall thickness:"
        //
        // oracle: erosion は `Round { radius: -0.8 }` = d(p) + 0.8 が全域で正
        // (板の半厚 0.4mm < 0.8mm) なので残る材料なし = EntirelyTooThin を証明できる
        let violations = safety_check_lol("box3d(15.0, 15.0, 0.4)");
        assert!(
            violations
                .iter()
                .any(|v| v.starts_with("Printability wall thickness:")),
            "expected Printability wall thickness violation, got {violations:?}"
        );
        assert!(
            !violations
                .iter()
                .any(|v| v.starts_with("DfAM wall thickness:")),
            "肉厚は証明側が canonical、DfAM 側は retry に流さない: {violations:?}"
        );
    }

    /// 厚い球は肉厚も連結性も **証明付き** 合格になる (未決定に倒れない)
    ///
    /// oracle: `sphere(10)` を 1.6mm erode しても半径 9.2mm の球が残るので
    /// 「1.6mm 以上の肉厚を持つ領域がある」は八分木で必ず証明できる
    #[test]
    fn printability_proves_a_thick_sphere() {
        let sdf = alice_bamboo::lol_to_sdf("sphere(10.0)").expect("parse");
        let b = Vec3::splat(12.0);
        let mesh = generate_mesh(&sdf, -b, b, Quality::Preview, false);
        let p = printability_summary(&sdf, &mesh, -b, b);
        assert_eq!(p.erosion, ProofVerdict::Proved, "notes: {:?}", p.notes);
        assert_eq!(p.connectivity, ProofVerdict::Proved, "notes: {:?}", p.notes);
        assert_eq!(p.thin_triangles, 0);
        assert!(p.fail_messages.is_empty(), "{:?}", p.fail_messages);
        assert!(p.ok);
    }

    /// 離れた 2 球は「到達不能」を証明して retry に流れる (= 印刷すると分解する)
    ///
    /// oracle: 中心間 40mm / 半径 5mm なので間に 30mm の空隙があり、内部と
    /// 確定したセルだけでは到達できず、外部と確定していないセルを全部使っても
    /// 繋がらない
    #[test]
    fn printability_detects_two_disconnected_solids() {
        let lol = "union(translate(-20, 0, 0, sphere(5.0)), translate(20, 0, 0, sphere(5.0)))";
        let sdf = alice_bamboo::lol_to_sdf(lol).expect("parse");
        let (bmin, bmax) = (Vec3::new(-27.0, -7.0, -7.0), Vec3::new(27.0, 7.0, 7.0));
        let mesh = generate_mesh(&sdf, bmin, bmax, Quality::Preview, false);
        let p = printability_summary(&sdf, &mesh, bmin, bmax);
        assert_eq!(
            p.connectivity,
            ProofVerdict::Violated,
            "notes: {:?}",
            p.notes
        );
        assert!(
            p.fail_messages
                .iter()
                .any(|m| m.starts_with("Printability connectivity:")),
            "{:?}",
            p.fail_messages
        );
        assert!(!p.ok);
    }

    /// 繋がった 2 球は連結性が合格 (上の test と同じ形で距離だけ変えた対照)
    ///
    /// 同じ `union` でも重なっていれば 1 つなので、検出が「union を見たら違反」
    /// ではなく実際の到達性を見ていることを押さえる
    #[test]
    fn printability_accepts_two_overlapping_solids() {
        let lol = "union(translate(-3, 0, 0, sphere(5.0)), translate(3, 0, 0, sphere(5.0)))";
        let sdf = alice_bamboo::lol_to_sdf(lol).expect("parse");
        let b = Vec3::new(11.0, 7.0, 7.0);
        let mesh = generate_mesh(&sdf, -b, b, Quality::Preview, false);
        let p = printability_summary(&sdf, &mesh, -b, b);
        assert_eq!(p.connectivity, ProofVerdict::Proved, "notes: {:?}", p.notes);
        assert!(
            !p.fail_messages
                .iter()
                .any(|m| m.starts_with("Printability connectivity:")),
            "{:?}",
            p.fail_messages
        );
    }

    /// 内部と確定したセルが 2 個未満なら連結性は `NotRun` (勝手に合格にしない)
    ///
    /// `NotRun` は「形状が bound の中に無い」ような退避経路で、**薄物では出ない**
    /// (薄板 / 5mm 殻が決着することは上の 3 本が押さえている 2026-09-28 より
    /// 前は薄物も細物も全部ここに落ちていた)
    #[test]
    fn connectivity_is_not_run_without_two_interior_cells() {
        // bound の外にある球 = どのセルも内部と確定しない
        let sdf = alice_bamboo::lol_to_sdf("translate(1000, 0, 0, sphere(1.0))").expect("parse");
        let (verdict, _, _) = connectivity_verdict(&sdf, Vec3::splat(-5.0), Vec3::splat(5.0));
        assert_eq!(verdict, ProofVerdict::NotRun);
    }

    /// 三値のうち **`Proved` だけ** が合格 (未決定 / 未実施を合格にしない)
    ///
    /// `ok` の組み立てが将来 `fail_messages.is_empty()` だけに単純化されると
    /// 「検証器が決められなかった」が「問題なし」に化ける ここは型で守れない
    /// ので 4 variant を列挙して固定する
    #[test]
    fn only_proved_verdicts_are_acceptable() {
        assert!(ProofVerdict::Proved.is_proved());
        assert!(!ProofVerdict::Violated.is_proved());
        assert!(!ProofVerdict::Undecided.is_proved());
        assert!(!ProofVerdict::NotRun.is_proved());
    }

    /// fail が 1 つも無くても、証明が揃わなければ合格にしない
    ///
    /// oracle: 肉厚 2.0mm の球殻 (`sphere(10) - sphere(8)`) は min_wall 1.6mm を
    /// 上回るので薄い三角形は 0 枚、連結性も `Proved` それでも erosion の大域
    /// 判定は octree 深さ 6 では決着せず `Undecided` で残る (2026-09-28 実測の
    /// 境界: 2.5mm → `Proved` / 2.0mm → 未決定で fail 0 件 / 1.6mm → 薄い三角形
    /// 59,024 枚で違反)
    ///
    /// 「探索で見つからなかった」を合格に繰り上げないことが本 judge の要点なので、
    /// **fail が空 かつ `ok == false` かつ notes で理由が出る** をここで固定する
    /// erosion の深さを上げてこの形状が `Proved` になったら本 test は red になる
    /// (その時は境界となる別の肉厚に付け替える)
    #[test]
    fn undecided_proof_is_not_promoted_to_ok() {
        let sdf = alice_bamboo::lol_to_sdf("subtract(sphere(10.0), sphere(8.0))").expect("parse");
        let b = Vec3::splat(12.0);
        let mesh = generate_mesh(&sdf, -b, b, Quality::Preview, false);
        let p = printability_summary(&sdf, &mesh, -b, b);
        assert_eq!(p.erosion, ProofVerdict::Undecided, "notes: {:?}", p.notes);
        assert_eq!(p.thin_triangles, 0);
        assert_eq!(p.connectivity, ProofVerdict::Proved, "notes: {:?}", p.notes);
        assert!(
            p.fail_messages.is_empty(),
            "この形状は fail が出ない前提: {:?}",
            p.fail_messages
        );
        assert!(!p.ok, "未決定を合格に繰り上げてはいけない");
        assert!(
            p.notes.iter().any(|n| n.contains("未決定")),
            "未決定の理由が surface されていない: {:?}",
            p.notes
        );
    }

    /// 離れた 2 枚の**薄板**は「到達不能」を証明する (分離検出は薄物でこそ要る)
    ///
    /// oracle: 板は Z 厚 0.8mm (`box3d` の Z half extent 0.4)、X は
    /// `[-30, -10]` と `[10, 30]` で間に 20mm の空隙 空隙のセルは外部と確定
    /// するので内部確定セルだけでは届かず、外部と確定していないセルを全部
    /// 使っても届かない
    ///
    /// 2026-09-28 の穴: 端点を「bound 全体のセル**対角**より深い格子点」から
    /// 選んでいたため、薄物では内部点が 0 個 → `NotRun` で判定を素通りして
    /// いた t2p の主力形状は薄物 (DC 経路 = 5mm 以下、SKADIS panel / coin)
    /// なので、そこで judge が動かないと分離検出に意味がない
    #[test]
    fn printability_detects_two_separate_thin_plates() {
        let lol = "union(translate(-20, 0, 0, box3d(10.0, 10.0, 0.4)), translate(20, 0, 0, box3d(10.0, 10.0, 0.4)))";
        let sdf = alice_bamboo::lol_to_sdf(lol).expect("parse");
        let (bmin, bmax) = (Vec3::new(-32.0, -12.0, -2.0), Vec3::new(32.0, 12.0, 2.0));
        let (verdict, cells, res) = connectivity_verdict(&sdf, bmin, bmax);
        assert_eq!(
            verdict,
            ProofVerdict::Violated,
            "薄板 2 枚の分離が検出できていない (未決定セル {cells}, 解像度 {res})"
        );
    }

    /// 1 枚の薄板は連結性が決着して合格 (上と同じ厚さ、離していないだけが差)
    ///
    /// 「薄いと必ず未実施」でも「union を見たら違反」でもないことを押さえる
    #[test]
    fn printability_proves_a_single_thin_plate_is_connected() {
        let sdf = alice_bamboo::lol_to_sdf("box3d(15.0, 15.0, 0.4)").expect("parse");
        let (bmin, bmax) = (Vec3::new(-17.0, -17.0, -2.0), Vec3::new(17.0, 17.0, 2.0));
        let (verdict, cells, res) = connectivity_verdict(&sdf, bmin, bmax);
        assert_eq!(
            verdict,
            ProofVerdict::Proved,
            "1 枚板の連結性が決着していない (未決定セル {cells}, 解像度 {res})"
        );
    }

    /// 肉厚 5mm の殻も連結性が決着する
    ///
    /// oracle: `sphere(12) - sphere(7)` は肉厚 5mm の球殻で、殻は 1 つに
    /// 繋がっている 2026-09-28 実測ではここも `NotRun` だった (殻の内部深さ
    /// 2.5mm < bound セル対角 3.0mm) = 判定不能は薄物だけの問題ではなかった
    #[test]
    fn printability_proves_a_5mm_shell_is_connected() {
        let sdf = alice_bamboo::lol_to_sdf("subtract(sphere(12.0), sphere(7.0))").expect("parse");
        let b = Vec3::splat(14.0);
        let (verdict, cells, res) = connectivity_verdict(&sdf, -b, b);
        assert_eq!(
            verdict,
            ProofVerdict::Proved,
            "5mm 殻の連結性が決着していない (未決定セル {cells}, 解像度 {res})"
        );
    }

    #[test]
    fn export_3mf_orientation_hint_for_table_shape() {
        // テーブル (脚 2 本 + 天板) は上下反転でサポートが消える → hint あり
        // LOL の box3d は half_extents 指定 (ALICE-Bamboo CLAUDE.md § half_extents ルール)
        let dir = tempfile::tempdir().unwrap();
        let lol = "union(union(translate(0, 0, 8.5, box3d(20, 10, 1.5)), translate(-18, 0, 5, box3d(2, 10, 5))), translate(18, 0, 5, box3d(2, 10, 5)))";
        let stats =
            export_mesh(lol, dir.path(), ExportFormat::ThreeMf, Quality::Preview).expect("export");
        let dfam = stats.dfam_summary.as_ref().expect("dfam_summary");
        let hint = dfam.orientation_hint.as_deref().expect("orientation hint");
        assert!(hint.starts_with("rotate: -Z up"), "{hint}");
        assert!(dfam.max_bridge_mm > 20.0, "{}", dfam.max_bridge_mm);
    }

    #[test]
    fn retryable_dfam_checks_exclude_bridge_and_advisories() {
        use alice_bamboo::dfam::Check;
        assert!(!is_retryable_dfam_check(Check::Watertight));
        // 肉厚は証明ベース側 (`printability_summary`) が canonical source
        assert!(!is_retryable_dfam_check(Check::WallThickness));
        assert!(is_retryable_dfam_check(Check::PositiveFeature));
        assert!(is_retryable_dfam_check(Check::HoleDiameter));
        assert!(!is_retryable_dfam_check(Check::Bridge));
        assert!(!is_retryable_dfam_check(Check::SupportRatio));
        assert!(!is_retryable_dfam_check(Check::Scale));
    }

    #[test]
    fn merge_safety_violations_appends_dfam_fails() {
        let caller = vec!["llm_retry_over_65deg".to_string()];
        let dfam = DfamSummary {
            ok: false,
            units_suspect: false,
            wall_p05_mm: Some(0.9),
            min_hole_mm: None,
            max_bridge_mm: 0.0,
            support_ratio: 0.0,
            findings: vec![],
            fail_messages: vec![
                "DfAM wall thickness: p05 wall 0.90 mm".to_string(),
                "llm_retry_over_65deg".to_string(), // 重複は追加しない
            ],
            orientation_hint: None,
        };
        let merged = merge_safety_violations(caller, None, Some(&dfam), None);
        assert_eq!(
            merged,
            vec![
                "llm_retry_over_65deg".to_string(),
                "DfAM wall thickness: p05 wall 0.90 mm".to_string()
            ]
        );
    }

    /// 証明ベース側の fail も manifest の `safety_violations` に載る
    /// (LoRA 共有の品質シグナルに「証明で落ちた」情報を残す)
    #[test]
    fn merge_safety_violations_appends_printability_fails() {
        let p = PrintabilitySummary {
            ok: false,
            min_wall_mm: 1.6,
            erosion: ProofVerdict::Violated,
            min_local_thickness_mm: Some(0.8),
            thin_triangles: 12,
            connectivity: ProofVerdict::Proved,
            max_overhang_deg: 90.0,
            overhang_triangles: 4,
            fail_messages: vec!["Printability wall thickness: ...".to_string()],
            notes: vec!["ここは note なので merge しない".to_string()],
        };
        let merged = merge_safety_violations(vec![], None, None, Some(&p));
        assert_eq!(merged, vec!["Printability wall thickness: ...".to_string()]);
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
    fn balance_parens_equal_returns_input_trimmed() {
        assert_eq!(balance_parens("sphere(1.0)"), "sphere(1.0)");
        assert_eq!(balance_parens("  sphere(1.0)  "), "sphere(1.0)");
    }

    #[test]
    fn balance_parens_strips_excess_trailing_close() {
        // 2026-08-07 実測 Qwen 2.5 3B iGPU 出力に近い pattern (open=5, close=7)
        let input = "translate(0, 0, 2, subtract(box3d(50, 50, 4), round(2, cylinder(2.5, 20)))))";
        let rescued = balance_parens(input);
        let open = rescued.chars().filter(|c| *c == '(').count();
        let close = rescued.chars().filter(|c| *c == ')').count();
        assert_eq!(open, close, "rescued output must be balanced");
        assert_eq!(open, 5);
    }

    #[test]
    fn balance_parens_appends_missing_close() {
        let input = "subtract(box3d(10, 10, 10), sphere(5)";
        let rescued = balance_parens(input);
        let open = rescued.chars().filter(|c| *c == '(').count();
        let close = rescued.chars().filter(|c| *c == ')').count();
        assert_eq!(open, close);
        assert!(rescued.ends_with("))"));
    }

    #[test]
    fn balance_parens_preserves_interior_close_when_stripping() {
        // 末尾に )) )) が並んでいても excess 分だけ落とす
        let input = "union(a(), b())"; // balanced
        assert_eq!(balance_parens(input), "union(a(), b())");
    }

    #[test]
    fn extract_lol_applies_balance_on_fenced_output() {
        let response = "```lol\nsphere(1.0))\n```"; // 1 excess close
        let extracted = extract_lol(response).unwrap();
        assert_eq!(extracted, "sphere(1.0)");
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
        assert_eq!(Quality::Preview.mesh_resolution(), 96);
        assert_eq!(Quality::High.mesh_resolution(), 192);
        assert_eq!(Quality::Ultra.mesh_resolution(), 256);
    }

    #[test]
    fn should_use_dc_for_flat_panel_shapes() {
        // SKADIS panel 300×300: bbox 312×17×312、aspect 18.4 → DC (Bamboo canonical と一致)
        assert!(should_use_dual_contouring((312.0, 17.0, 312.0)));
        // 板状 (200×2×200): min_dim 2.0 <= 5.0 → DC
        assert!(should_use_dual_contouring((200.0, 2.0, 200.0)));
        // 棒状 (100×5×5): aspect 20.0 → DC
        assert!(should_use_dual_contouring((100.0, 5.0, 5.0)));
    }

    #[test]
    fn should_use_dc_for_thin_coins() {
        // コイン Ø22.8 × 1.7mm: min_dim 1.7 <= 5.0 → DC
        assert!(should_use_dual_contouring((22.8, 1.7, 22.8)));
        // 極薄 (10×0.5×10): min_dim 0.5 → DC
        assert!(should_use_dual_contouring((10.0, 0.5, 10.0)));
    }

    #[test]
    fn should_use_mc_for_bulky_shapes() {
        // 立方体 20×20×20: aspect 1.0、min_dim 20.0 → MC
        assert!(!should_use_dual_contouring((20.0, 20.0, 20.0)));
        // 直方体 30×20×15: aspect 2.0、min_dim 15.0 → MC
        assert!(!should_use_dual_contouring((30.0, 20.0, 15.0)));
        // 中型 (50×15×30): aspect 3.33、min_dim 15.0 → MC
        assert!(!should_use_dual_contouring((50.0, 15.0, 30.0)));
    }

    #[test]
    fn should_use_dc_at_boundary_min_dim_5mm() {
        // min_dim ちょうど 5.0mm → DC (`<= 5.0`)
        assert!(should_use_dual_contouring((100.0, 5.0, 100.0)));
        // min_dim 5.1mm、aspect 100/5.1=19.6 → DC (aspect 経由)
        assert!(should_use_dual_contouring((100.0, 5.1, 100.0)));
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
        let merged = merge_safety_violations(caller.clone(), None, None, None);
        assert_eq!(merged, caller);
    }

    #[test]
    fn merge_safety_violations_safe_summary_passthrough() {
        let caller = vec!["llm_retry_over_65deg".to_string()];
        let sum = safety_summary(true, vec!["ignored_because_safe".to_string()]);
        let merged = merge_safety_violations(caller.clone(), Some(&sum), None, None);
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
        let merged = merge_safety_violations(caller, Some(&sum), None, None);
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
        let merged = merge_safety_violations(caller, Some(&sum), None, None);
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
        // DfAM summary は 3MF 経路で必ず populate、findings は重大度順で先頭 watertight
        let dfam = stats.dfam_summary.as_ref().expect("dfam_summary");
        assert!(!dfam.findings.is_empty());
        assert!(
            dfam.findings[0].1.starts_with("DfAM watertight:"),
            "{:?}",
            dfam.findings[0]
        );
        assert!(dfam.wall_p05_mm.is_some());
        assert!(!dfam.units_suspect);
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
