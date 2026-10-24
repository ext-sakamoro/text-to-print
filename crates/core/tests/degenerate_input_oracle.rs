//! Degenerate-input oracle for the public API of `text-to-print-core`.
//!
//! Every function here takes a value that a caller does not control: an expiry in days, a license
//! string pasted by a user, a clock reading, a response from an LLM, a shape that an LLM wrote. None
//! of them may take the process down, and a function that returns `Result` must report a value it
//! cannot represent as an `Err`, not as a panic.
//!
//! What this file pins:
//!
//! * `LicenseIssuer::issue(valid_days)`: `chrono::Duration::days` and `DateTime + Duration` panic for
//!   an unrepresentable span (`i64::MAX` days, or just 100 million days) — it returns `Err` instead.
//! * `Tier::effective_state_with_policy`: `expires_at + grace` overflowed for an extreme instant or a
//!   negative grace; the deadline now saturates, checked against an independent `i128` reference.
//! * License parsing (`from_base64`, `LicenseVerifier::new`, `verify`) and the text helpers of the
//!   pipeline (`extract_lol`, `balance_parens`, `validate_lol`) on hostile strings.
//! * `export_mesh` / `preview_lol_to_mesh` / `safety_check_lol` on degenerate shapes (a zero or
//!   negative scale, a 1e30 size, an empty subtraction): the result may be an error or an empty
//!   mesh, never a panic.

use chrono::{DateTime, Duration, TimeZone, Utc};
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use text_to_print_core::license::{LicenseIssuer, LicenseKey, LicenseVerifier};
use text_to_print_core::pipeline::{self, ExportFormat, Quality};
use text_to_print_core::tier::{RollbackPolicy, Tier, TierState};

/// Distinct panic messages -> (count, first offending case)
#[derive(Default)]
struct Panics(BTreeMap<String, (usize, String)>);

impl Panics {
    fn run(&mut self, label: impl FnOnce() -> String, f: impl FnOnce()) {
        if let Err(payload) = catch_unwind(AssertUnwindSafe(f)) {
            let msg = payload
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<non-string panic>".to_string());
            let entry = self
                .0
                .entry(msg.chars().take(100).collect())
                .or_insert((0, label()));
            entry.0 += 1;
        }
    }

    fn assert_none(&self, what: &str) {
        let report: Vec<String> = self
            .0
            .iter()
            .map(|(msg, (n, first))| format!("  x{n:<4} {msg}\n         first: {first}"))
            .collect();
        assert!(
            self.0.is_empty(),
            "{what}: {} distinct panic(s)\n{}",
            self.0.len(),
            report.join("\n")
        );
    }
}

// ───────────────────────── license ─────────────────────────

#[test]
fn issuing_a_license_with_an_unrepresentable_validity_is_an_error_not_a_panic() {
    let issuer = LicenseIssuer::generate();
    let mut p = Panics::default();
    // `chrono` represents roughly +-262 143 years: 96 million days is the end of the range
    let unrepresentable = [
        i64::MAX,
        i64::MIN,
        i64::MAX / 86_400,
        i64::MIN / 86_400,
        1_000_000_000_000,
        i64::from(i32::MAX),
        100_000_000,
    ];
    for &days in &unrepresentable {
        let mut result = None;
        p.run(
            || format!("issue(valid_days = {days})"),
            || result = Some(issuer.issue(Tier::Pro, "user", days)),
        );
        if let Some(r) = result {
            assert!(
                r.is_err(),
                "valid_days = {days} cannot be represented but was accepted"
            );
        }
    }
    p.assert_none("LicenseIssuer::issue");
}

#[test]
fn ordinary_validity_spans_still_issue_and_verify() {
    let issuer = LicenseIssuer::generate();
    let verifier = LicenseVerifier::new(&issuer.public_key_bytes()).expect("public key");
    for days in [1_i64, 30, 365, 36_500] {
        let key = issuer
            .issue(Tier::Pro, "user", days)
            .unwrap_or_else(|e| panic!("{days} days: {e}"));
        verifier
            .verify(&key)
            .unwrap_or_else(|e| panic!("{days} days must verify: {e}"));
        let left = key.payload.expires_at - key.payload.issued_at;
        assert!(
            (left - Duration::days(days)).num_seconds().abs() <= 1,
            "{days} days: span {left}"
        );
    }
    // zero and negative spans are legal (an already-expired key, used by tests and revocation)
    for days in [0_i64, -1, -365] {
        let key = issuer
            .issue(Tier::Pro, "user", days)
            .unwrap_or_else(|e| panic!("{days} days: {e}"));
        assert!(
            verifier.verify(&key).is_err(),
            "{days} days is expired and must not verify"
        );
    }
}

