//! 安全性の検査の結果を構造で持つ (種類 / 重大度 / 測定値 / 限界 / 位置 / 件数)
//!
//! 以前は結果を文字列だけで返しており、app はその文字列を利用者にそのまま見せるか、
//! 先頭の語を読んで分類するしかなかった ここでは検査の結果を [`Violation`] に持ち、
//! 利用者向けの文 (ja / en) は 1 つの表 (`data/violation_text.json`) から作る
//! 元の文字列は [`Violation::message`] に残し、LLM の修正の指示と「詳細」の表示に使う
//!
//! 限界と重大度の決め方は `data/printability_policy.json` の 1 つの表にある 検査
//! (DfAM の限界 / 肉厚の証明) もこの表から限界を受け取る ([`Policy::limit`])
//!
//! 標本で測る項目 (穴径 / 突起) は、報告された位置を含む最も大きい内接球を探して測り直す
//! ([`remeasure`]) 標本が入隅で床と壁の両方に近い点を読むと実際より狭く出るので、
//! 測り直した値が限界を満たすなら「測り方による誤差の可能性」として情報に下げる

use std::collections::BTreeMap;
use std::sync::OnceLock;

use alice_sdf::SdfNode;
use glam::Vec3;
use serde::{Deserialize, Serialize};

/// 違反の種類 (`data/violation_text.json` と `data/printability_policy.json` の key)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// 局所的に薄い (三角形ごとの厳密 march)
    WallThin,
    /// 形全体が薄い (erosion の証明)
    WallTooThinEverywhere,
    /// 2 つ以上に分かれる (連結性の証明)
    Disconnected,
    /// 穴やすき間が狭い (DfAM の標本)
    HoleTooSmall,
    /// 突起が細い (DfAM の標本)
    FeatureTooSmall,
    /// 反りの危険度 Critical
    WarpCritical,
    /// 反りの危険度 High
    WarpHigh,
    /// 反りの危険度 Medium
    WarpMedium,
    /// 梁の荷重の安全率不足
    BeamOverloaded,
    /// 冷却時の熱応力
    ThermalStress,
    /// 使用温度がガラス転移温度に近い
    ThermalNearTg,
    /// LOL を読めない
    LolParse,
    /// 判別できなかった
    Other,
}

impl Kind {
    /// 全ての種類 (表の網羅の試験に使う)
    pub const ALL: [Self; 13] = [
        Self::WallThin,
        Self::WallTooThinEverywhere,
        Self::Disconnected,
        Self::HoleTooSmall,
        Self::FeatureTooSmall,
        Self::WarpCritical,
        Self::WarpHigh,
        Self::WarpMedium,
        Self::BeamOverloaded,
        Self::ThermalStress,
        Self::ThermalNearTg,
        Self::LolParse,
        Self::Other,
    ];

    /// 表の key
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::WallThin => "wall_thin",
            Self::WallTooThinEverywhere => "wall_too_thin_everywhere",
            Self::Disconnected => "disconnected",
            Self::HoleTooSmall => "hole_too_small",
            Self::FeatureTooSmall => "feature_too_small",
            Self::WarpCritical => "warp_critical",
            Self::WarpHigh => "warp_high",
            Self::WarpMedium => "warp_medium",
            Self::BeamOverloaded => "beam_overloaded",
            Self::ThermalStress => "thermal_stress",
            Self::ThermalNearTg => "thermal_near_tg",
            Self::LolParse => "lol_parse",
            Self::Other => "other",
        }
    }
}

/// 重大度
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// 印刷の前に直す
    Blocking,
    /// 確認する (限界の近く / 対処できることが多い)
    Advisory,
    /// 参考
    Info,
}

/// 結果の確かさ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// 証明 (erosion / 連結性)
    Proved,
    /// 測定 (標本 / 三角形ごとの march)
    Measured,
    /// 測り直すと限界を満たす (測り方による誤差の可能性)
    PossibleArtefact,
}

/// 利用者に見せる文の言語
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    /// 日本語
    Ja,
    /// 英語
    En,
}

impl Lang {
    const fn key(self) -> &'static str {
        match self {
            Self::Ja => "ja",
            Self::En => "en",
        }
    }

    /// BCP 47 の先頭 (`ja` / `ja-JP` は日本語、それ以外は英語)
    #[must_use]
    pub fn from_tag(tag: &str) -> Self {
        if tag.get(..2).is_some_and(|p| p.eq_ignore_ascii_case("ja")) {
            Self::Ja
        } else {
            Self::En
        }
    }
}

