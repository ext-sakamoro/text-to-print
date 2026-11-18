//! Golden tests of the time forms the worker signs and stores.
//!
//! The licence payload is signed as its serde JSON bytes and read by released clients, and the
//! D1 rows hold times as text: changing the time library must leave every string here byte for
//! byte as it is. The tests go through serde and the conversion functions the handlers use, with
//! unix seconds and strings in and strings out, so they hold for any time type behind them.
//!
//! oracle: RFC 3339 (section 5.6) and the instants written out by hand below (1_763_458_200 is
//! 2025-11-18T09:30:00Z); no value here is computed by the code under test.

use super::{licence_expiry, period_end_at, row_time};
use crate::license_issue::LicensePayload;

fn payload_json(issued_at: &str, expires_at: &str) -> String {
    format!(
        r#"{{"tier":"Pro","user_id":"u1","issued_at":"{issued_at}","expires_at":"{expires_at}"}}"#
    )
}

#[test]
fn a_licence_time_reads_and_writes_as_recorded() {
    for (input, written) in [
        ("2026-11-18T09:30:00Z", "2026-11-18T09:30:00Z"),
        ("2026-11-18T09:30:00.123Z", "2026-11-18T09:30:00.123Z"),
        ("2026-11-18T09:30:00.1Z", "2026-11-18T09:30:00.100Z"),
        ("2026-11-18T09:30:00.123456Z", "2026-11-18T09:30:00.123456Z"),
        (
            "2026-11-18T09:30:00.123456789Z",
            "2026-11-18T09:30:00.123456789Z",
        ),
        ("2026-11-18T09:30:00+00:00", "2026-11-18T09:30:00Z"),
        ("2026-11-18T18:30:00+09:00", "2026-11-18T09:30:00Z"),
        ("1969-12-31T23:59:59.5Z", "1969-12-31T23:59:59.500Z"),
    ] {
        let p: LicensePayload = serde_json::from_str(&payload_json(input, input))
            .unwrap_or_else(|e| panic!("{input}: refused ({e})"));
        assert_eq!(
            serde_json::to_string(&p).unwrap(),
            payload_json(written, written),
            "{input}"
        );
    }
    for input in [
        "",
        "2026-11-18T09:30:00",
        "2026-02-29T00:00:00Z",
        "1763458200",
    ] {
        assert!(
            serde_json::from_str::<LicensePayload>(&payload_json(input, "2026-11-18T09:30:00Z"))
                .is_err(),
            "{input:?} was read"
        );
    }
}

#[test]
fn a_stripe_period_end_is_stored_and_licensed_as_recorded() {
    for (unix, row, expiry_json) in [
        (
            1_763_458_200,
            "2025-11-18T09:30:00+00:00",
            "\"2025-11-21T09:30:00Z\"",
        ),
        (0, "1970-01-01T00:00:00+00:00", "\"1970-01-04T00:00:00Z\""),
        // a period ending on the last day of February in a leap year
        (
            1_709_164_800,
            "2024-02-29T00:00:00+00:00",
            "\"2024-03-03T00:00:00Z\"",
        ),
        (
            -86_400,
            "1969-12-31T00:00:00+00:00",
            "\"1970-01-03T00:00:00Z\"",
        ),
    ] {
        let end = period_end_at(unix).unwrap_or_else(|| panic!("{unix}: out of range"));
        assert_eq!(row_time(&end), row, "{unix}");
        assert_eq!(
            serde_json::to_string(&licence_expiry(end)).unwrap(),
            expiry_json,
            "{unix}"
        );
        assert_eq!(row_time(&licence_expiry(end)).len(), row.len(), "{unix}");
    }
    // far outside any calendar: refused, not wrapped
    assert!(period_end_at(i64::MAX).is_none());
    assert!(period_end_at(i64::MIN).is_none());
}
