//! The 3MF archives the export writes, against a recorded oracle
//!
//! Oracle: `tests/fixtures/zip_oracle/zip2.json`, recorded from the export of the three programs in
//! the same directory with the code at the time (zip 2.x): for every archive entry its name, its place
//! in the order, its compression method, its modification time, its uncompressed size and the SHA-256
//! of its content. A change of the zip library or of the writer must keep all of these; the compressed
//! bytes of the archive are not compared (another deflate implementation may encode the same content
//! differently).
//!
//! The export names the object with a fresh UUID v7 on every run, so the same program does not give
//! the same bytes twice. That name is masked before hashing, and the mask is held to exactly that: one
//! distinct UUID per archive, in the five places the oracle records (the model's Title, the object's
//! `name` twice and `source_file` in `Metadata/model_settings.config`, the plate's object `name`), and
//! nowhere else. A UUID anywhere else, a second distinct one, or one fewer place fails.

use std::io::Read;
use std::path::Path;

use text_to_print_core::pipeline::{ExportFormat, Quality, export_mesh};

const UUID_PLACEHOLDER: &[u8; 36] = b"00000000-0000-7000-8000-000000000000";
const MASKED_PLACES: usize = 5;

/// UUID v7 in its lower-case hyphenated form: 8-4-4-4-12 hex, version nibble 7, variant 8 / 9 / a / b
fn is_uuid_v7(w: &[u8]) -> bool {
    w.len() == 36
        && w.iter().enumerate().all(|(i, &c)| match i {
            8 | 13 | 18 | 23 => c == b'-',
            14 => c == b'7',
            19 => matches!(c, b'8' | b'9' | b'a' | b'b'),
            _ => c.is_ascii_digit() || (b'a'..=b'f').contains(&c),
        })
}

/// the content with every UUID v7 replaced by the placeholder, the number replaced, and the values seen
fn mask(data: &[u8]) -> (Vec<u8>, usize, Vec<Vec<u8>>) {
    let mut out = data.to_vec();
    let (mut n, mut seen) = (0, Vec::new());
    let mut i = 0;
    while i + 36 <= out.len() {
        if is_uuid_v7(&out[i..i + 36]) {
            seen.push(out[i..i + 36].to_vec());
            out[i..i + 36].copy_from_slice(UUID_PLACEHOLDER);
            n += 1;
            i += 36;
        } else {
            i += 1;
        }
    }
    (out, n, seen)
}

fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn fixtures() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/zip_oracle")
}

#[test]
fn the_mask_takes_exactly_a_uuid_v7() {
    let (_, n, _) = mask(b"x 01a14560-e463-78d1-800b-82069f00aaae y");
    assert_eq!(n, 1);
    for not_v7 in [
        b"01a14560-e463-48d1-800b-82069f00aaae".as_slice(), // version 4
        b"01a14560-e463-78d1-c00b-82069f00aaae",            // variant c
        b"01A14560-E463-78D1-800B-82069F00AAAE",            // upper case
        b"01a14560e46378d1800b82069f00aaae",                // no hyphens
    ] {
        assert_eq!(mask(not_v7).1, 0, "{}", String::from_utf8_lossy(not_v7));
    }
}

#[test]
fn every_archive_entry_matches_the_recorded_oracle() {
    let oracle: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(fixtures().join("zip2.json")).unwrap())
            .unwrap();
    let programs = oracle.as_object().expect("oracle is an object of programs");
    assert_eq!(programs.len(), 3, "the oracle holds the three programs");
    let mut compared = 0;
    for (program, entries) in programs {
        let lol = std::fs::read_to_string(fixtures().join(format!("{program}.lol"))).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let stats = export_mesh(&lol, dir.path(), ExportFormat::ThreeMf, Quality::Preview)
            .unwrap_or_else(|e| panic!("{program}: export: {e:#}"));
        let bytes = std::fs::read(&stats.path).unwrap();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let entries = entries.as_array().unwrap();
        assert_eq!(archive.len(), entries.len(), "{program}: number of entries");
        let (mut places, mut values) = (0, Vec::new());
        for (i, want) in entries.iter().enumerate() {
            let mut file = archive.by_index(i).unwrap();
            let name = file.name().to_string();
            assert_eq!(
                name,
                want["name"].as_str().unwrap(),
                "{program}: entry {i} name / order"
            );
            let method = match file.compression() {
                zip::CompressionMethod::Stored => 0,
                zip::CompressionMethod::Deflated => 8,
                other => panic!("{program} {name}: compression {other:?}"),
            };
            assert_eq!(
                method,
                want["method"].as_u64().unwrap(),
                "{program} {name}: compression method"
            );
            let t = file.last_modified().expect("modification time");
            let modified = [
                t.year(),
                t.month().into(),
                t.day().into(),
                t.hour().into(),
                t.minute().into(),
                t.second().into(),
            ];
            let want_modified: Vec<u16> = want["modified"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
                .collect();
            assert_eq!(
                modified.to_vec(),
                want_modified,
                "{program} {name}: modification time"
            );
            let mut data = Vec::new();
            file.read_to_end(&mut data).unwrap();
            assert_eq!(
                data.len() as u64,
                want["size"].as_u64().unwrap(),
                "{program} {name}: size"
            );
            let (masked, n, seen) = mask(&data);
            assert_eq!(
                n as u64,
                want["masked"].as_u64().unwrap(),
                "{program} {name}: UUIDs masked"
            );
            assert_eq!(
                sha256_hex(&masked),
                want["sha256"].as_str().unwrap(),
                "{program} {name}: content"
            );
            places += n;
            values.extend(seen);
            compared += 1;
        }
        values.sort();
        values.dedup();
        assert_eq!(
            places, MASKED_PLACES,
            "{program}: the mask must take exactly the object name's places"
        );
        assert_eq!(values.len(), 1, "{program}: one object name per archive");
    }
    assert!(compared > 0, "compared no entry");
}

