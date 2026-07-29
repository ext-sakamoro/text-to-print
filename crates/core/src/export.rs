use anyhow::Result;
use std::path::Path;

use crate::pipeline::ExportFormat;

pub fn save_mesh(
    data: &[u8],
    format: ExportFormat,
    output_dir: &Path,
) -> Result<std::path::PathBuf> {
    let filename = format!("{}.{}", uuid::Uuid::now_v7(), format.extension());
    let path = output_dir.join(filename);
    std::fs::write(&path, data)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn save_mesh_creates_3mf_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = save_mesh(b"test-data", ExportFormat::ThreeMf, dir.path()).unwrap();
        assert!(path.exists());
        assert_eq!(path.extension().unwrap(), "3mf");
        assert_eq!(std::fs::read(&path).unwrap(), b"test-data");
    }

    #[test]
    fn save_mesh_creates_fbx_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = save_mesh(b"fbx-data", ExportFormat::Fbx, dir.path()).unwrap();
        assert!(path.exists());
        assert_eq!(path.extension().unwrap(), "fbx");
    }

    #[test]
    fn save_mesh_creates_stl_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = save_mesh(b"stl-data", ExportFormat::Stl, dir.path()).unwrap();
        assert!(path.exists());
        assert_eq!(path.extension().unwrap(), "stl");
    }

    #[test]
    fn save_mesh_fails_on_bad_path() {
        let bad_path = PathBuf::from("/nonexistent/deeply/nested/dir");
        assert!(save_mesh(b"data", ExportFormat::ThreeMf, &bad_path).is_err());
    }
}