/// 検査の結果 1 件
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Violation {
    /// 種類
    pub kind: Kind,
    /// 重大度 (方針の表と、測り直した値から決める)
    pub severity: Severity,
    /// 確かさ
    pub confidence: Confidence,
    /// 検査が報告した測定値 \[mm\]
    pub measured: Option<f32>,
    /// 位置の周りで測り直した値 \[mm\] (標本の項目で位置がある時)
    pub remeasured: Option<f32>,
    /// 限界 \[mm\]
    pub limit: Option<f32>,
    /// 位置 (形の座標系、mm)
    pub location: Option<[f32; 3]>,
    /// 該当した三角形などの件数
    pub count: Option<u32>,
    /// 意図した形の注記 (`data/template_annotations.json`)
    pub intentional: Option<String>,
    /// 検査の元の文字列 (LLM の修正の指示と「詳細」に使う、利用者向けの文ではない)
    pub message: String,
}

impl Violation {
    /// 利用者に見せる値 (測り直した値があればそれ)
    #[must_use]
    pub fn shown_measured(&self) -> Option<f32> {
        self.remeasured.or(self.measured)
    }

    /// 利用者向けの題 (ja / en)
    #[must_use]
    pub fn title(&self, lang: Lang) -> String {
        self.fill(&text().kinds[self.kind.key()][lang.key()].title)
    }

    /// 利用者向けの対処 (ja / en)
    #[must_use]
    pub fn action(&self, lang: Lang) -> String {
        self.fill(&text().kinds[self.kind.key()][lang.key()].action)
    }

    /// 題と対処に添える注記 (測り方の誤差 / 測り直し / 意図した形)
    #[must_use]
    pub fn notes(&self, lang: Lang) -> Vec<String> {
        let t = text();
        let mut out = Vec::new();
        if self.confidence == Confidence::PossibleArtefact {
            out.push(self.fill(&t.notes["possible_artefact"][lang.key()]));
        } else if self.remeasured.is_some() {
            out.push(fill_values(
                &t.notes["remeasured"][lang.key()],
                self.measured,
                self.limit,
                self.count,
                None,
            ));
        }
        if let Some(note) = &self.intentional {
            out.push(fill_values(
                &t.notes["intentional"][lang.key()],
                None,
                None,
                None,
                Some(note),
            ));
        }
        out
    }

    fn fill(&self, template: &str) -> String {
        fill_values(
            template,
            self.shown_measured(),
            self.limit,
            self.count,
            self.intentional.as_deref(),
        )
    }
}

fn fmt_mm(v: f32) -> String {
    // 2 桁 (0.04 / 1.50 / 2.00): 元の検査の文字列と同じ桁
    format!("{v:.2}")
}

fn fill_values(
    template: &str,
    measured: Option<f32>,
    limit: Option<f32>,
    count: Option<u32>,
    note: Option<&str>,
) -> String {
    let mut s = template.to_string();
    if let Some(m) = measured {
        s = s.replace("{measured}", &fmt_mm(m));
    }
    if let Some(l) = limit {
        s = s.replace("{limit}", &fmt_mm(l));
    }
    if let Some(c) = count {
        s = s.replace("{count}", &c.to_string());
    }
    if let Some(n) = note {
        s = s.replace("{note}", n);
    }
    s
}

// --- 表 (data/*.json) ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct TextPair {
    title: String,
    action: String,
}

#[derive(Debug, Deserialize)]
struct Text {
    kinds: BTreeMap<String, BTreeMap<String, TextPair>>,
    notes: BTreeMap<String, BTreeMap<String, String>>,
    severity: BTreeMap<String, BTreeMap<String, String>>,
    header: BTreeMap<String, String>,
    details: BTreeMap<String, String>,
}

/// 方針の表の 1 項目
#[derive(Debug, Clone, Deserialize)]
pub struct PolicyKind {
    /// 限界 \[mm\] (測る項目だけ)
    pub limit_mm: Option<f32>,
    /// `blocking` / `advisory` / `info` / `by_band`
    pub severity: String,
    /// この製品で選んだ値 (設計の指針の値ではない)
    #[serde(default)]
    pub ours: bool,
    /// 出所
    pub source: String,
}

