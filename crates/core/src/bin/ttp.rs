//! `ttp` — text-to-print headless CLI (Agent Skill 用の入口)
//!
//! GUI を経由せず LOL DSL → 3MF / STL / STEP / G-code を生成し、DfAM /
//! 向き探索 / G-code 静的検証の結果を **JSON で stdout** に出す
//! Claude Code / Codex 等のエージェントが `skills/text-to-print/SKILL.md`
//! 経由で叩く前提 (出力は機械可読、判定は呼び出し側)
//!
//! ```text
//! ttp check    --lol <file|->                       # LOL を parse + safety + DfAM (export なし)
//! ttp export   --lol <file|-> --out <dir> [--format 3mf|stl|step|gcode] [--quality preview|high|ultra]
//! ttp validate --gcode <file> [--bed h2d|h2d-dual|x1c|a1-mini]
//! ```
//!
//! 依存を増やさないため引数解析は手書き (clap 不使用)

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use text_to_print_core::pipeline::{
    DfamSummary, ExportFormat, MeshStats, Quality, export_mesh, safety_check_lol,
};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first().map(String::as_str) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let result = match cmd {
        "check" => cmd_check(&args[1..]),
        "export" => cmd_export(&args[1..]),
        "validate" => cmd_validate(&args[1..]),
        "--help" | "-h" | "help" => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        other => Err(format!("unknown command `{other}`\n{USAGE}")),
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

const USAGE: &str = "\
ttp — text-to-print headless CLI (JSON on stdout)

  ttp check    --lol <file|->
  ttp export   --lol <file|-> --out <dir> [--format 3mf|stl|step|gcode] [--quality preview|high|ultra]
  ttp validate --gcode <file> [--bed h2d|h2d-dual|x1c|a1-mini]

exit codes: 0 ok / 1 error / 3 findings failed (check: DfAM fail or safety violation; validate: not ok)";

/// `--key value` の最小パーサ (順不同、重複は後勝ち)
fn flag<'a>(args: &'a [String], key: &str) -> Option<&'a str> {
    let mut found = None;
    let mut i = 0;
    while i + 1 < args.len() {
        if args[i] == key {
            found = Some(args[i + 1].as_str());
            i += 2;
        } else {
            i += 1;
        }
    }
    found
}

fn read_lol(spec: &str) -> Result<String, String> {
    if spec == "-" {
        let mut s = String::new();
        std::io::stdin()
            .read_to_string(&mut s)
            .map_err(|e| format!("stdin: {e}"))?;
        Ok(s)
    } else {
        std::fs::read_to_string(spec).map_err(|e| format!("read {spec}: {e}"))
    }
}

fn parse_format(s: &str) -> Result<ExportFormat, String> {
    Ok(match s {
        "3mf" => ExportFormat::ThreeMf,
        "stl" => ExportFormat::Stl,
        "fbx" => ExportFormat::Fbx,
        "step" => ExportFormat::Step,
        "gcode" => ExportFormat::Gcode,
        other => return Err(format!("unknown format `{other}` (3mf|stl|fbx|step|gcode)")),
    })
}

fn parse_quality(s: &str) -> Result<Quality, String> {
    Ok(match s {
        "preview" => Quality::Preview,
        "high" => Quality::High,
        "ultra" => Quality::Ultra,
        other => return Err(format!("unknown quality `{other}` (preview|high|ultra)")),
    })
}

fn parse_bed(s: &str) -> Result<alice_bamboo::MachineBounds, String> {
    use alice_bamboo::MachineBounds;
    Ok(match s {
        "h2d" => MachineBounds::bambu_h2d_single(),
        "h2d-dual" => MachineBounds::bambu_h2d_dual(),
        "x1c" | "p1s" => MachineBounds::bambu_x1c(),
        "a1-mini" => MachineBounds::bambu_a1_mini(),
        other => return Err(format!("unknown bed `{other}` (h2d|h2d-dual|x1c|a1-mini)")),
    })
}

// ---------------------------------------------------------------------------
// check
// ---------------------------------------------------------------------------

