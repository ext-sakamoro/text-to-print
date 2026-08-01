//! Model 選択 (`alice-llm-server` sidecar 対象モデル)
//!
//! - **Qwen 3.5-4B Q4_K_M** (~2.4GB、推奨、Mac/Windows/Linux 全対応)
//! - **Bonsai 27B Q1_0** (~7GB、高品質、GPU 8GB+ 推奨、Jetson は `--hybrid`)

use serde::{Deserialize, Serialize};

/// UI dropdown / 設定 file で表現する model 選択肢
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ModelChoice {
    /// Qwen 3.5-4B Q4_K_M (default)
    #[default]
    Qwen35_4B,
    /// Bonsai 27B Q1_0
    Bonsai27B,
}

impl ModelChoice {
    /// 全 variant を UI dropdown 用に列挙
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[Self::Qwen35_4B, Self::Bonsai27B]
    }

    /// OpenAI 互換 API の `model` field に載せる ID (sidecar は無視するが log に出る)
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::Qwen35_4B => "qwen3.5-4b-q4_k_m",
            Self::Bonsai27B => "bonsai-27b-q1_0",
        }
    }

    /// UI 表示用の human readable label
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Qwen35_4B => "Qwen 3.5-4B (Q4_K_M, ~2.4GB)",
            Self::Bonsai27B => "Bonsai 27B (Q1_0, ~7GB)",
        }
    }

    /// GGUF file の default 名 (models_dir 配下から探索)
    #[must_use]
    pub const fn default_filename(self) -> &'static str {
        match self {
            Self::Qwen35_4B => "qwen3.5-4b-q4_k_m.gguf",
            Self::Bonsai27B => "bonsai-27b-q1_0.gguf",
        }
    }

    /// Stage 3-C.9: Hugging Face repo + filename tuple used by
    /// [`crate::downloader::download_model`] to construct the resolve URL
    ///
    /// Format: `(repo_id, file_within_repo)` The URL becomes
    /// `https://huggingface.co/{repo}/resolve/main/{file}`
    ///
    /// Placeholder Bonsai repo `Project-ALICE/Bonsai-27B-GGUF` is a
    /// stand-in until the actual repo is published — attempting to
    /// download it will surface an HTTP 404 which the downloader turns
    /// into `DownloadStatus::Error`, and the UI falls back to Sidecar
    #[must_use]
    pub const fn default_hf_ref(self) -> (&'static str, &'static str) {
        match self {
            Self::Qwen35_4B => (
                "Qwen/Qwen2.5-7B-Instruct-GGUF",
                "qwen2.5-7b-instruct-q4_k_m.gguf",
            ),
            Self::Bonsai27B => ("Project-ALICE/Bonsai-27B-GGUF", "bonsai-27b-q1_0.gguf"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_qwen() {
        assert_eq!(ModelChoice::default(), ModelChoice::Qwen35_4B);
    }

    #[test]
    fn all_contains_two() {
        assert_eq!(ModelChoice::all().len(), 2);
        assert!(ModelChoice::all().contains(&ModelChoice::Qwen35_4B));
        assert!(ModelChoice::all().contains(&ModelChoice::Bonsai27B));
    }

    #[test]
    fn model_ids_are_distinct() {
        assert_ne!(
            ModelChoice::Qwen35_4B.model_id(),
            ModelChoice::Bonsai27B.model_id()
        );
    }

    #[test]
    fn labels_are_non_empty() {
        for m in ModelChoice::all() {
            assert!(!m.label().is_empty());
            assert!(!m.model_id().is_empty());
            assert!(m.default_filename().ends_with(".gguf"));
        }
    }

    #[test]
    fn serialization_roundtrip() {
        for m in ModelChoice::all() {
            let json = serde_json::to_string(m).unwrap();
            let back: ModelChoice = serde_json::from_str(&json).unwrap();
            assert_eq!(*m, back);
        }
    }
}