/// 数値 1 つと出所
#[derive(Debug, Clone, Deserialize)]
pub struct PolicyValue {
    /// 値
    pub value: f32,
    /// この製品で選んだ値
    #[serde(default)]
    pub ours: bool,
    /// 出所
    pub source: String,
}

/// 印刷可能性の判定の方針 (`data/printability_policy.json`)
#[derive(Debug, Clone, Deserialize)]
pub struct Policy {
    /// 限界から何割以内を advisory にするか
    pub band: PolicyValue,
    /// 測り直しで標本の点を含むとみなす余裕 \[mm\]
    pub discriminator_tolerance_mm: PolicyValue,
    /// 誤差扱いに下げる前に確かめる周りの範囲 (limit に掛ける)
    pub discriminator_neighbourhood_factor: PolicyValue,
    /// 種類ごとの限界と重大度
    pub kinds: BTreeMap<String, PolicyKind>,
}

impl Policy {
    /// 種類の項目
    ///
    /// # Panics
    /// 表に種類が無い時 (試験 `every_kind_is_in_both_tables` が先に止める)
    #[must_use]
    pub fn kind(&self, kind: Kind) -> &PolicyKind {
        &self.kinds[kind.key()]
    }

    /// 種類の限界 \[mm\]
    #[must_use]
    pub fn limit(&self, kind: Kind) -> Option<f32> {
        self.kind(kind).limit_mm
    }

    /// 測定値から重大度を決める (`by_band` は限界の `band` 以内を advisory にする)
    #[must_use]
    pub fn severity(&self, kind: Kind, measured: Option<f32>) -> Severity {
        match self.kind(kind).severity.as_str() {
            "blocking" => Severity::Blocking,
            "advisory" => Severity::Advisory,
            "info" => Severity::Info,
            _ => match (measured, self.limit(kind)) {
                (Some(m), Some(l)) if m >= l => Severity::Info,
                (Some(m), Some(l)) if m >= l * (1.0 - self.band.value) => Severity::Advisory,
                _ => Severity::Blocking,
            },
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct Annotation {
    call: String,
    kind: Kind,
    region: [[f32; 3]; 2],
    note: String,
}

#[derive(Debug, Deserialize)]
struct Annotations {
    annotations: Vec<Annotation>,
}

const POLICY_JSON: &str = include_str!("../data/printability_policy.json");
const TEXT_JSON: &str = include_str!("../data/violation_text.json");
const ANNOTATIONS_JSON: &str = include_str!("../data/template_annotations.json");

/// 方針の表
///
/// # Panics
/// 同梱の表が読めない時 (crate に入れる data の誤り、試験が先に止める)
pub fn policy() -> &'static Policy {
    static P: OnceLock<Policy> = OnceLock::new();
    P.get_or_init(|| serde_json::from_str(POLICY_JSON).expect("data/printability_policy.json"))
}

fn text() -> &'static Text {
    static T: OnceLock<Text> = OnceLock::new();
    T.get_or_init(|| serde_json::from_str(TEXT_JSON).expect("data/violation_text.json"))
}

fn annotations() -> &'static [Annotation] {
    static A: OnceLock<Annotations> = OnceLock::new();
    &A.get_or_init(|| {
        serde_json::from_str(ANNOTATIONS_JSON).expect("data/template_annotations.json")
    })
    .annotations
}

/// 結果の一覧の見出し ([`Lang`])
#[must_use]
pub fn header(lang: Lang) -> &'static str {
    &text().header[lang.key()]
}

/// 元の文字列を畳む欄の見出し
#[must_use]
pub fn details_label(lang: Lang) -> &'static str {
    &text().details[lang.key()]
}

/// 重大度の名前
#[must_use]
pub fn severity_label(severity: Severity, lang: Lang) -> &'static str {
    let key = match severity {
        Severity::Blocking => "blocking",
        Severity::Advisory => "advisory",
        Severity::Info => "info",
    };
    &text().severity[key][lang.key()]
}

/// 利用者に見せる形 (app の境界で JSON にする)
#[derive(Debug, Clone, Serialize)]
pub struct Shown {
    /// 種類
    pub kind: Kind,
    /// 重大度
    pub severity: Severity,
    /// 確かさ
    pub confidence: Confidence,
    /// 題
    pub title: String,
    /// 対処
    pub action: String,
    /// 注記
    pub notes: Vec<String>,
    /// 検査の元の文字列 (「詳細」)
    pub raw: String,
}

