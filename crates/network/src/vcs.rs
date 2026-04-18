use serde::{Deserialize, Serialize};
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
}

impl Default for SdfDag {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(hash: &str, parent: Option<&str>, lol: &str) -> MerkleNode {
        MerkleNode {
            hash: hash.to_string(),
            parent: parent.map(|s| s.to_string()),
            author_did: "did:key:test".to_string(),
            lol_source: lol.to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn empty_dag() {
        let dag = SdfDag::new();
        assert!(dag.heads().is_empty());
        assert!(dag.get("any").is_none());
    }

    #[test]
    fn single_commit() {
        let mut dag = SdfDag::new();
        dag.commit(make_node("h1", None, "sphere(1.0)"));
        assert_eq!(dag.heads(), &["h1"]);
        assert_eq!(dag.get("h1").unwrap().lol_source, "sphere(1.0)");
    }

    #[test]
    fn linear_history() {
        let mut dag = SdfDag::new();
        dag.commit(make_node("h1", None, "v1"));
        dag.commit(make_node("h2", Some("h1"), "v2"));
        dag.commit(make_node("h3", Some("h2"), "v3"));

        assert_eq!(dag.heads(), &["h3"]);

        let hist = dag.history("h3");
        assert_eq!(hist.len(), 3);
        assert_eq!(hist[0].hash, "h3");
        assert_eq!(hist[1].hash, "h2");
        assert_eq!(hist[2].hash, "h1");
    }

    #[test]
    fn fork_creates_two_heads() {
        let mut dag = SdfDag::new();
        dag.commit(make_node("h1", None, "base"));
        dag.commit(make_node("h2", Some("h1"), "fork-a"));
        dag.commit(make_node("h3", Some("h1"), "fork-b"));

        // h1 removed when h2 committed, but h3 also removes h1
        // heads should be h2 and h3
        let heads = dag.heads();
        assert_eq!(heads.len(), 2);
        assert!(heads.contains(&"h2".to_string()));
        assert!(heads.contains(&"h3".to_string()));
    }

    #[test]
    fn history_of_missing_node() {
        let dag = SdfDag::new();
        assert!(dag.history("nonexistent").is_empty());
    }

    #[test]
    fn get_missing_returns_none() {
        let dag = SdfDag::new();
        assert!(dag.get("x").is_none());
    }

    #[test]
    fn merkle_node_serialization() {
        let node = make_node("abc", Some("parent"), "sphere(1.0)");
        let json = serde_json::to_string(&node).unwrap();
        let deserialized: MerkleNode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.hash, "abc");
        assert_eq!(deserialized.parent.as_deref(), Some("parent"));
    }
}
