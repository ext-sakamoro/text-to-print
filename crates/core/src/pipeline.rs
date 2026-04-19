use alice_lol::print_export::{ExportStats, PrintConfig};
use alice_lol::runtime_parser;
use anyhow::{Result, bail};
use std::path::Path;
use tracing::info;

#[derive(Debug, Clone)]
pub struct GenerationResult {
    pub lol_source: String,
    pub stats: Option<MeshStats>,
}

#[derive(Debug, Clone)]
pub struct MeshStats {
    pub vertex_count: usize,
    pub triangle_count: usize,
    pub path: String,
}

#[derive(Debug, Clone, Copy)]
pub enum ExportFormat {
    ThreeMf,
    Fbx,
    Stl,
}

#[derive(Debug, Clone, Copy)]
pub enum Quality {
    Preview,
    High,
    Ultra,
}

impl Quality {
    pub fn to_print_config(self) -> PrintConfig {
        match self {
            Self::Preview => PrintConfig::preview(),
            Self::High => PrintConfig::high_quality(),
            Self::Ultra => PrintConfig::ultra(),
        }
    }
}

/// LOL ソースを検証（パースできるか）
pub fn validate_lol(lol_source: &str) -> Result<()> {
    runtime_parser::parse_lol(lol_source)
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("LOL parse error: {}", e.message))
}

/// LOL → メッシュファイル (.3mf / .fbx / .stl) にエクスポート
pub fn export_mesh(
    lol_source: &str,
    output_dir: &Path,
    format: ExportFormat,
    quality: Quality,
) -> Result<MeshStats> {
    let config = quality.to_print_config();

    let filename = format!("{}.{}", uuid::Uuid::new_v4(), format.extension());
    let output_path = output_dir.join(&filename);

    info!(
        format = format.extension(),
        quality = ?quality,
        path = %output_path.display(),
        "exporting mesh"
    );

    let stats = match format {
        ExportFormat::ThreeMf => {
            alice_lol::print_export::lol_to_3mf(lol_source, &output_path, &config)?
        }
        ExportFormat::Fbx => {
            alice_lol::print_export::lol_to_fbx(lol_source, &output_path, &config)?
        }
        ExportFormat::Stl => {
            alice_lol::print_export::lol_to_stl(lol_source, &output_path, &config)?
        }
    };

    Ok(to_mesh_stats(&stats))
}

fn to_mesh_stats(stats: &ExportStats) -> MeshStats {
    MeshStats {
        vertex_count: stats.vertex_count,
        triangle_count: stats.triangle_count,
        path: stats.path.clone(),
    }
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::ThreeMf => "3mf",
            Self::Fbx => "fbx",
            Self::Stl => "stl",
        }
    }
}

/// LOL ソースからコードブロックを抽出
pub fn extract_lol(llm_response: &str) -> Option<String> {
    let start = llm_response.find("```lol")?;
    let code_start = llm_response[start..].find('\n')? + start + 1;
    let end = llm_response[code_start..].find("```")? + code_start;
    Some(llm_response[code_start..end].trim().to_string())
}

/// LOL → WGSL シェーダー生成（SDF プレビュー用）
pub fn lol_to_wgsl(lol_source: &str) -> Result<String> {
    let node = runtime_parser::parse_lol(lol_source)
        .map_err(|e| anyhow::anyhow!("LOL parse error: {}", e.message))?;

    let wgsl = alice_lol::to_wgsl(&node);
    if wgsl.is_empty() {
        bail!("WGSL generation failed: empty output");
    }
    Ok(wgsl)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_lol() {
        let response = "Here is the model:\n```lol\nsphere(1.0)\n```\nDone.";
        assert_eq!(extract_lol(response), Some("sphere(1.0)".to_string()));
    }

    #[test]
    fn test_extract_lol_no_block() {
        assert_eq!(extract_lol("no code here"), None);
    }

    #[test]
    fn test_validate_lol_valid() {
        assert!(validate_lol("sphere(1.0)").is_ok());
    }

    #[test]
    fn test_validate_lol_invalid() {
        assert!(validate_lol("not_a_primitive(1.0)").is_err());
    }
}