impl Violation {
    /// 利用者に見せる形にする
    #[must_use]
    pub fn shown(&self, lang: Lang) -> Shown {
        Shown {
            kind: self.kind,
            severity: self.severity,
            confidence: self.confidence,
            title: self.title(lang),
            action: self.action(lang),
            notes: self.notes(lang),
            raw: self.message.clone(),
        }
    }
}

// --- 測り直し -------------------------------------------------------------------------------

/// 標本の位置 `at` を含む最も大きい内接球の直径 (空隙は `gap = true`、材料は `false`)
///
/// 中心 `c` を `at` の周り (半径 `reach`) の格子で探し、`c` での内接球の半径 `r = ±sdf(c)` が
/// `|c - at| ≤ r + tolerance` を満たすもの (球が `at` を余裕つきで含む) の最大を取る
/// 「`at` の近くの最大」でなく「`at` を含む最大」にするのは、隙間の外の広い空間の球を拾わない
/// ため 余裕 `tolerance` は、標本が入隅の点 (どの大きい球にもちょうどは含まれない) を
/// 読んだ時に、その隙間の本当の幅の球に届くためのもの
#[must_use]
pub fn remeasure(sdf: &SdfNode, at: Vec3, reach: f32, tolerance: f32, gap: bool) -> f32 {
    const GRID: i32 = 13;
    const REFINE: usize = 6;
    let sign = if gap { 1.0 } else { -1.0 };
    // 中心 c の内接球の半径 (条件を満たさない c は 0)
    let score = |c: Vec3| -> f32 {
        let r = sign * alice_sdf::eval(sdf, c);
        if r > 0.0 && c.distance(at) <= r + tolerance {
            r
        } else {
            0.0
        }
    };
    let half = (GRID - 1) as f32 / 2.0;
    let mut best = (0.0_f32, at);
    for ix in 0..GRID {
        for iy in 0..GRID {
            for iz in 0..GRID {
                let o = Vec3::new(ix as f32 - half, iy as f32 - half, iz as f32 - half) / half;
                if o.length_squared() > 1.0 {
                    continue;
                }
                let c = at + o * reach;
                let r = score(c);
                if r > best.0 {
                    best = (r, c);
                }
            }
        }
    }
    // 格子の幅で値が段になるので、最良点の周りを半分ずつ細かくして探す (3^3 近傍)
    let mut step = reach / half;
    for _ in 0..REFINE {
        step *= 0.5;
        let centre = best.1;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let c = centre + Vec3::new(dx as f32, dy as f32, dz as f32) * step;
                    if c.distance(at) > reach {
                        continue;
                    }
                    let r = score(c);
                    if r > best.0 {
                        best = (r, c);
                    }
                }
            }
        }
    }
    2.0 * best.0
}

/// 標本の周りの同じ側の点が全て `limit` 以上に測り直されるか (誤差扱いに下げてよいか)
///
/// 半径 `radius` の球の中の格子 (各軸 `GRID` 点) のうち、標本から `tolerance` より離れていて、
/// 同じ側 (`gap` なら空隙 `sdf > 0`、突起なら材料 `sdf < 0`) にある点を全て測り直す
/// 1 点でも `limit` 未満なら false 調べる点が 2 点未満でも false (判断の材料が無い)
///
/// 標本 1 点だけで下げると、厚い部分との境の近く (tolerance 以内) の標本が厚い部分の幅を
/// 読み、本物の薄い部分を下げてしまう 周りの全点を求めるので、入隅の近くの点が狭く読まれる
/// 場合も下げない (下げ損ねは安全側: 警告が残るだけ)
#[must_use]
pub fn neighbourhood_is_wide(
    sdf: &SdfNode,
    at: Vec3,
    radius: f32,
    tolerance: f32,
    limit: f32,
    gap: bool,
) -> bool {
    const GRID: i32 = 5;
    let sign = if gap { 1.0 } else { -1.0 };
    let half = (GRID - 1) as f32 / 2.0;
    let mut checked = 0;
    for ix in 0..GRID {
        for iy in 0..GRID {
            for iz in 0..GRID {
                let o = Vec3::new(ix as f32 - half, iy as f32 - half, iz as f32 - half) / half;
                if o.length_squared() > 1.0 {
                    continue;
                }
                let q = at + o * radius;
                if q.distance(at) <= tolerance || sign * alice_sdf::eval(sdf, q) <= 0.0 {
                    continue;
                }
                if remeasure(sdf, q, limit, tolerance, gap) < limit {
                    return false;
                }
                checked += 1;
            }
        }
    }
    checked >= 2
}