/// the app's export (`export_mesh_with_metadata`, which rewrites the archive with
/// `manifest::embed_in_3mf`): every entry against `zip2_app.json`. Two entries carry values of the
/// run itself (the creation time, the time taken, the hash of the archive, an id from the object
/// name): `3D/3dmodel.model` and `Metadata/alice_manifest.json` are held to their name, place, method
/// and time and to their structure; every other entry is compared by content with the object name
/// masked, which appears in exactly 4 of them, one distinct UUID
#[test]
fn every_entry_of_the_app_export_matches_the_recorded_oracle() {
    use text_to_print_core::pipeline::{MetadataInputs, export_mesh_with_metadata};
    let oracle: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(fixtures().join("zip2_app.json")).unwrap())
            .unwrap();
    let programs = oracle.as_object().expect("oracle is an object of programs");
    assert_eq!(programs.len(), 3, "the oracle holds the three programs");
    let mut compared = 0;
    for (program, entries) in programs {
        let lol = std::fs::read_to_string(fixtures().join(format!("{program}.lol"))).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let meta = MetadataInputs {
            prompt: "oracle",
            prompt_lang: "en",
            llm_model: "none",
            llm_seed: Some(1),
            retry_count: 0,
            safety_violations: vec![],
            tier: text_to_print_core::tier::Tier::Free,
        };
        let (stats, _) = export_mesh_with_metadata(
            &lol,
            dir.path(),
            ExportFormat::ThreeMf,
            Quality::Preview,
            meta,
        )
        .unwrap_or_else(|e| panic!("{program}: export: {e:#}"));
        let bytes = std::fs::read(&stats.path).unwrap();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let entries = entries.as_array().unwrap();
        assert_eq!(archive.len(), entries.len(), "{program}: number of entries");
        let (mut places, mut values) = (0, Vec::new());
        for (i, want) in entries.iter().enumerate() {
            let mut file = archive.by_index(i).unwrap();
            let name = file.name().to_string();
            assert_eq!(
                name,
                want["name"].as_str().unwrap(),
                "{program}: entry {i} name / order"
            );
            let method = match file.compression() {
                zip::CompressionMethod::Stored => 0,
                zip::CompressionMethod::Deflated => 8,
                other => panic!("{program} {name}: compression {other:?}"),
            };
            assert_eq!(
                method,
                want["method"].as_u64().unwrap(),
                "{program} {name}: compression method"
            );
            let t = file.last_modified().expect("modification time");
            let modified = [
                t.year(),
                t.month().into(),
                t.day().into(),
                t.hour().into(),
                t.minute().into(),
                t.second().into(),
            ];
            let want_modified: Vec<u16> = want["modified"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
                .collect();
            assert_eq!(
                modified.to_vec(),
                want_modified,
                "{program} {name}: modification time"
            );
            let mut data = Vec::new();
            file.read_to_end(&mut data).unwrap();
            if want.get("sha256").is_none() {
                let text = String::from_utf8(data)
                    .unwrap_or_else(|_| panic!("{program} {name}: not utf-8"));
                match name.as_str() {
                    "3D/3dmodel.model" => {
                        assert!(
                            text.contains("<model") && text.contains("<metadata name=\"Title\">"),
                            "{program}: model"
                        );
                    }
                    "Metadata/alice_manifest.json" => {
                        let m: serde_json::Value =
                            serde_json::from_str(&text).expect("manifest is JSON");
                        assert_eq!(m["schema_version"], "1", "{program}: manifest schema");
                    }
                    other => {
                        panic!("{program}: {other} has no recorded content and no structural check")
                    }
                }
                compared += 1;
                continue;
            }
            assert_eq!(
                data.len() as u64,
                want["size"].as_u64().unwrap(),
                "{program} {name}: size"
            );
            let (masked, n, seen) = mask(&data);
            assert_eq!(
                n as u64,
                want["masked"].as_u64().unwrap(),
                "{program} {name}: UUIDs masked"
            );
            assert_eq!(
                sha256_hex(&masked),
                want["sha256"].as_str().unwrap(),
                "{program} {name}: content"
            );
            places += n;
            values.extend(seen);
            compared += 1;
        }
        values.sort();
        values.dedup();
        assert_eq!(
            places, 4,
            "{program}: the mask must take exactly the object name's places"
        );
        assert_eq!(values.len(), 1, "{program}: one object name per archive");
    }
    assert!(compared > 0, "compared no entry");
}
