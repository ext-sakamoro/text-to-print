use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Merkle DAG のノード (SDF バージョン管理)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleNode {
    pub hash: String,
    pub parent: Option<String>,
    pub author_did: String,
    pub lol_source: String,
    pub timestamp: String,
}

/// LOL ソースの diff（フォーク時に P2P 伝播する最小単位）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdfDiff {
    pub original_hash: String,
    pub fork_hash: String,
    pub original_lol: String,
    pub forked_lol: String,
    pub author_did: String,
    pub timestamp: String,
}

impl SdfDiff {
    /// diff のバイトサイズ（SDF 数式なので数十〜数百バイト）
    pub fn wire_size(&self) -> usize {
        self.original_hash.len()
            + self.fork_hash.len()
            + self.original_lol.len()
            + self.forked_lol.len()
            + self.author_did.len()
            + self.timestamp.len()
    }
}

/// SDF の変更履歴を管理する Merkle DAG
pub struct SdfDag {
    nodes: HashMap<String, MerkleNode>,
    heads: Vec<String>,
}

impl SdfDag {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            heads: Vec::new(),
        }
    }

    /// LOL ソースから content-addressable ハッシュを生成
    pub fn hash_lol(lol_source: &str, author_did: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(lol_source.as_bytes());
        hasher.update(author_did.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// 新しい SDF をコミット（初回公開）
    pub fn commit_new(&mut self, lol_source: &str, author_did: &str) -> MerkleNode {
        let hash = Self::hash_lol(lol_source, author_did);
        let node = MerkleNode {
            hash: hash.clone(),
            parent: None,
            author_did: author_did.to_string(),
            lol_source: lol_source.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        self.commit(node.clone());
        node
    }

    /// 既存 SDF をフォーク（リミックス）
    pub fn fork(
        &mut self,
        original_hash: &str,
        new_lol_source: &str,
        author_did: &str,
    ) -> Option<(MerkleNode, SdfDiff)> {
        let original = self.get(original_hash)?.clone();

        let fork_hash = Self::hash_lol(new_lol_source, author_did);
        let node = MerkleNode {
            hash: fork_hash.clone(),
            parent: Some(original_hash.to_string()),
            author_did: author_did.to_string(),
            lol_source: new_lol_source.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        let diff = SdfDiff {
            original_hash: original_hash.to_string(),
            fork_hash,
            original_lol: original.lol_source.clone(),
            forked_lol: new_lol_source.to_string(),
            author_did: author_did.to_string(),
            timestamp: node.timestamp.clone(),
        };

        self.commit(node.clone());
        Some((node, diff))
    }

    /// diff からフォークを適用（P2P 受信時）
    pub fn apply_diff(&mut self, diff: &SdfDiff) -> Option<MerkleNode> {
        // LOL を検証
        if alice_lol::runtime_parser::parse_lol(&diff.forked_lol).is_err() {
            tracing::warn!(hash = %diff.fork_hash, "received invalid forked LOL, dropping");
            return None;
        }

        let node = MerkleNode {
            hash: diff.fork_hash.clone(),
            parent: Some(diff.original_hash.clone()),
            author_did: diff.author_did.clone(),
            lol_source: diff.forked_lol.clone(),
            timestamp: diff.timestamp.clone(),
        };

        self.commit(node.clone());
        Some(node)
    }

    pub fn commit(&mut self, node: MerkleNode) {
        if let Some(ref parent) = node.parent {
            self.heads.retain(|h| h != parent);
        }
        let hash = node.hash.clone();
        self.nodes.insert(hash.clone(), node);
        self.heads.push(hash);
    }

    pub fn get(&self, hash: &str) -> Option<&MerkleNode> {
        self.nodes.get(hash)
    }

    pub fn heads(&self) -> &[String] {
        &self.heads
    }

    pub fn history(&self, hash: &str) -> Vec<&MerkleNode> {
        let mut result = Vec::new();
        let mut current = Some(hash.to_string());
        while let Some(h) = current {
            if let Some(node) = self.nodes.get(&h) {
                result.push(node);
                current = node.parent.clone();
            } else {
                break;
            }
        }
        result
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// DAG をファイルに保存
    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let data = DagSnapshot {
            nodes: self.nodes.values().cloned().collect(),
            heads: self.heads.clone(),
        };
        let json = serde_json::to_string_pretty(&data)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// ファイルから DAG を読み込み、なければ空の DAG を返す
    pub fn load_or_new(path: &std::path::Path) -> Self {
        if path.exists()
            && let Ok(json) = std::fs::read_to_string(path)
            && let Ok(snapshot) = serde_json::from_str::<DagSnapshot>(&json)
        {
            let nodes: HashMap<String, MerkleNode> = snapshot
                .nodes
                .into_iter()
                .map(|n| (n.hash.clone(), n))
                .collect();
            return Self {
                nodes,
                heads: snapshot.heads,
            };
        }
        Self::new()
    }
}

#[derive(Serialize, Deserialize)]
struct DagSnapshot {
    nodes: Vec<MerkleNode>,
    heads: Vec<String>,
}

impl Default for SdfDag {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_dag() {
        let dag = SdfDag::new();
        assert!(dag.heads().is_empty());
        assert!(dag.get("any").is_none());
        assert_eq!(dag.node_count(), 0);
    }

    #[test]
    fn commit_new() {
        let mut dag = SdfDag::new();
        let node = dag.commit_new("sphere(1.0)", "did:key:abc");
        assert_eq!(dag.heads(), &[node.hash.clone()]);
        assert_eq!(dag.get(&node.hash).unwrap().lol_source, "sphere(1.0)");
        assert_eq!(dag.node_count(), 1);
    }

    #[test]
    fn hash_is_deterministic() {
        let h1 = SdfDag::hash_lol("sphere(1.0)", "did:key:abc");
        let h2 = SdfDag::hash_lol("sphere(1.0)", "did:key:abc");
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_differs_by_content() {
        let h1 = SdfDag::hash_lol("sphere(1.0)", "did:key:abc");
        let h2 = SdfDag::hash_lol("sphere(2.0)", "did:key:abc");
        assert_ne!(h1, h2);
    }

    #[test]
    fn hash_differs_by_author() {
        let h1 = SdfDag::hash_lol("sphere(1.0)", "did:key:abc");
        let h2 = SdfDag::hash_lol("sphere(1.0)", "did:key:xyz");
        assert_ne!(h1, h2);
    }

    #[test]
    fn fork_creates_parent_link() {
        let mut dag = SdfDag::new();
        let original = dag.commit_new("sphere(1.0)", "did:key:a");
        let (forked, diff) = dag
            .fork(&original.hash, "sphere(2.0)", "did:key:b")
            .unwrap();

        assert_eq!(forked.parent.as_deref(), Some(original.hash.as_str()));
        assert_eq!(diff.original_lol, "sphere(1.0)");
        assert_eq!(diff.forked_lol, "sphere(2.0)");
        assert!(diff.wire_size() < 500); // SDF diff は軽量
    }

    #[test]
    fn fork_updates_heads() {
        let mut dag = SdfDag::new();
        let original = dag.commit_new("sphere(1.0)", "did:key:a");
        let (forked, _) = dag
            .fork(&original.hash, "sphere(2.0)", "did:key:b")
            .unwrap();

        // original is no longer a head, forked is
        assert!(!dag.heads().contains(&original.hash));
        assert!(dag.heads().contains(&forked.hash));
    }

    #[test]
    fn fork_nonexistent_returns_none() {
        let mut dag = SdfDag::new();
        assert!(
            dag.fork("nonexistent", "sphere(1.0)", "did:key:a")
                .is_none()
        );
    }

    #[test]
    fn linear_history() {
        let mut dag = SdfDag::new();
        let n1 = dag.commit_new("v1", "did:key:a");
        let (n2, _) = dag.fork(&n1.hash, "v2", "did:key:a").unwrap();
        let (n3, _) = dag.fork(&n2.hash, "v3", "did:key:a").unwrap();

        let hist = dag.history(&n3.hash);
        assert_eq!(hist.len(), 3);
        assert_eq!(hist[0].lol_source, "v3");
        assert_eq!(hist[1].lol_source, "v2");
        assert_eq!(hist[2].lol_source, "v1");
    }

    #[test]
    fn apply_diff_valid() {
        let mut dag = SdfDag::new();
        let original = dag.commit_new("sphere(1.0)", "did:key:a");

        let diff = SdfDiff {
            original_hash: original.hash.clone(),
            fork_hash: SdfDag::hash_lol("sphere(2.0)", "did:key:b"),
            original_lol: "sphere(1.0)".to_string(),
            forked_lol: "sphere(2.0)".to_string(),
            author_did: "did:key:b".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        let node = dag.apply_diff(&diff).unwrap();
        assert_eq!(node.lol_source, "sphere(2.0)");
        assert_eq!(dag.node_count(), 2);
    }

    #[test]
    fn apply_diff_invalid_lol_rejected() {
        let mut dag = SdfDag::new();
        dag.commit_new("sphere(1.0)", "did:key:a");

        let diff = SdfDiff {
            original_hash: "whatever".to_string(),
            fork_hash: "hash2".to_string(),
            original_lol: "sphere(1.0)".to_string(),
            forked_lol: "invalid_garbage!!!".to_string(),
            author_did: "did:key:b".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        assert!(dag.apply_diff(&diff).is_none());
        assert_eq!(dag.node_count(), 1); // not added
    }

    #[test]
    fn history_of_missing_node() {
        let dag = SdfDag::new();
        assert!(dag.history("nonexistent").is_empty());
    }

    #[test]
    fn merkle_node_serialization() {
        let mut dag = SdfDag::new();
        let node = dag.commit_new("sphere(1.0)", "did:key:test");
        let json = serde_json::to_string(&node).unwrap();
        let deserialized: MerkleNode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.hash, node.hash);
    }

    #[test]
    fn diff_serialization() {
        let diff = SdfDiff {
            original_hash: "h1".to_string(),
            fork_hash: "h2".to_string(),
            original_lol: "sphere(1.0)".to_string(),
            forked_lol: "sphere(2.0)".to_string(),
            author_did: "did:key:test".to_string(),
            timestamp: "2026-01-01".to_string(),
        };
        let json = serde_json::to_string(&diff).unwrap();
        let deserialized: SdfDiff = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.fork_hash, "h2");
    }

    #[test]
    fn dag_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dag.json");

        let mut dag = SdfDag::new();
        dag.commit_new("sphere(1.0)", "did:key:a");
        dag.commit_new("box3d(1.0, 1.0, 1.0)", "did:key:b");
        dag.save(&path).unwrap();

        let loaded = SdfDag::load_or_new(&path);
        assert_eq!(loaded.node_count(), 2);
        assert_eq!(loaded.heads().len(), 2);
    }

    #[test]
    fn dag_load_missing_file_returns_empty() {
        let dag = SdfDag::load_or_new(std::path::Path::new("/nonexistent/dag.json"));
        assert_eq!(dag.node_count(), 0);
    }

    #[test]
    fn dag_save_preserves_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dag.json");

        let mut dag = SdfDag::new();
        let n1 = dag.commit_new("v1", "did:key:a");
        let (n2, _) = dag.fork(&n1.hash, "v2", "did:key:a").unwrap();
        dag.save(&path).unwrap();

        let loaded = SdfDag::load_or_new(&path);
        let hist = loaded.history(&n2.hash);
        assert_eq!(hist.len(), 2);
        assert_eq!(hist[0].lol_source, "v2");
        assert_eq!(hist[1].lol_source, "v1");
    }
}