/// 測る項目 (穴径 / 突起) の違反を作る 位置があれば測り直して重大度と確かさを決める
#[must_use]
pub fn measured_violation(
    sdf: &SdfNode,
    kind: Kind,
    measured: Option<f32>,
    location: Option<Vec3>,
    count: Option<u32>,
    message: String,
) -> Violation {
    let p = policy();
    let limit = p.limit(kind);
    let gap = match kind {
        Kind::HoleTooSmall => Some(true),
        Kind::FeatureTooSmall => Some(false),
        _ => None,
    };
    let remeasured = match (gap, location, limit) {
        (Some(gap), Some(at), Some(l)) => Some(remeasure(
            sdf,
            at,
            l,
            p.discriminator_tolerance_mm.value,
            gap,
        )),
        _ => None,
    };
    // 誤差扱いに下げるのは、標本の点と、その周りの同じ側の点が全て limit 以上の時だけ
    let wide_around = |l: f32| match (gap, location) {
        (Some(gap), Some(at)) => neighbourhood_is_wide(
            sdf,
            at,
            l * p.discriminator_neighbourhood_factor.value,
            p.discriminator_tolerance_mm.value,
            l,
            gap,
        ),
        _ => false,
    };
    let (severity, confidence) = match (remeasured, limit) {
        (Some(r), Some(l)) if r >= l && wide_around(l) => {
            (Severity::Info, Confidence::PossibleArtefact)
        }
        // 標本の点は広いが周りに狭い所がある: 標本の値でなく元の測定で判断する
        (Some(r), Some(l)) if r >= l => (p.severity(kind, measured), Confidence::Measured),
        (Some(r), _) => (p.severity(kind, Some(r)), Confidence::Measured),
        (None, _) => (p.severity(kind, measured), Confidence::Measured),
    };
    Violation {
        kind,
        severity,
        confidence,
        measured,
        remeasured,
        limit,
        location: location.map(|v| v.to_array()),
        count,
        intentional: None,
        message,
    }
}

/// 証明や区分で決まる違反 (測り直さない)
#[must_use]
pub fn fixed_violation(
    kind: Kind,
    confidence: Confidence,
    measured: Option<f32>,
    count: Option<u32>,
    message: String,
) -> Violation {
    let p = policy();
    Violation {
        kind,
        severity: p.severity(kind, measured),
        confidence,
        measured,
        remeasured: None,
        limit: p.limit(kind),
        location: None,
        count,
        intentional: None,
        message,
    }
}