#[test]
fn license_parsing_and_verification_never_panic_on_hostile_input() {
    let issuer = LicenseIssuer::generate();
    let ok = issuer.issue(Tier::Pro, "user", 30).expect("issue");
    let good = ok.to_base64().expect("base64");
    let mut p = Panics::default();
    let hostile: Vec<(&str, String)> = vec![
        ("empty", String::new()),
        ("garbage", "!!!!".into()),
        ("padding only", "====".into()),
        ("non-ascii", "日本語🙂".into()),
        ("truncated", good[..good.len() / 2].to_string()),
        ("valid base64 / not json", "AAAA".into()),
        ("huge", "A".repeat(5_000_000)),
        ("whitespace", "  \n\t ".into()),
    ];
    for (label, s) in &hostile {
        p.run(
            || format!("from_base64 {label}"),
            || {
                assert!(
                    LicenseKey::from_base64(s).is_err(),
                    "{label} must not parse"
                );
            },
        );
    }
    for key in [[0_u8; 32], [0xff; 32], [1_u8; 32]] {
        p.run(
            || format!("LicenseVerifier::new({key:?})"),
            || {
                let _ = LicenseVerifier::new(&key);
            },
        );
    }
    for n in [0_usize, 1, 31, 32, 33, 1000] {
        p.run(
            || format!("from_did_public_key(len {n})"),
            || {
                let _ = LicenseVerifier::from_did_public_key(&vec![7_u8; n]);
            },
        );
    }
    let verifier = LicenseVerifier::new(&issuer.public_key_bytes()).expect("public key");
    for sig_len in [0_usize, 1, 63, 65, 1000] {
        let mut bad = issuer.issue(Tier::Pro, "user", 30).expect("issue");
        bad.signature = vec![0_u8; sig_len];
        p.run(
            || format!("verify with a {sig_len}-byte signature"),
            || {
                assert!(verifier.verify(&bad).is_err());
            },
        );
    }
    p.assert_none("license parsing / verification");
}

// ───────────────────────── tier ─────────────────────────

/// Independent reference: the grace deadline in milliseconds, saturated to chrono's range
fn reference_state(
    tier: Tier,
    expires: DateTime<Utc>,
    now: DateTime<Utc>,
    policy: RollbackPolicy,
) -> (&'static str, Option<i128>) {
    if now <= expires {
        return ("Active", None);
    }
    match policy {
        RollbackPolicy::Immediate => ("RolledBack", None),
        RollbackPolicy::NoRollback => {
            let _ = tier;
            ("Active", None)
        }
        RollbackPolicy::GracePeriod(d) => {
            let lo = i128::from(DateTime::<Utc>::MIN_UTC.timestamp_millis());
            let hi = i128::from(DateTime::<Utc>::MAX_UTC.timestamp_millis());
            let deadline = (i128::from(expires.timestamp_millis())
                + i128::from(d.num_milliseconds()))
            .clamp(lo, hi);
            if i128::from(now.timestamp_millis()) <= deadline || now <= expires {
                ("Grace", Some(deadline))
            } else {
                ("RolledBack", None)
            }
        }
    }
}

#[test]
fn the_tier_state_is_total_over_extreme_instants_and_grace_periods() {
    let instants: Vec<DateTime<Utc>> = vec![
        DateTime::<Utc>::MIN_UTC,
        Utc.timestamp_opt(-1, 0).unwrap(),
        Utc.timestamp_opt(0, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 10, 2, 0, 0, 0).unwrap(),
        DateTime::<Utc>::MAX_UTC,
    ];
    let graces = [
        Duration::zero(),
        Duration::days(-1),
        Duration::days(14),
        Duration::days(365 * 100_000),
        Duration::MAX,
        Duration::MIN,
        Duration::seconds(i64::MAX / 1000),
    ];
    let mut p = Panics::default();
    for &expires in &instants {
        for &now in &instants {
            for &grace in &graces {
                let mut got = None;
                p.run(
                    || format!("expires {expires} now {now} grace {grace}"),
                    || {
                        got = Some(Tier::effective_state_with_policy(
                            Tier::Pro,
                            expires,
                            now,
                            RollbackPolicy::GracePeriod(grace),
                        ));
                        let _ = Tier::effective_state_with_policy(
                            Tier::Pro,
                            expires,
                            now,
                            RollbackPolicy::Immediate,
                        );
                        let _ = Tier::effective_state_with_policy(
                            Tier::Pro,
                            expires,
                            now,
                            RollbackPolicy::NoRollback,
                        );
                    },
                );
                let Some(state) = got else { continue };
                let (kind, deadline) =
                    reference_state(Tier::Pro, expires, now, RollbackPolicy::GracePeriod(grace));
                match (&state, kind) {
                    (TierState::Active(_), "Active") | (TierState::RolledBack, "RolledBack") => {}
                    (TierState::Grace { until, .. }, "Grace") => {
                        let want = deadline.expect("grace has a deadline");
                        let got_ms = i128::from(until.timestamp_millis());
                        assert!(
                            (got_ms - want).abs() <= 1,
                            "expires {expires} now {now} grace {grace}: until {until} ({got_ms} ms), want {want} ms"
                        );
                    }
                    _ => panic!(
                        "expires {expires} now {now} grace {grace}: got {state:?}, want {kind}"
                    ),
                }
            }
        }
    }
    p.assert_none("Tier::effective_state_with_policy");
}

