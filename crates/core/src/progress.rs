//! 形を作る処理の進み具合と中止
//!
//! 処理は段 ([`Phase`]) の列で、各段の中では実際に済んだ作業量 (`done / total`) を報告する
//! 作業量を数えられない処理 (meshing の 1 回の呼び出しの中と、修復 (Repair) の段の全体) は、段に入った
//! ことだけを報告し、進んだふりはしない 修復には段の中の区切りが無く、重みの 65% を占める (実測) ので、
//! 割合はその間 修復の段の始まりで止まる 全体の割合は段ごとの重み (実測した所要時間の比) から作る ([`Weighted`])
//!
//! [`Progress::on_cells`] は meshing の側に作業量の報告を足す時と同じ形にしてある
//! (meshing の中で `on_cells(済んだ slab, 全 slab)` を呼べば、この段がそのまま細かくなる)
//!
//! どの報告でも [`ControlFlow::Break`] を返すと処理は中止され、[`Cancelled`] の誤りで終わる
//! 中止は報告を受けた時点で効く (報告の無い不透明な処理の途中では効かず、その処理の終わりで効く)
//! 中止の待ちの上限は、その時に走っている不透明な処理 (1 回の meshing、または修復の全体) の長さで、
//! 修復が最も長くなりやすい

use std::fmt;
use std::ops::ControlFlow;

/// 処理の段 (この順に進む)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Phase {
    /// LOL を読む
    Parse,
    /// 形を包む箱を求める
    Bounds,
    /// 固体のある領域を区間評価で探す (作業量 = 区間評価の回数 / 上限)
    SolidSearch,
    /// mesh を作る (作業量 = 済んだ部分 / 部分の数、1 回の meshing の中は数えられない)
    Mesh,
    /// mesh を修復する
    Repair,
}

impl Phase {
    /// 全ての段 (順)
    pub const ALL: [Self; 5] = [
        Self::Parse,
        Self::Bounds,
        Self::SolidSearch,
        Self::Mesh,
        Self::Repair,
    ];

    /// 表示や記録に使う名前
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Parse => "parse",
            Self::Bounds => "bounds",
            Self::SolidSearch => "solid_search",
            Self::Mesh => "mesh",
            Self::Repair => "repair",
        }
    }

    const fn index(self) -> usize {
        self as usize
    }
}

/// 進み具合の受け手 meshing の作業量の報告 (`on_cells`) と同じ形
pub trait Progress {
    /// 段に入った
    fn on_phase(&mut self, phase: Phase) -> ControlFlow<()>;
    /// 今の段で済んだ作業量 (`done <= total`、`total > 0`)
    fn on_cells(&mut self, done: u64, total: u64) -> ControlFlow<()>;
}

/// 何もしない受け手 (進み具合を使わない呼び出し)
#[derive(Debug, Default, Clone, Copy)]
pub struct NoProgress;

impl Progress for NoProgress {
    fn on_phase(&mut self, _phase: Phase) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }
    fn on_cells(&mut self, _done: u64, _total: u64) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }
}

/// 中止された
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

impl fmt::Display for Cancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("cancelled")
    }
}

impl std::error::Error for Cancelled {}

/// `ControlFlow::Break` を [`Cancelled`] の誤りにする
///
/// # Errors
/// 受け手が中止を返した時
pub fn check(flow: ControlFlow<()>) -> anyhow::Result<()> {
    match flow {
        ControlFlow::Continue(()) => Ok(()),
        ControlFlow::Break(()) => Err(Cancelled.into()),
    }
}

/// 利用者に見せる 1 回の報告
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Report {
    /// 今の段
    pub phase: Phase,
    /// 全体の割合 (0..=1、減らない)
    pub overall: f32,
    /// 今の段の中の作業量が数えられているか (false = 段の中は不透明、割合は段の始まりで止まる)
    pub determinate: bool,
    /// 処理が終わった (overall = 1 の報告は 1 回だけ)
    pub finished: bool,
}

/// 段ごとの重み (全体に占める所要時間の比、合計 1)
///
/// 同梱 template 116 件を Preview の品質で作った時の、各段が全体に占める割合の中央値
/// (release、arm64 の host で実測 2026-10-12: Parse 0.000 / Bounds 0.001 / SolidSearch 0.179 /
/// Mesh 0.168 / Repair 0.652、0 の段は 0.001 にして合計 1 に揃えた) 重みは割合の見せ方だけに
/// 効き、処理には影響しない
pub const PREVIEW_WEIGHTS: [f32; 5] = [0.001, 0.001, 0.179, 0.168, 0.651];