/// 同梱 template の意図的な形の注記を付ける (LOL の最上位の関数名と、違反の位置で照合)
///
/// 位置の無い違反には付けない (どの部分の違反か分からないので、意図した部分とは言えない)
pub fn annotate(lol_source: &str, violations: &mut [Violation]) {
    let call = lol_source
        .trim_start()
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .next()
        .unwrap_or("");
    for v in violations.iter_mut() {
        let Some(at) = v.location else { continue };
        for a in annotations() {
            let inside = (0..3).all(|i| a.region[0][i] <= at[i] && at[i] <= a.region[1][i]);
            if a.call == call && a.kind == v.kind && inside {
                v.intentional = Some(a.note.clone());
                v.severity = Severity::Info;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_is_in_both_tables_with_ja_and_en() {
        let mut checked = 0;
        for kind in Kind::ALL {
            let pk = policy()
                .kinds
                .get(kind.key())
                .unwrap_or_else(|| panic!("{} が方針の表に無い", kind.key()));
            assert!(!pk.source.trim().is_empty(), "{} の出所が空", kind.key());
            assert!(
                ["blocking", "advisory", "info", "by_band"].contains(&pk.severity.as_str()),
                "{}: {}",
                kind.key(),
                pk.severity
            );
            if pk.severity == "by_band" {
                assert!(
                    pk.limit_mm.is_some(),
                    "{} は by_band なのに limit が無い",
                    kind.key()
                );
            }
            let t = text()
                .kinds
                .get(kind.key())
                .unwrap_or_else(|| panic!("{} の文が無い", kind.key()));
            for lang in [Lang::Ja, Lang::En] {
                let pair = &t[lang.key()];
                assert!(!pair.title.trim().is_empty() && !pair.action.trim().is_empty());
                checked += 1;
            }
        }
        assert_eq!(checked, Kind::ALL.len() * 2, "比較 0 件で通らない");
        // 表に余分な種類が無い (表だけ増えて code が知らない種類を作らない)
        assert_eq!(policy().kinds.len(), Kind::ALL.len());
        assert_eq!(text().kinds.len(), Kind::ALL.len());
        for key in ["possible_artefact", "remeasured", "intentional"] {
            for lang in ["ja", "en"] {
                assert!(!text().notes[key][lang].trim().is_empty());
            }
        }
    }

    /// 題の値の差し込み口と、種類が持つ値が合う (差し込まれずに `{…}` が残らない)
    #[test]
    fn titles_fill_every_placeholder() {
        for kind in Kind::ALL {
            let measured = policy().limit(kind).map(|l| l * 0.5);
            let v = Violation {
                kind,
                severity: Severity::Blocking,
                confidence: Confidence::Measured,
                measured,
                remeasured: None,
                limit: policy().limit(kind),
                location: None,
                count: Some(3),
                intentional: None,
                message: String::new(),
            };
            for lang in [Lang::Ja, Lang::En] {
                let s = format!("{} {}", v.title(lang), v.action(lang));
                assert!(!s.contains('{'), "{}: {s}", kind.key());
            }
        }
    }

    #[test]
    fn band_splits_blocking_advisory_info() {
        let p = policy();
        // limit 1.6、band 10% ⇒ 1.44 未満は blocking、1.44 以上 1.6 未満は advisory
        assert_eq!(p.severity(Kind::WallThin, Some(0.04)), Severity::Blocking);
        assert_eq!(p.severity(Kind::WallThin, Some(1.43)), Severity::Blocking);
        assert_eq!(p.severity(Kind::WallThin, Some(1.45)), Severity::Advisory);
        assert_eq!(p.severity(Kind::WallThin, Some(1.6)), Severity::Info);
        assert_eq!(p.severity(Kind::WallThin, None), Severity::Blocking);
        assert_eq!(p.severity(Kind::WarpHigh, None), Severity::Advisory);
        assert_eq!(p.severity(Kind::WarpCritical, None), Severity::Blocking);
        assert_eq!(p.severity(Kind::ThermalNearTg, None), Severity::Info);
        assert!(
            p.band.ours && p.discriminator_tolerance_mm.ours,
            "製品で選んだ値は ours を付ける"
        );
    }

    /// 方針の表の限界が ALICE-Bamboo の FDM の値と同じ (違えば表の側を見直す)
    #[test]
    fn policy_limits_match_the_bamboo_fdm_row() {
        let l =
            alice_bamboo::dfam::DfamConfig::for_process(alice_bamboo::dfam::Process::Fdm).limits;
        let p = policy();
        assert_eq!(p.limit(Kind::WallThin), Some(l.min_unsupported_wall_mm));
        assert_eq!(
            p.limit(Kind::WallTooThinEverywhere),
            Some(l.min_unsupported_wall_mm)
        );
        assert_eq!(p.limit(Kind::HoleTooSmall), Some(l.min_hole_diameter_mm));
        assert_eq!(
            p.limit(Kind::FeatureTooSmall),
            Some(l.min_positive_feature_mm)
        );
    }

    fn sdf(lol: &str) -> SdfNode {
        alice_bamboo::lol_to_sdf(lol).expect("parse")
    }

    /// oracle: 壁から 1.5mm 離れた角柱の隙間 (箱 25 の内側 23、角柱は x 17.5..21.5)
    /// 標本は床の 0.4mm 上の入隅で 0.62mm と読んだ 測り直しは 1.5
    #[test]
    fn remeasure_recovers_the_real_gap_next_to_a_floor() {
        let s = sdf(
            "union(subtract(box3d(25, 25, 8), translate(0, 0, 5, box3d(23, 23, 8))), translate(19.5, 19.5, -0.5, box3d(2, 2, 2.5)))",
        );
        let r = remeasure(&s, Vec3::new(21.8, 18.6, -2.6), 2.0, 0.1, true);
        assert!((1.45..=1.55).contains(&r), "gap {r}");
        let v = measured_violation(
            &s,
            Kind::HoleTooSmall,
            Some(0.62),
            Some(Vec3::new(21.8, 18.6, -2.6)),
            None,
            String::new(),
        );
        // 1.5 は限界 2.0 の 10% より下 ⇒ blocking のまま、値は測り直した方を見せる
        assert_eq!(v.confidence, Confidence::Measured);
        assert_eq!(v.severity, Severity::Blocking);
        assert!((v.shown_measured().unwrap() - 1.5).abs() < 0.05);
    }

    /// oracle: 幅 3mm の隙間の入隅 (床と壁の両方に近い点) で標本が 0.6 と読んだ場合
    /// 標本の点も周りの同じ側の点も全て 3.0 に測り直されるので、誤差扱いに下げる
    #[test]
    fn wide_gap_sampled_at_a_corner_is_demoted() {
        // 壁 (内側 23) から 3mm 離れた角柱 (x 16..20)
        let s = sdf(
            "union(subtract(box3d(25, 25, 8), translate(0, 0, 5, box3d(23, 23, 8))), translate(18, 18, -0.5, box3d(2, 2, 2.5)))",
        );
        let v = measured_violation(
            &s,
            Kind::HoleTooSmall,
            Some(0.6),
            Some(Vec3::new(22.6, 18.0, -2.6)),
            None,
            String::new(),
        );
        // oracle: 柱 (x ≤ 20) と壁 (x = 23) の間 3.0mm
        assert!((v.remeasured.unwrap() - 3.0).abs() < 0.05, "{v:?}");
        assert_eq!(v.confidence, Confidence::PossibleArtefact);
        assert_eq!(v.severity, Severity::Info);
    }

    /// 5c の指摘: 厚い土台 (厚さ 4) の上の薄い板 (幅 0.2) で、標本が土台の上面から tolerance の
    /// 0.5 / 1 / 2 倍の高さにある時 標本 1 点の測り直しは土台の厚さを読みうるが、周りの点 (板の中) が
    /// 狭いので下げない
    #[test]
    fn thin_fin_on_a_thick_base_is_never_demoted() {
        // 土台 z ∈ [-4, 0]、板 x ∈ [-0.1, 0.1]、z ∈ [0, 6]
        let s = sdf("union(box3d(10, 10, 2), translate(0, 0, 5, box3d(0.1, 10, 3)))");
        let s = SdfNode::Translate {
            child: std::sync::Arc::new(s),
            offset: Vec3::new(0.0, 0.0, -2.0),
        };
        let tol = policy().discriminator_tolerance_mm.value;
        let mut checked = 0;
        for k in [0.5_f32, 1.0, 2.0] {
            let at = Vec3::new(0.0, 0.0, k * tol);
            let v = measured_violation(
                &s,
                Kind::FeatureTooSmall,
                Some(0.2),
                Some(at),
                None,
                String::new(),
            );
            assert_ne!(
                v.confidence,
                Confidence::PossibleArtefact,
                "offset {k}×tol: {v:?}"
            );
            assert_eq!(v.severity, Severity::Blocking, "offset {k}×tol");
            checked += 1;
        }
        assert_eq!(checked, 3);
    }

    /// 凸の角の薄いひれ (幅 0.2): 標本の点の測り直し自体が 0.2 (< limit) なので、周りの判定には
    /// 届かない この試験が固定するのは remeasure の角での符号の扱い (材料側の内接球を取る) だけ
    #[test]
    fn thin_fin_at_a_convex_corner_is_never_demoted() {
        let s = sdf("union(box3d(5, 5, 5), translate(5.5, 5.5, 0, box3d(0.5, 0.1, 5)))");
        let at = Vec3::new(5.05, 5.5, 0.0);
        let v = measured_violation(
            &s,
            Kind::FeatureTooSmall,
            Some(0.2),
            Some(at),
            None,
            String::new(),
        );
        assert_ne!(v.confidence, Confidence::PossibleArtefact, "{v:?}");
    }

    /// 周りの判定そのものが下げを止める場合: 厚い箱 (半幅 5) の面 x = 5 に、凸の稜線 (y = 5) から 1mm の
    /// 所で幅 0.2 の板が付いている 標本を面から 0.05 の所 (余裕の幅の中) に置くと、標本の点の測り直しは
    /// 箱の中の球 (稜線までの 1mm で止まる、直径 2.0) を読んで limit 0.8 以上になる 下げないのは周りの点
    /// (板の中、0.2) を見る判定のおかげ
    #[test]
    fn thin_fin_beside_a_convex_edge_is_kept_by_the_neighbourhood_gate() {
        let s = sdf("union(box3d(5, 5, 5), translate(6, 4, 0, box3d(1, 0.1, 5)))");
        let at = Vec3::new(5.05, 4.0, 0.0);
        let limit = policy().limit(Kind::FeatureTooSmall).unwrap();
        let tol = policy().discriminator_tolerance_mm.value;
        let raw = remeasure(&s, at, limit, tol, false);
        assert!(
            raw >= limit,
            "標本の点の測り直しが limit 未満だと周りの判定に届かない: {raw}"
        );
        let v = measured_violation(
            &s,
            Kind::FeatureTooSmall,
            Some(0.2),
            Some(at),
            None,
            String::new(),
        );
        assert_eq!(v.confidence, Confidence::Measured, "{v:?}");
        assert_ne!(v.severity, Severity::Info);
    }

    /// 周りの同じ側の点が 2 点未満なら下げない (判断の材料が無い)
    /// 材料の中の標本の +x 側 (距離 1.0 = 半径) にだけ大きい空洞 (半径 3、表面 x = 0.95) がある
    /// 同じ側 (空隙) の格子点は (1, 0, 0) の 1 点だけで、その点の測り直しは空洞の直径 (6) を読む
    #[test]
    fn neighbourhood_with_fewer_than_two_points_does_not_demote() {
        let one = sdf("subtract(box3d(20, 20, 20), translate(3.95, 0, 0, sphere(3)))");
        let at = Vec3::ZERO;
        assert!(
            remeasure(&one, Vec3::new(1.0, 0.0, 0.0), 2.0, 0.1, true) >= 2.0,
            "その 1 点は広い"
        );
        assert!(
            !neighbourhood_is_wide(&one, at, 1.0, 0.1, 2.0, true),
            "1 点で下げた"
        );
        // 同じ側の点が 0 点 (空隙の無い箱の中)
        let none = sdf("box3d(20, 20, 20)");
        assert!(
            !neighbourhood_is_wide(&none, at, 1.0, 0.1, 2.0, true),
            "0 点で下げた"
        );
        // 対照: 空洞を近づけて同じ側の点を増やすと下げる (判定が常に false ではない)
        let many = sdf("subtract(box3d(20, 20, 20), translate(2.5, 0, 0, sphere(3)))");
        assert!(neighbourhood_is_wide(
            &many,
            Vec3::new(0.5, 0.0, 0.0),
            1.0,
            0.1,
            2.0,
            true
        ));
    }

    /// 平らな壁の脇の幅 3mm の隙間 (床から離れた高さ) の標本は、周りも全て 3mm なので下げる
    #[test]
    fn wide_gap_away_from_corners_is_demoted() {
        let s = sdf(
            "union(subtract(box3d(25, 25, 8), translate(0, 0, 5, box3d(23, 23, 8))), translate(18, 18, -0.5, box3d(2, 2, 6)))",
        );
        let v = measured_violation(
            &s,
            Kind::HoleTooSmall,
            Some(0.6),
            Some(Vec3::new(22.95, 18.0, 2.0)),
            None,
            String::new(),
        );
        assert_eq!(v.confidence, Confidence::PossibleArtefact, "{v:?}");
        assert_eq!(v.severity, Severity::Info);
    }

    /// oracle: 材料の内接球 = 板厚 (0.6mm の板の中央) 突起の測り直しは 0.6 前後
    #[test]
    fn remeasure_material_is_the_plate_thickness() {
        let s = sdf("box3d(10, 10, 0.3)");
        let r = remeasure(&s, Vec3::new(0.0, 0.0, 0.0), 0.8, 0.1, false);
        assert!((0.55..=0.61).contains(&r), "plate {r}");
    }

    #[test]
    fn annotation_needs_call_kind_and_location_inside() {
        // 同梱の注記は空 (利用者との表の確認の前) なので、照合だけを空振りしない形で確かめる
        let mut v = vec![fixed_violation(
            Kind::WallThin,
            Confidence::Measured,
            Some(0.1),
            None,
            String::new(),
        )];
        annotate("tape_dispenser(76, 50, 3)", &mut v);
        assert!(v[0].intentional.is_none(), "位置の無い違反に注記は付かない");
    }

    #[test]
    fn lang_from_tag() {
        assert_eq!(Lang::from_tag("ja-JP"), Lang::Ja);
        assert_eq!(Lang::from_tag("JA"), Lang::Ja);
        assert_eq!(Lang::from_tag("en"), Lang::En);
        assert_eq!(Lang::from_tag(""), Lang::En);
    }
}
