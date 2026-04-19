use serde::{Deserialize, Serialize};

use crate::vcs::SdfDiff;

/// P2P ネットワークに伝播するイベント
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncEvent {
    /// 新しい SDF が公開された (General tier)
    SdfPublished {
        id: String,
        lol_source: String,
        author_did: String,
        prompt: String,
    },
    /// SDF がフォーク/リミックスされた
    SdfForked { diff: SdfDiff },
}

/// gossipsub トピック
pub const TOPIC_SDF_EVENTS: &str = "3dvbgaran/sdf/v1";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_event_published_roundtrip() {
        let event = SyncEvent::SdfPublished {
            id: "test-id".to_string(),
            lol_source: "sphere(1.0)".to_string(),
            author_did: "did:key:abc".to_string(),
            prompt: "a sphere".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: SyncEvent = serde_json::from_str(&json).unwrap();
        if let SyncEvent::SdfPublished { id, .. } = deserialized {
            assert_eq!(id, "test-id");
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn sync_event_forked_roundtrip() {
        let event = SyncEvent::SdfForked {
            diff: SdfDiff {
                original_hash: "h1".to_string(),
                fork_hash: "h2".to_string(),
                original_lol: "sphere(1.0)".to_string(),
                forked_lol: "sphere(2.0)".to_string(),
                author_did: "did:key:test".to_string(),
                timestamp: "2026-01-01".to_string(),
            },
        };
        let bytes = serde_json::to_vec(&event).unwrap();
        assert!(bytes.len() < 500); // SDF diff は軽量
        let deserialized: SyncEvent = serde_json::from_slice(&bytes).unwrap();
        if let SyncEvent::SdfForked { diff } = deserialized {
            assert_eq!(diff.forked_lol, "sphere(2.0)");
        } else {
            panic!("wrong variant");
        }
    }
}
