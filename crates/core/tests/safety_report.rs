//! 構造で返す安全性の検査 (`pipeline::safety_report`) の試験
//!
//! * 文字列 (`safety_check_lol`) が構造化の前と同じ bytes であること: LLM の修正の指示は
//!   この文字列で分類するので、変わると retry loop の振る舞いが変わる 期待値は構造化の前の
//!   core (main a39e48c) で同梱 template 116 件を検査した結果 (`fixtures/safety_messages_templates.json`)
//!   通常の試験は速い数件、全件は `--release -- --ignored` で走らせる
//! * 構造の各欄 (種類 / 重大度 / 確かさ / 値) と、利用者向けの文に検査の元の文字列が出ないこと
//! * raspi_case の穴径の標本 (0.09mm) を測り直すと本当の隙間 (柱と壁の間 1.5mm) が出ること

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
    assert_eq!(wall.limit, Some(1.6));
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

/// raspi_case の穴径の標本は床の 0.4mm 上の入隅 (柱の脇) を読んで 0.09mm と報告する
/// その位置 (柱の中心から z に 1.5mm ずれた所) の柱と壁の間は、spec から 1.74mm
/// (壁の内側 43.5 と、柱 (中心 (39, 24.5)、半径 3) の表面の距離 √(4.5² + 1.5²) − 3) で、
/// 柱の真横の最も狭い所が 1.5mm 測り直しは限界 2.0 を下回る (誤差扱いにはしない)
#[test]
fn raspi_gap_is_remeasured_to_the_real_slot() {
    let v = safety_report("raspi_case(85, 56, 25)");
    let hole = v
        .iter()
        .find(|x| x.kind == Kind::HoleTooSmall)
        .expect("hole");
    assert!(hole.measured.unwrap() < 0.2, "{hole:?}");
    let r = hole.remeasured.expect("位置があるので測り直す");
    // 柱の脇は z 方向に広がる楔なので、標本の点を含む最大の球は最も狭い所 (1.5) より少し大きい
    // (標本の点の位置で 1.5〜1.9 の間を動く) どちらでも限界 2.0 未満 = 誤差扱いにはしない
    assert!((1.45..=1.9).contains(&r), "remeasured {r}");
    assert_ne!(hole.severity, Severity::Info);
    assert_eq!(hole.confidence, Confidence::Measured);
}

/// 利用者向けの文 (題 / 対処 / 注記) に検査の元の文字列の語が出ない (ja / en)
/// 元の文字列は `message` (「詳細」) にだけある
#[test]
fn shown_text_is_free_of_raw_check_output() {
    let mut checked = 0;
    for lol in [
        "box3d(10, 10, 0.3)",
        "box3d(150, 150, 2)",
        "raspi_case(85, 56, 25)",
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
