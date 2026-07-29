use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Cursor, Read, Write};
use std::path::Path;
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub const SCHEMA_VERSION: &str = "1";
pub const SCHEMA_URL: &str = "https://alicelaw.net/text-to-print/schema/v1/manifest.schema.json";
pub const ALICE_NAMESPACE: &str = "https://alicelaw.net/text-to-print/ns";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AliceManifest {
    pub schema_version: String,
    pub uuid: Uuid,
    pub timestamp: DateTime<Utc>,
    pub generation: Generation,
    pub quality: Quality,
    pub environment: Environment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Generation {
    pub prompt: String,
    pub prompt_lang: String,
    pub llm_model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_seed: Option<u64>,
    pub lol_source: String,
    pub lol_sha256: String,
    pub mesh_sha256: String,
    pub vertex_count: usize,
    pub triangle_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quality {
    pub success: bool,
    pub retry_count: u32,
    pub time_to_file_ms: u64,
    pub safety_violations: Vec<String>,
    pub export_format: String,
    pub user_kept: bool,
    pub user_edited: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
    pub app_version: String,
    pub os: String,
    pub arch: String,
}

impl Environment {
    pub fn detect() -> Self {
        Self {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
        }
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub struct ManifestBuilder<'a> {
    pub prompt: &'a str,
    pub prompt_lang: &'a str,
    pub llm_model: &'a str,
    pub llm_seed: Option<u64>,
    pub lol_source: &'a str,
    pub mesh_bytes: &'a [u8],
    pub vertex_count: usize,
    pub triangle_count: usize,
    pub export_format: &'a str,
    pub retry_count: u32,
    pub time_to_file_ms: u64,
    pub safety_violations: Vec<String>,
    pub success: bool,
}

impl<'a> ManifestBuilder<'a> {
    pub fn build(self) -> AliceManifest {
        AliceManifest {
            schema_version: SCHEMA_VERSION.to_string(),
            uuid: Uuid::now_v7(),
            timestamp: Utc::now(),
            generation: Generation {
                prompt: self.prompt.to_string(),
                prompt_lang: self.prompt_lang.to_string(),
                llm_model: self.llm_model.to_string(),
                llm_seed: self.llm_seed,
                lol_source: self.lol_source.to_string(),
                lol_sha256: sha256_hex(self.lol_source.as_bytes()),
                mesh_sha256: sha256_hex(self.mesh_bytes),
                vertex_count: self.vertex_count,
                triangle_count: self.triangle_count,
            },
            quality: Quality {
                success: self.success,
                retry_count: self.retry_count,
                time_to_file_ms: self.time_to_file_ms,
                safety_violations: self.safety_violations,
                export_format: self.export_format.to_string(),
                user_kept: false,
                user_edited: false,
            },
            environment: Environment::detect(),
        }
    }
}

/// Rewrite an existing 3MF file to embed alice_manifest.json + XML metadata tags.
///
/// Reads all ZIP entries into memory, injects `<metadata name="alice:...">`
/// tags into `3D/3dmodel.model` right after the opening `<model ...>` element,
/// and adds `Metadata/alice_manifest.json` as a new STORE entry.
pub fn embed_in_3mf(path: &Path, manifest: &AliceManifest) -> Result<()> {
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let mut archive = ZipArchive::new(Cursor::new(bytes)).with_context(|| "parse 3MF as ZIP")?;

    let mut entries: Vec<(String, Vec<u8>)> = Vec::with_capacity(archive.len() + 1);
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut data)?;
        entries.push((name, data));
    }

    let manifest_json = serde_json::to_vec_pretty(manifest)?;

    let out =
        std::fs::File::create(path).with_context(|| format!("recreate {}", path.display()))?;
    let mut writer = ZipWriter::new(std::io::BufWriter::new(out));
    let options: SimpleFileOptions =
        SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

    for (name, mut data) in entries {
        if name == "3D/3dmodel.model" {
            let s = String::from_utf8(data).with_context(|| "3dmodel.model not utf-8")?;
            data = inject_metadata_xml(&s, manifest).into_bytes();
        }
        writer.start_file(name, options)?;
        writer.write_all(&data)?;
    }

    writer.start_file("Metadata/alice_manifest.json", options)?;
    writer.write_all(&manifest_json)?;
    writer.finish()?;
    Ok(())
}

fn inject_metadata_xml(model_xml: &str, m: &AliceManifest) -> String {
    let mut out = String::with_capacity(model_xml.len() + 512);

    let (before_model, model_and_after) = match model_xml.find("<model") {
        Some(i) => (&model_xml[..i], &model_xml[i..]),
        None => return model_xml.to_string(),
    };
    let (open_tag_end_rel, _) = match model_and_after.find('>') {
        Some(i) => (i, ()),
        None => return model_xml.to_string(),
    };
    let open_tag = &model_and_after[..=open_tag_end_rel];
    let rest = &model_and_after[open_tag_end_rel + 1..];

    let open_tag_with_ns = if open_tag.contains("xmlns:alice") {
        open_tag.to_string()
    } else {
        let insert_at = open_tag.len() - 1;
        let mut buf = String::with_capacity(open_tag.len() + 64);
        buf.push_str(&open_tag[..insert_at]);
        buf.push_str(&format!(" xmlns:alice=\"{ALICE_NAMESPACE}\""));
        buf.push_str(&open_tag[insert_at..]);
        buf
    };

    out.push_str(before_model);
    out.push_str(&open_tag_with_ns);
    out.push('\n');
    for (k, v) in [
        ("alice:schema_version", m.schema_version.as_str()),
        ("alice:uuid", &m.uuid.to_string()),
        ("alice:timestamp", &m.timestamp.to_rfc3339()),
        ("alice:llm_model", m.generation.llm_model.as_str()),
        ("alice:lol_sha256", m.generation.lol_sha256.as_str()),
        ("alice:mesh_sha256", m.generation.mesh_sha256.as_str()),
    ] {
        out.push_str(&format!(
            "  <metadata name=\"{k}\">{}</metadata>\n",
            xml_escape(v)
        ));
    }
    out.push_str(rest);
    out
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest() -> AliceManifest {
        ManifestBuilder {
            prompt: "a cube 20mm",
            prompt_lang: "en",
            llm_model: "qwen3.5-4b-q4km",
            llm_seed: Some(42),
            lol_source: "cube(20.0)",
            mesh_bytes: b"fake-mesh-bytes",
            vertex_count: 8,
            triangle_count: 12,
            export_format: "3mf",
            retry_count: 0,
            time_to_file_ms: 1234,
            safety_violations: vec![],
            success: true,
        }
        .build()
    }

    #[test]
    fn sha256_hex_matches_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn manifest_json_roundtrip_preserves_fields() {
        let m = sample_manifest();
        let json = serde_json::to_string(&m).unwrap();
        let back: AliceManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.schema_version, "1");
        assert_eq!(back.uuid, m.uuid);
        assert_eq!(back.generation.prompt, "a cube 20mm");
        assert_eq!(back.generation.vertex_count, 8);
        assert_eq!(back.quality.time_to_file_ms, 1234);
        assert_eq!(back.quality.export_format, "3mf");
    }

    #[test]
    fn inject_metadata_adds_namespace_and_tags() {
        let src =
            "<?xml version=\"1.0\"?>\n<model unit=\"millimeter\" xmlns=\"http://x\">\n</model>";
        let m = sample_manifest();
        let out = inject_metadata_xml(src, &m);
        assert!(out.contains("xmlns:alice=\"https://alicelaw.net/text-to-print/ns\""));
        assert!(out.contains("<metadata name=\"alice:uuid\">"));
        assert!(out.contains("<metadata name=\"alice:lol_sha256\">"));
    }

    #[test]
    fn embed_in_3mf_adds_manifest_and_metadata() {
        use std::io::Write as _;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.3mf");

        {
            let f = std::fs::File::create(&path).unwrap();
            let mut zw = ZipWriter::new(f);
            let opts: SimpleFileOptions =
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            zw.start_file("[Content_Types].xml", opts).unwrap();
            zw.write_all(b"<Types/>").unwrap();
            zw.start_file("3D/3dmodel.model", opts).unwrap();
            zw.write_all(
                b"<?xml version=\"1.0\"?>\n<model unit=\"millimeter\" xmlns=\"http://x\">\n</model>",
            )
            .unwrap();
            zw.finish().unwrap();
        }

        let m = sample_manifest();
        embed_in_3mf(&path, &m).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        let mut ar = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let names: Vec<String> = (0..ar.len())
            .map(|i| ar.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.contains(&"[Content_Types].xml".to_string()));
        assert!(names.contains(&"3D/3dmodel.model".to_string()));
        assert!(names.contains(&"Metadata/alice_manifest.json".to_string()));

        let mut man = ar.by_name("Metadata/alice_manifest.json").unwrap();
        let mut json = String::new();
        man.read_to_string(&mut json).unwrap();
        let parsed: AliceManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.uuid, m.uuid);
        drop(man);

        let mut model = ar.by_name("3D/3dmodel.model").unwrap();
        let mut xml = String::new();
        model.read_to_string(&mut xml).unwrap();
        assert!(xml.contains("xmlns:alice="));
        assert!(xml.contains("<metadata name=\"alice:uuid\">"));
    }
}