fn cmd_check(args: &[String]) -> Result<ExitCode, String> {
    let lol = read_lol(flag(args, "--lol").ok_or("--lol <file|-> required")?)?;
    let violations = safety_check_lol(&lol);
    // export なしで DfAM 全体を見るため Preview 3MF を一時 dir に書いて summary だけ使う
    let dir = std::env::temp_dir().join(format!("ttp-check-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("tempdir: {e}"))?;
    let stats = export_mesh(&lol, &dir, ExportFormat::ThreeMf, Quality::Preview);
    let _ = std::fs::remove_dir_all(&dir);
    let stats = stats.map_err(|e| format!("{e:#}"))?;

    let dfam_ok = stats.dfam_summary.as_ref().is_none_or(|d| d.ok);
    let ok = violations.is_empty() && dfam_ok;
    let json = serde_json::json!({
        "ok": ok,
        "safety_violations": violations,
        "dfam": stats.dfam_summary.as_ref().map(dfam_json),
        "overhang_ratio": stats.overhang_summary.as_ref().map(|o| o.overhang_ratio),
        "vertex_count": stats.vertex_count,
        "triangle_count": stats.triangle_count,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?
    );
    Ok(if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(3)
    })
}

// ---------------------------------------------------------------------------
// export
// ---------------------------------------------------------------------------

fn cmd_export(args: &[String]) -> Result<ExitCode, String> {
    let lol = read_lol(flag(args, "--lol").ok_or("--lol <file|-> required")?)?;
    let out: PathBuf = flag(args, "--out").ok_or("--out <dir> required")?.into();
    let format = parse_format(flag(args, "--format").unwrap_or("3mf"))?;
    let quality = parse_quality(flag(args, "--quality").unwrap_or("preview"))?;
    std::fs::create_dir_all(&out).map_err(|e| format!("mkdir {}: {e}", out.display()))?;

    let stats = export_mesh(&lol, &out, format, quality).map_err(|e| format!("{e:#}"))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&stats_json(&stats)).map_err(|e| e.to_string())?
    );
    Ok(ExitCode::SUCCESS)
}

fn stats_json(stats: &MeshStats) -> serde_json::Value {
    serde_json::json!({
        "path": stats.path,
        "vertex_count": stats.vertex_count,
        "triangle_count": stats.triangle_count,
        "overhang": stats.overhang_summary.as_ref().map(|o| serde_json::json!({
            "ratio": o.overhang_ratio,
            "max_wall_angle_deg": o.max_wall_angle_deg,
            "area_mm2": o.total_overhang_area_mm2,
        })),
        "safety": stats.safety_summary.as_ref().map(|s| serde_json::json!({
            "material": s.material_name,
            "is_safe": s.is_safe,
            "warp_category": s.warp_category,
            "messages": s.messages,
        })),
        "dfam": stats.dfam_summary.as_ref().map(dfam_json),
        "slice": stats.slice_summary.map(|s| serde_json::json!({
            "layer_count": s.layer_count,
            "filament_meters": s.filament_meters,
            "print_time_seconds": s.print_time_seconds,
        })),
    })
}

fn dfam_json(d: &DfamSummary) -> serde_json::Value {
    serde_json::json!({
        "ok": d.ok,
        "units_suspect": d.units_suspect,
        "wall_p05_mm": d.wall_p05_mm,
        "min_hole_mm": d.min_hole_mm,
        "max_bridge_mm": d.max_bridge_mm,
        "support_ratio": d.support_ratio,
        "orientation_hint": d.orientation_hint,
        "findings": d.findings.iter().map(|(verdict, msg)| serde_json::json!({
            "verdict": verdict,
            "message": msg,
        })).collect::<Vec<_>>(),
    })
}

// ---------------------------------------------------------------------------
// validate
// ---------------------------------------------------------------------------

fn cmd_validate(args: &[String]) -> Result<ExitCode, String> {
    let path = Path::new(flag(args, "--gcode").ok_or("--gcode <file> required")?);
    let bed = parse_bed(flag(args, "--bed").unwrap_or("h2d"))?;
    let gcode =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let report = alice_bamboo::validate::validate_gcode(&gcode, &bed);
    let json = serde_json::json!({
        "ok": report.ok,
        "truncated": report.truncated,
        "stats": {
            "lines": report.stats.lines,
            "movement_moves": report.stats.movement_moves,
            "extrusion_moves": report.stats.extrusion_moves,
            "observed_x": report.stats.observed_x,
            "observed_y": report.stats.observed_y,
            "observed_z": report.stats.observed_z,
        },
        "errors": report.errors.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "warnings": report.warnings.iter().map(ToString::to_string).collect::<Vec<_>>(),
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?
    );
    Ok(if report.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(3)
    })
}