// ───────────────────────── pipeline: text ─────────────────────────

#[test]
fn the_text_helpers_never_panic_on_hostile_strings() {
    let strings: Vec<(&str, String)> = vec![
        ("empty", String::new()),
        ("nul", "\0".into()),
        ("lone fence", "```".into()),
        ("unclosed fence", "```lol\nsphere(1".into()),
        ("deep open parens", "(".repeat(200_000)),
        ("deep close parens", ")".repeat(200_000)),
        ("unicode", "日本語🙂```lol\n".into()),
        ("json open", "{\"code\": ".into()),
        ("json wrap", "{\"code\": \"sphere(1)\"}".into()),
        (
            "1 MB inside a fence",
            format!("```lol\n{}\n```", "a".repeat(1_000_000)),
        ),
        ("nested fences", "```lol\n```lol\n```".into()),
    ];
    let mut p = Panics::default();
    for (label, s) in &strings {
        p.run(
            || format!("extract_lol {label}"),
            || {
                let _ = pipeline::extract_lol(s);
            },
        );
        p.run(
            || format!("balance_parens {label}"),
            || {
                let _ = pipeline::balance_parens(s);
            },
        );
        p.run(
            || format!("validate_lol {label}"),
            || {
                let _ = pipeline::validate_lol(s);
            },
        );
    }
    p.assert_none("pipeline text helpers");
}

// ───────────────────────── pipeline: degenerate shapes ─────────────────────────

#[test]
fn degenerate_shapes_are_an_error_or_an_empty_mesh_never_a_panic() {
    let dir = tempfile::tempdir().expect("tempdir");
    let shapes = [
        "sphere(0.0)",
        "sphere(-1.0)",
        "sphere(1e30)",
        "sphere(1e-30)",
        "box3d(0.0, 0.0, 0.0)",
        "box3d(1e30, 1.0, 1.0)",
        "box3d(-5.0, 1.0, 1.0)",
        "cylinder(0.0, 0.0)",
        "translate(1e30, 0.0, 0.0, sphere(1.0))",
        "scale(0.0, sphere(1.0))",
        "scale(-1.0, sphere(1.0))",
        "scale(1e30, sphere(1.0))",
        "subtract(sphere(1.0), sphere(1.0))",
        "intersect(sphere(1.0), translate(10.0, 0.0, 0.0, sphere(1.0)))",
        "rotate(1e30, 0.0, 0.0, box3d(1.0, 1.0, 1.0))",
        "twist(1e30, box3d(1.0, 1.0, 1.0))",
    ];
    let mut p = Panics::default();
    for lol in shapes {
        let out = dir.path().to_path_buf();
        p.run(
            || format!("export_mesh 3mf {lol}"),
            || {
                let _ = pipeline::export_mesh(lol, &out, ExportFormat::ThreeMf, Quality::Preview);
            },
        );
        let out = dir.path().to_path_buf();
        p.run(
            || format!("export_mesh stl {lol}"),
            || {
                let _ = pipeline::export_mesh(lol, &out, ExportFormat::Stl, Quality::Preview);
            },
        );
        p.run(
            || format!("preview_lol_to_mesh {lol}"),
            || {
                let _ = pipeline::preview_lol_to_mesh(lol, Quality::Preview);
            },
        );
        p.run(
            || format!("safety_check_lol {lol}"),
            || {
                let _ = pipeline::safety_check_lol(lol);
            },
        );
    }
    p.assert_none("degenerate shapes through the pipeline");
}
