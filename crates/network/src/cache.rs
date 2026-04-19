use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedSdf {
    pub id: String,
    pub lol_source: String,
    pub author_did: String,
    pub created_at: String,
}

pub struct SdfCache {
    cache_dir: PathBuf,
    index: HashMap<String, CachedSdf>,
}

impl SdfCache {
    pub fn open(data_dir: &Path) -> Result<Self> {
        let cache_dir = data_dir.join("cache");
        std::fs::create_dir_all(&cache_dir)?;

        let index_path = cache_dir.join("index.json");
        let index: HashMap<String, CachedSdf> = if index_path.exists() {
            let data = std::fs::read_to_string(&index_path)?;
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            HashMap::new()
        };

        Ok(Self { cache_dir, index })
    }

    pub fn put(&mut self, sdf: CachedSdf) -> Result<()> {
        let sdf_path = self.cache_dir.join(format!("{}.lol", &sdf.id));
        std::fs::write(&sdf_path, &sdf.lol_source)?;
        self.index.insert(sdf.id.clone(), sdf);
        self.flush_index()?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&CachedSdf> {
        self.index.get(id)
    }

    pub fn list_public(&self) -> Vec<&CachedSdf> {
        self.index.values().collect()
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    fn flush_index(&self) -> Result<()> {
        let index_path = self.cache_dir.join("index.json");
        let data = serde_json::to_string_pretty(&self.index)?;
        std::fs::write(index_path, data)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_sdf(id: &str, lol: &str) -> CachedSdf {
        CachedSdf {
            id: id.to_string(),
            lol_source: lol.to_string(),
            author_did: "did:key:test".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn empty_cache() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SdfCache::open(dir.path()).unwrap();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn put_and_get() {
        let dir = tempfile::tempdir().unwrap();
        let mut cache = SdfCache::open(dir.path()).unwrap();
        cache.put(make_sdf("s1", "sphere(1.0)")).unwrap();
        assert_eq!(cache.len(), 1);
        let got = cache.get("s1").unwrap();
        assert_eq!(got.lol_source, "sphere(1.0)");
    }

    #[test]
    fn get_missing_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SdfCache::open(dir.path()).unwrap();
        assert!(cache.get("nonexistent").is_none());
    }

    #[test]
    fn list_public_returns_all() {
        let dir = tempfile::tempdir().unwrap();
        let mut cache = SdfCache::open(dir.path()).unwrap();
        cache.put(make_sdf("s1", "sphere(1.0)")).unwrap();
        cache.put(make_sdf("s2", "box3d(1.0, 1.0, 1.0)")).unwrap();
        assert_eq!(cache.list_public().len(), 2);
    }

    #[test]
    fn cache_persists_to_disk() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut cache = SdfCache::open(dir.path()).unwrap();
            cache.put(make_sdf("s1", "sphere(2.0)")).unwrap();
        }
        // Re-open
        let cache = SdfCache::open(dir.path()).unwrap();
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get("s1").unwrap().lol_source, "sphere(2.0)");
    }

    #[test]
    fn lol_file_written_to_disk() {
        let dir = tempfile::tempdir().unwrap();
        let mut cache = SdfCache::open(dir.path()).unwrap();
        cache.put(make_sdf("myid", "torus(2.0, 0.5)")).unwrap();
        let lol_path = dir.path().join("cache").join("myid.lol");
        assert!(lol_path.exists());
        assert_eq!(
            std::fs::read_to_string(lol_path).unwrap(),
            "torus(2.0, 0.5)"
        );
    }
}
