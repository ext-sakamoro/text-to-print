//! 構造で返す安全性の検査 (`pipeline::safety_report`) の試験
//!
//! * 文字列 (`safety_check_lol`) が構造化の前と同じ bytes であること: LLM の修正の指示は
//!   この文字列で分類するので、変わると retry loop の振る舞いが変わる 期待値は同梱 template 116 件を
//!   検査した結果 (`fixtures/safety_messages_templates.json`、ALICE の lock を上げた時に作り直し、
//!   template ごとの差は CHANGELOG に書く)
//!   通常の試験は速い数件、全件は `--release -- --ignored` で走らせる
//! * 構造の各欄 (種類 / 重大度 / 確かさ / 値) と、利用者向けの文に検査の元の文字列が出ないこと
//! * sbc_case の柱と壁の間の溝が囲まれた穴として報告されず、測り直すと spec の幅 (1.5mm) が出ること

use std::collections::BTreeMap;

use serde::Deserialize;
use text_to_print_core::pipeline::{safety_check_lol, safety_report};
use text_to_print_core::violation::{Confidence, Kind, Lang, Severity};

#[derive(Deserialize)]
struct Expected {
    lol: String,
    messages: Vec<String>,
}

fn fixture() -> BTreeMap<String, Expected> {
    serde_json::from_str(include_str!("fixtures/safety_messages_templates.json")).expect("fixture")
}

fn check_ids(ids: &[&str]) {
    let all = fixture();
    let mut compared = 0;
    for id in ids {
        let e = all
            .get(*id)
            .unwrap_or_else(|| panic!("{id} が fixture に無い"));
        assert_eq!(safety_check_lol(&e.lol), e.messages, "{id}: {}", e.lol);
        compared += 1;
    }
    assert_eq!(compared, ids.len());
}

/// 速い数件 (通常の試験)
#[test]
fn strings_are_unchanged_for_fast_templates() {
    check_ids(&["coin_100yen", "spool_holder_1kg", "magnet_knife_rail_8"]);
}

/// 同梱 template の全件 (遅い: `cargo test --release -p text-to-print-core --test safety_report -- --ignored`)
#[test]
#[ignore = "slow: 116 templates, run with --release -- --ignored"]
fn strings_are_unchanged_for_every_template() {
    let all = fixture();
    assert_eq!(all.len(), 116, "fixture の件数");
    let ids: Vec<&str> = all.keys().map(String::as_str).collect();
    check_ids(&ids);
}

#[test]
fn parse_error_is_blocking_and_proved() {
    let v = safety_report("box3d(1, 2");
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].kind, Kind::LolParse);
    assert_eq!(v[0].severity, Severity::Blocking);
    assert!(v[0].message.starts_with("LOL parse error:"));
}

#[test]
fn thin_plate_reports_measured_wall_and_proved_erosion() {
    let v = safety_report("box3d(10, 10, 0.3)");
    let kinds: Vec<Kind> = v.iter().map(|x| x.kind).collect();
    assert!(kinds.contains(&Kind::WallTooThinEverywhere), "{kinds:?}");
    let wall = v
        .iter()
        .find(|x| x.kind == Kind::WallThin)
        .expect("wall_thin");
    // oracle: 板厚 2 × 0.3 = 0.6mm
    assert!((wall.measured.unwrap() - 0.6).abs() < 0.05, "{wall:?}");
    // 限界は印刷に使う printer profile から Law が導いた値 (nozzle 0.4 × 周回 3 = 1.2)
    assert_eq!(
        wall.limit,
        Some(text_to_print_core::violation::printer_limits().wall_advisory_mm)
    );
    assert_eq!(wall.severity, Severity::Blocking);
    assert!(wall.count.unwrap() > 0);
    let everywhere = v
        .iter()
        .find(|x| x.kind == Kind::WallTooThinEverywhere)
        .unwrap();
    assert_eq!(everywhere.confidence, Confidence::Proved);
}

#[test]
fn large_flat_plate_is_warp_critical() {
    let v = safety_report("box3d(150, 150, 2)");
    assert_eq!(v[0].kind, Kind::WarpCritical, "{v:?}");
    assert_eq!(v[0].severity, Severity::Blocking);
}

/// sbc_case の柱 (standoff) と壁の間の隙間
///
/// 以前は柱の底の円が床の面だけで接する継ぎ目があり、穴径の検査はその継ぎ目 (0.09mm) を標本に読んで
/// 測り直しが柱と壁の間 (1.5〜1.9mm) を返していた 柱を床の厚みの半分だけ床の中へ下ろした今は継ぎ目が無く、
/// 囲まれた隙間の検査は何も報告しない 柱と壁の間は上が開いた溝で、測り直すと spec の閉形式の幅が出る
///
/// oracle (spec `sbc_case(85, 56, 25)`、既定の壁 2 / 床 3 / clearance 1 / 柱 直径 6 高さ 5 / inset 3.5):
/// 壁の内側 x = (85 + 2·1) / 2 = 43.5、柱の中心 x = 85/2 − 3.5 = 39 (半径 3)、柱と壁の最も狭い所は
/// 43.5 − 42 = 1.5 柱は z = −28/2 + 3 = −11 から 5mm 高い
#[test]
fn sbc_case_standoff_slot_is_not_an_enclosed_hole_and_measures_to_spec() {
    let v = safety_report("sbc_case(85, 56, 25)");
    let holes = v.iter().filter(|x| x.kind == Kind::HoleTooSmall).count();
    assert_eq!(holes, 0, "{v:?}");
    let sdf = alice_bamboo::lol_to_sdf("sbc_case(85, 56, 25)").expect("parse");
    let mut measured = 0;
    for (x, y) in [
        (42.75f32, 24.5f32),
        (42.75, -24.5),
        (-42.75, 24.5),
        (-42.75, -24.5),
    ] {
        let at = glam::Vec3::new(x, y, -8.5);
        assert!(
            alice_sdf::eval(&sdf, at) > 0.0,
            "柱と壁の間の点 {at} が空隙でない"
        );
        let r = text_to_print_core::violation::remeasure(&sdf, at, 2.0, 0.1, true);
        // 柱の脇は y 方向に広がる楔なので、点を含む最大の球は最も狭い所 (1.5) より少し大きい
        assert!((1.45..=1.9).contains(&r), "{at}: remeasured {r}");
        measured += 1;
    }
    assert_eq!(measured, 4, "比較 0 件で通らない");
}

/// 利用者向けの文 (題 / 対処 / 注記) に検査の元の文字列の語が出ない (ja / en)
/// 元の文字列は `message` (「詳細」) にだけある
#[test]
fn shown_text_is_free_of_raw_check_output() {
    let mut checked = 0;
    for lol in [
        "box3d(10, 10, 0.3)",
        "box3d(150, 150, 2)",
        "sbc_case(85, 56, 25)",
        "box3d(1, 2",
    ] {
        for v in safety_report(lol) {
            for lang in [Lang::Ja, Lang::En] {
                let s = v.shown(lang);
                let text = format!("{} {} {}", s.title, s.action, s.notes.join(" "));
                assert!(
                    !text.contains(&v.message),
                    "{lol}: 元の文字列がそのまま出た"
                );
                for raw in [
                    "DfAM",
                    "Printability",
                    "erosion",
                    "march",
                    "ISO",
                    "Warp risk",
                    "parse error",
                    "[",
                    "{",
                ] {
                    assert!(!text.contains(raw), "{lol} {lang:?}: {raw} in {text}");
                }
                checked += 1;
            }
        }
    }
    assert!(checked >= 10, "比較 {checked} 件");
}
