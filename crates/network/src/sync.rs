use serde::{Deserialize, Serialize};

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
    SdfForked {
        original_id: String,
        fork_id: String,
        diff: Vec<u8>,
        author_did: String,
    },
}

/// gossipsub トピック
pub const TOPIC_SDF_EVENTS: &str = "3dvbgaran/sdf/v1";