/// 段と作業量から全体の割合を作り、減らない列にして受け手に渡す
///
/// 受け手が中止を返したら以降は何も渡さない 終わり ([`Weighted::finish`]) は 1 回だけ渡す
pub struct Weighted<F: FnMut(Report) -> ControlFlow<()>> {
    weights: [f32; 5],
    sink: F,
    phase: Phase,
    last: f32,
    stopped: bool,
}

impl<F: FnMut(Report) -> ControlFlow<()>> Weighted<F> {
    /// 重みと受け手
    pub fn new(weights: [f32; 5], sink: F) -> Self {
        Self {
            weights,
            sink,
            phase: Phase::Parse,
            last: 0.0,
            stopped: false,
        }
    }

    fn start_of(&self, phase: Phase) -> f32 {
        // 空の和は -0.0 になるので 0.0 から足す
        self.weights[..phase.index()].iter().fold(0.0, |a, w| a + w)
    }

    fn emit(&mut self, overall: f32, determinate: bool, finished: bool) -> ControlFlow<()> {
        if self.stopped {
            return ControlFlow::Break(());
        }
        let overall = overall.clamp(self.last, 1.0);
        self.last = overall;
        let flow = (self.sink)(Report {
            phase: self.phase,
            overall,
            determinate,
            finished,
        });
        if flow.is_break() {
            self.stopped = true;
        }
        flow
    }

    /// 処理が最後まで済んだ (全体 1 を 1 回だけ渡す)
    pub fn finish(&mut self) {
        if !self.stopped {
            let _ = self.emit(1.0, true, true);
            self.stopped = true;
        }
    }
}

impl<F: FnMut(Report) -> ControlFlow<()>> Progress for Weighted<F> {
    fn on_phase(&mut self, phase: Phase) -> ControlFlow<()> {
        // 段は戻らない (同じ段に入り直す時は割合を戻さない)
        if phase > self.phase {
            self.phase = phase;
        }
        let at = self.start_of(self.phase);
        self.emit(at, false, false)
    }

    fn on_cells(&mut self, done: u64, total: u64) -> ControlFlow<()> {
        if total == 0 {
            return self.emit(self.last, false, false);
        }
        #[allow(clippy::cast_precision_loss)]
        let frac = (done.min(total) as f32) / (total as f32);
        let at = self.start_of(self.phase) + self.weights[self.phase.index()] * frac;
        self.emit(at, true, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_sum_to_one() {
        let s: f32 = PREVIEW_WEIGHTS.iter().sum();
        assert!((s - 1.0).abs() < 1e-6, "{s}");
        assert!(PREVIEW_WEIGHTS.iter().all(|w| *w > 0.0));
    }

    #[test]
    fn overall_never_decreases_and_finishes_once() {
        let mut seen = Vec::new();
        let mut w = Weighted::new(PREVIEW_WEIGHTS, |r| {
            seen.push(r);
            ControlFlow::Continue(())
        });
        let _ = w.on_phase(Phase::SolidSearch);
        let _ = w.on_cells(10, 100);
        let _ = w.on_cells(5, 100); // 戻る報告でも割合は減らない
        let _ = w.on_phase(Phase::Bounds); // 段は戻らない
        let _ = w.on_phase(Phase::Mesh);
        let _ = w.on_cells(1, 2);
        w.finish();
        w.finish();
        let overall: Vec<f32> = seen.iter().map(|r| r.overall).collect();
        assert!(overall.windows(2).all(|p| p[1] >= p[0]), "{overall:?}");
        assert_eq!(seen.iter().filter(|r| r.finished).count(), 1);
        assert_eq!(seen.iter().filter(|r| r.overall >= 1.0).count(), 1);
        assert_eq!(seen.last().unwrap().overall, 1.0);
        // 段に入った報告は不透明 (段の中の作業量はまだ無い)
        assert!(!seen[0].determinate);
    }

    #[test]
    fn nothing_after_cancel() {
        let mut calls = 0;
        {
            let mut w = Weighted::new(PREVIEW_WEIGHTS, |_r| {
                calls += 1;
                if calls == 2 {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            });
            assert!(w.on_phase(Phase::Bounds).is_continue());
            assert!(w.on_phase(Phase::SolidSearch).is_break());
            assert!(w.on_cells(1, 2).is_break());
            w.finish();
        }
        assert_eq!(calls, 2, "中止の後は受け手を呼ばない");
    }

    #[test]
    fn check_maps_break_to_cancelled() {
        assert!(check(ControlFlow::Continue(())).is_ok());
        let e = check(ControlFlow::Break(())).unwrap_err();
        assert!(e.downcast_ref::<Cancelled>().is_some());
    }
}
