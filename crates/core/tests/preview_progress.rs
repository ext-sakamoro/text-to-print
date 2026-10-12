//! 形を作る処理の進み具合と中止 (`pipeline::preview_lol_to_mesh_with_progress`)
//!
//! * 報告の有無で mesh が変わらない (bytes で比べる)
//! * 全体の割合は減らず、終わり (1.0) は 1 回だけ、段は Parse → … → Repair の順
//! * 中止はどの段で返しても `Cancelled` で終わり、mesh を返さず、以後は報告しない

use std::ops::ControlFlow;

use text_to_print_core::pipeline::{
    Quality, preview_lol_to_mesh, preview_lol_to_mesh_with_progress,
};
use text_to_print_core::progress::{Cancelled, PREVIEW_WEIGHTS, Phase, Progress, Report, Weighted};

/// 形の bytes (頂点の位置と index)
fn bytes(m: &alice_sdf::mesh::Mesh) -> Vec<u8> {
    let mut out = Vec::new();
    for v in &m.vertices {
        for c in v.position.to_array() {
            out.extend(c.to_bits().to_le_bytes());
        }
    }
    for i in &m.indices {
        out.extend(i.to_le_bytes());
    }
    out
}

/// 1 部品 / 回転して箱が膨らむ (再メッシュ) / 離れた部品 (成分ごと) の 3 つの経路
const SHAPES: [&str; 3] = [
    "box3d(5, 5, 2.5)",
    "rotate(30, 0, 0, box3d(20, 20, 0.8))",
    "union(sphere(3), translate(40, 0, 0, sphere(3)))",
];

#[test]
fn mesh_is_identical_with_and_without_progress() {
    let mut compared = 0;
    for lol in SHAPES {
        let plain = preview_lol_to_mesh(lol, Quality::Preview).expect(lol);
        let mut reports = Vec::new();
        let with = {
            let mut w = Weighted::new(PREVIEW_WEIGHTS, |r| {
                reports.push(r);
                ControlFlow::Continue(())
            });
            let m = preview_lol_to_mesh_with_progress(lol, Quality::Preview, &mut w).expect(lol);
            w.finish();
            m
        };
        assert_eq!(bytes(&plain), bytes(&with), "{lol}");
        assert!(!reports.is_empty(), "{lol}: 報告が 0 件");
        compared += 1;
    }
    assert_eq!(compared, SHAPES.len());
}

fn run(lol: &str) -> Vec<Report> {
    let mut reports = Vec::new();
    {
        let mut w = Weighted::new(PREVIEW_WEIGHTS, |r| {
            reports.push(r);
            ControlFlow::Continue(())
        });
        preview_lol_to_mesh_with_progress(lol, Quality::Preview, &mut w).expect(lol);
        w.finish();
    }
    reports
}

/// 実際の 3 つの形で、割合が減らない / 段が順 / 終わりが 1 回であることを見る
/// この 3 つの形の報告は元々減らない順に来るので、`Weighted` の「減らないように抑える」処理
/// (clamp) は通らない clamp そのものは `progress` の単体試験 (戻る報告を合成して与える) が固定する
#[test]
fn overall_is_monotonic_phases_are_ordered_and_finish_is_once() {
    for lol in SHAPES {
        let r = run(lol);
        let overall: Vec<f32> = r.iter().map(|x| x.overall).collect();
        assert!(
            overall.windows(2).all(|p| p[1] >= p[0]),
            "{lol}: {overall:?}"
        );
        let phases: Vec<Phase> = r.iter().map(|x| x.phase).collect();
        assert!(phases.windows(2).all(|p| p[1] >= p[0]), "{lol}: {phases:?}");
        assert_eq!(phases.first(), Some(&Phase::Parse));
        assert!(
            r[0].overall.is_sign_positive() && r[0].overall == 0.0,
            "{lol}: 最初は +0"
        );
        assert!(
            phases.contains(&Phase::Mesh) && phases.contains(&Phase::Repair),
            "{lol}: {phases:?}"
        );
        assert_eq!(r.iter().filter(|x| x.finished).count(), 1, "{lol}");
        assert_eq!(r.iter().filter(|x| x.overall >= 1.0).count(), 1, "{lol}");
    }
}

/// 作業量の少ない形でも、最後は 1.0 が 1 回だけ届く (割合が途中で止まったままにならない)
#[test]
fn tiny_shape_reaches_one_exactly_once() {
    let r = run("sphere(0.5)");
    assert_eq!(r.last().map(|x| (x.overall, x.finished)), Some((1.0, true)));
    assert_eq!(r.iter().filter(|x| x.finished).count(), 1);
}

/// 段に入った時に中止を返す受け手
struct CancelAt {
    phase: Phase,
    calls_after: usize,
    cancelled: bool,
}

impl Progress for CancelAt {
    fn on_phase(&mut self, phase: Phase) -> ControlFlow<()> {
        if self.cancelled {
            self.calls_after += 1;
        }
        if phase == self.phase {
            self.cancelled = true;
            return ControlFlow::Break(());
        }
        ControlFlow::Continue(())
    }
    fn on_cells(&mut self, _done: u64, _total: u64) -> ControlFlow<()> {
        if self.cancelled {
            self.calls_after += 1;
            return ControlFlow::Break(());
        }
        ControlFlow::Continue(())
    }
}

#[test]
fn cancel_in_every_phase_returns_cancelled_and_stops_reporting() {
    let mut checked = 0;
    for phase in Phase::ALL {
        let mut p = CancelAt {
            phase,
            calls_after: 0,
            cancelled: false,
        };
        let r = preview_lol_to_mesh_with_progress(SHAPES[1], Quality::Preview, &mut p);
        let Err(e) = r else {
            panic!("{phase:?}: 中止したのに mesh が返った");
        };
        assert!(e.downcast_ref::<Cancelled>().is_some(), "{phase:?}: {e}");
        assert_eq!(p.calls_after, 0, "{phase:?}: 中止の後に報告した");
        checked += 1;
    }
    assert_eq!(checked, Phase::ALL.len());
}

/// 固体の探索の途中 (作業量の報告) で中止しても止まる
#[test]
fn cancel_during_solid_search_counts() {
    struct CancelOnCells(usize);
    impl Progress for CancelOnCells {
        fn on_phase(&mut self, _p: Phase) -> ControlFlow<()> {
            ControlFlow::Continue(())
        }
        fn on_cells(&mut self, _d: u64, _t: u64) -> ControlFlow<()> {
            self.0 += 1;
            ControlFlow::Break(())
        }
    }
    let mut p = CancelOnCells(0);
    let Err(e) = preview_lol_to_mesh_with_progress(SHAPES[1], Quality::Preview, &mut p) else {
        panic!("中止したのに mesh が返った");
    };
    assert!(e.downcast_ref::<Cancelled>().is_some());
    assert_eq!(p.0, 1, "中止を返した後に作業量を報告した");
}
