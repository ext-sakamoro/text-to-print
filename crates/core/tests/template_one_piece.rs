//! 同梱 template 116 件が 1 つの塊で印刷されること
//!
//! 各 template の SDF を格子で標本し、内側 (距離 < 0) の点を 6 近傍で繋いだ塊を数える 別の塊に分かれた
//! template は、印刷すると部品が浮く・別々に落ちる (filament_spool_holder の peg が base の 2mm 上に
//! 浮いていた形) 例外は [`MULTI_PIECE`] に理由つきで書く
//!
//! 格子の限界 (この試験が見落とす形):
//! * 刻み (既定 1mm、格子が 4,000 万点を超える大きい template は 1.25 倍ずつ粗くする) より狭い隙間で離れた
//!   2 つの塊は、隙間の両側の点が隣り合うので 1 つに数えうる (別の塊を見落とす向き)
//! * 刻みより細い首で繋がった 1 つの塊は、首に点が載らないと 2 つに数えうる (誤って落とす向き、落ちたら
//!   その template を測り直して確かめる)
//!
//! 全件は時間がかかるので `cargo test --release -p text-to-print-core --test template_one_piece -- --ignored`

use std::collections::BTreeMap;

use serde::Deserialize;

/// 2 つ以上の塊で正しい template と理由 (無い時は空)
const MULTI_PIECE: &[(&str, &str)] = &[];

/// 格子の点の数の上限 (これを超える template は刻みを粗くする)
const MAX_CELLS: f32 = 40.0e6;

#[derive(Deserialize)]
struct Entry {
    lol: String,
}

fn templates() -> BTreeMap<String, Entry> {
    serde_json::from_str(include_str!("fixtures/safety_messages_templates.json")).expect("fixture")
}

/// SDF の内側の塊の数と、使った格子の刻み \[mm\]
fn pieces(lol: &str) -> (usize, f32) {
    let sdf = alice_bamboo::lol_to_sdf(lol).expect("template parses");
    let a = alice_sdf::tight_aabb::compute_tight_aabb_with_config(
        &sdf,
        &alice_sdf::tight_aabb::TightAabbConfig::preset_large(),
    );
    let size = a.max - a.min;
    let cells = |h: f32| (size.x / h).ceil() * (size.y / h).ceil() * (size.z / h).ceil();
    let mut h = 1.0f32;
    while cells(h) > MAX_CELLS {
        h *= 1.25;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 正の有限の寸法 / 刻み
    let n = [
        (size.x / h).ceil() as usize + 1,
        (size.y / h).ceil() as usize + 1,
        (size.z / h).ceil() as usize + 1,
    ];
    let idx = |i: usize, j: usize, k: usize| (k * n[1] + j) * n[0] + i;
    let mut inside = vec![false; n[0] * n[1] * n[2]];
    for k in 0..n[2] {
        for j in 0..n[1] {
            for i in 0..n[0] {
                #[allow(clippy::cast_precision_loss)] // 格子の番号は数千以下
                let p = a.min + glam::Vec3::new(i as f32, j as f32, k as f32) * h;
                inside[idx(i, j, k)] = alice_sdf::eval(&sdf, p) < 0.0;
            }
        }
    }
    let mut seen = vec![false; inside.len()];
    let mut count = 0;
    let mut stack = Vec::new();
    for start in 0..inside.len() {
        if !inside[start] || seen[start] {
            continue;
        }
        count += 1;
        seen[start] = true;
        stack.push(start);
        while let Some(c) = stack.pop() {
            let (i, j, k) = (c % n[0], (c / n[0]) % n[1], c / (n[0] * n[1]));
            let plane = n[0] * n[1];
            let neighbours = [
                (i > 0).then(|| c - 1),
                (i + 1 < n[0]).then(|| c + 1),
                (j > 0).then(|| c - n[0]),
                (j + 1 < n[1]).then(|| c + n[0]),
                (k > 0).then(|| c - plane),
                (k + 1 < n[2]).then(|| c + plane),
            ];
            for d in neighbours.into_iter().flatten() {
                if inside[d] && !seen[d] {
                    seen[d] = true;
                    stack.push(d);
                }
            }
        }
    }
    (count, h)
}

/// 数え方の歯: 離れた 2 つの箱は 2、重ねた 2 つの箱は 1、何も無い形は 0
#[test]
fn piece_count_separates_and_joins() {
    assert_eq!(
        pieces("union(box3d(5, 5, 5), translate(20, 0, 0, box3d(5, 5, 5)))").0,
        2
    );
    assert_eq!(
        pieces("union(box3d(5, 5, 5), translate(8, 0, 0, box3d(5, 5, 5)))").0,
        1
    );
    // 刻みより広い 3mm の隙間で離れた 2 つ (格子の限界の内側で見分けられること)
    assert_eq!(
        pieces("union(box3d(5, 5, 5), translate(13, 0, 0, box3d(5, 5, 5)))").0,
        2
    );
}

#[test]
#[ignore = "slow: 116 templates, run with --release -- --ignored"]
fn every_template_is_one_piece() {
    let all = templates();
    let mut checked = 0;
    let mut wrong = Vec::new();
    for (id, e) in &all {
        let (count, h) = pieces(&e.lol);
        let allowed = MULTI_PIECE.iter().any(|(m, _)| m == id);
        if (count == 1) == allowed {
            wrong.push(format!(
                "{id}: {count} piece(s) at step {h:.2} mm ({})",
                e.lol
            ));
        }
        checked += 1;
    }
    assert_eq!(checked, all.len());
    assert!(
        checked >= 116,
        "比較 {checked} 件 (同梱 template は 116 件)"
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
