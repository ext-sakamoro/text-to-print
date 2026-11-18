//! Golden tests of every time form text-to-print-core stores, signs or writes into a file.
//!
//! The forms are fixed by text, not by the time library: licence payloads are signed as the bytes
//! they were issued as and parsed again on every start, manifests sit inside 3MF files already
//! printed, and the 3MF `CreationDate` is read by slicers. Changing the time library must leave
//! every string here byte for byte as it is. The tests go through the public serde and file paths
//! only, so they hold for any time type behind them.
//!
//! oracle: RFC 3339 (section 5.6) and the instants written out by hand below; no value here is
//! computed by the code under test.

use std::io::{Cursor, Read as _, Write as _};

use text_to_print_core::license::LicensePayload;
use text_to_print_core::manifest::{AliceManifest, ManifestBuilder, embed_in_3mf};
use text_to_print_core::tier::Tier;

fn payload_json(issued_at: &str, expires_at: &str) -> String {
    format!(
        r#"{{"tier":"Pro","user_id":"u1","issued_at":"{issued_at}","expires_at":"{expires_at}"}}"#
    )
}

/// What a stored or received licence time reads as, written back out: (input, output).
/// The output is the RFC 3339 text in UTC with `Z`, the fraction in groups of 3 digits, as short
/// as the value allows (none, 3, 6 or 9 digits).
const READ_THEN_WRITTEN: &[(&str, &str)] = &[
    ("2026-11-18T09:30:00Z", "2026-11-18T09:30:00Z"),
    ("2026-11-18T09:30:00.123Z", "2026-11-18T09:30:00.123Z"),
    ("2026-11-18T09:30:00.120Z", "2026-11-18T09:30:00.120Z"),
    ("2026-11-18T09:30:00.1Z", "2026-11-18T09:30:00.100Z"),
    ("2026-11-18T09:30:00.000Z", "2026-11-18T09:30:00Z"),
    ("2026-11-18T09:30:00.123456Z", "2026-11-18T09:30:00.123456Z"),
    (
        "2026-11-18T09:30:00.123456789Z",
        "2026-11-18T09:30:00.123456789Z",
    ),
    ("2026-11-18T09:30:00.1234Z", "2026-11-18T09:30:00.123400Z"),
    // the `+00:00` that `to_rfc3339` writes (crash reports, Stripe rows) reads as the same instant
    ("2026-11-18T09:30:00+00:00", "2026-11-18T09:30:00Z"),
    // another offset is converted to UTC
    ("2026-11-18T18:30:00+09:00", "2026-11-18T09:30:00Z"),
    ("2026-11-18T04:00:00-05:30", "2026-11-18T09:30:00Z"),
    ("2026-11-18T00:30:00+09:00", "2026-11-17T15:30:00Z"),
    // before 1970, with a fraction
    ("1969-12-31T23:59:59.5Z", "1969-12-31T23:59:59.500Z"),
    // a leap day, the first and last representable four-digit years
    ("2024-02-29T00:00:00Z", "2024-02-29T00:00:00Z"),
    ("0001-01-01T00:00:00Z", "0001-01-01T00:00:00Z"),
    (
        "9999-12-31T23:59:59.999999999Z",
        "9999-12-31T23:59:59.999999999Z",
    ),
];

/// Texts a licence time must not read as (the key is then refused, never read as another instant).
const REFUSED: &[&str] = &[
    "",
    "2026-11-18",
    "2026-11-18T09:30:00",
    "2026-02-29T00:00:00Z",
    "2026-11-31T00:00:00Z",
    "2026-13-01T00:00:00Z",
    "2026-11-18T24:00:00Z",
    "2026-11-18T09:60:00Z",
    "2026-11-18T09:30:00+24:00",
    "2026-11-18T09:30:00.Z",
    "1763458200",
];

#[test]
fn a_licence_time_reads_and_writes_as_recorded() {
    for (input, written) in READ_THEN_WRITTEN {
        let p: LicensePayload = serde_json::from_str(&payload_json(input, input))
            .unwrap_or_else(|e| panic!("{input}: refused ({e})"));
        assert_eq!(
            serde_json::to_string(&p).unwrap(),
            payload_json(written, written),
            "{input}"
        );
    }
}

#[test]
fn a_malformed_licence_time_is_refused() {
    for input in REFUSED {
        assert!(
            serde_json::from_str::<LicensePayload>(&payload_json(input, "2026-11-18T09:30:00Z"))
                .is_err(),
            "{input:?} was read"
        );
    }
}

fn manifest_at(timestamp: &str) -> AliceManifest {
    let m = ManifestBuilder {
        prompt: "a cube 20mm",
        prompt_lang: "en",
        llm_model: "m",
        llm_seed: Some(42),
        lol_source: "cube(20.0)",
        mesh_bytes: b"mesh",
        vertex_count: 8,
        triangle_count: 12,
        export_format: "3mf",
        retry_count: 0,
        time_to_file_ms: 1,
        safety_violations: vec![],
        success: true,
        tier: Tier::Pro,
    }
    .build();
    let mut v = serde_json::to_value(&m).unwrap();
    v["timestamp"] = serde_json::Value::String(timestamp.to_owned());
    serde_json::from_value(v).unwrap()
}

/// The manifest time inside the 3MF (`Metadata/alice_manifest.json`) and the slicer-visible
/// `CreationDate` (RFC 3339 with `+00:00`, the fraction as short as the value allows).
#[test]
fn the_3mf_carries_the_recorded_time_forms() {
    for (timestamp, in_json, creation_date) in [
        (
            "2026-11-18T09:30:00Z",
            "2026-11-18T09:30:00Z",
            "2026-11-18T09:30:00+00:00",
        ),
        (
            "2026-11-18T09:30:00.250Z",
            "2026-11-18T09:30:00.250Z",
            "2026-11-18T09:30:00.250+00:00",
        ),
        (
            "2026-11-18T09:30:00.123456789+09:00",
            "2026-11-18T00:30:00.123456789Z",
            "2026-11-18T00:30:00.123456789+00:00",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.3mf");
        {
            let mut zw = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zw.start_file("[Content_Types].xml", opts).unwrap();
            zw.write_all(b"<Types/>").unwrap();
            zw.start_file("3D/3dmodel.model", opts).unwrap();
            zw.write_all(b"<?xml version=\"1.0\"?>\n<model unit=\"millimeter\" xmlns=\"http://x\">\n</model>")
                .unwrap();
            zw.finish().unwrap();
        }
        embed_in_3mf(&path, &manifest_at(timestamp)).unwrap();
        let mut ar = zip::ZipArchive::new(Cursor::new(std::fs::read(&path).unwrap())).unwrap();
        let mut json = String::new();
        ar.by_name("Metadata/alice_manifest.json")
            .unwrap()
            .read_to_string(&mut json)
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["timestamp"], in_json, "{timestamp}");
        let mut xml = String::new();
        ar.by_name("3D/3dmodel.model")
            .unwrap()
            .read_to_string(&mut xml)
            .unwrap();
        let tag = format!("<metadata name=\"CreationDate\">{creation_date}</metadata>");
        assert!(xml.contains(&tag), "{timestamp}: want {tag} in {xml}");
    }
}
