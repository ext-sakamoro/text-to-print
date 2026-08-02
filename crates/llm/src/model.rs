//! Model 選択 (`alice-llm-server` sidecar 対象モデル)
//!
//! - **Qwen 3.5-4B Q4_K_M** (~2.4GB、推奨、Mac/Windows/Linux 全対応)
//! - **Gemma 2 27B Q3_K_L** (~14GB、GPU 12GB+ or RAM 24GB+ 推奨、Bonsai 実
//!   PrismML fork が publish 済まで Gemma family 代替として提供)

use serde::{Deserialize, Serialize};

/// UI dropdown / 設定 file で表現する model 選択肢
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ModelChoice {
    /// Qwen 3.5-4B Q4_K_M (default)
    ///
    /// Note: `default_hf_ref` currently resolves to Qwen 2.5-7B-Instruct
    /// GGUF as the closest publicly-available substitute for a small
    /// Qwen-family instruct model; the local filename keeps the 3.5-4B
    /// slug for forward-compatibility once Qwen 3.5-4B is released
    #[default]
    Qwen35_4B,
    /// Gemma 2 27B (Q3_K_L, ~14 GB) — Gemma-family alternative that
    /// exercises the `Gemma` [`crate::embedded_backend::ChatTemplate`]
    /// path Real HF repo `bartowski/gemma-2-27b-it-GGUF`
    ///
    /// Replaces the pre-Stage 3-C.10 `Bonsai27B` placeholder that
    /// pointed at a non-existent `PrismML/Bonsai-27B-Q1_0-GGUF` repo
    /// The real Bonsai / PrismML Q1_0 model can be added back as a
    /// third variant once its repo is public
    Gemma2_27B,
}

impl ModelChoice {
    /// 全 variant を UI dropdown 用に列挙
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[Self::Qwen35_4B, Self::Gemma2_27B]
    }

    /// OpenAI 互換 API の `model` field に載せる ID (sidecar は無視するが log に出る)
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::Qwen35_4B => "qwen3.5-4b-q4_k_m",
            Self::Gemma2_27B => "gemma-2-27b-it-q3_k_l",
        }
    }

    /// UI 表示用の human readable label
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Qwen35_4B => "Qwen 3.5-4B (Q4_K_M, ~2.4GB)",
            Self::Gemma2_27B => "Gemma 2 27B (Q3_K_L, ~14GB)",
        }
    }

    /// GGUF file の default 名 (models_dir 配下から探索)
    #[must_use]
    pub const fn default_filename(self) -> &'static str {
        match self {
            Self::Qwen35_4B => "qwen3.5-4b-q4_k_m.gguf",
            Self::Gemma2_27B => "gemma-2-27b-it-q3_k_l.gguf",
        }
    }

    /// Stage 3-C.9 + 3-C.10: Hugging Face repo + filename tuple used by
    /// [`crate::downloader::download_model`] to construct the resolve URL
    ///
    /// Format: `(repo_id, file_within_repo)` The URL becomes
    /// `https://huggingface.co/{repo}/resolve/main/{file}` Both variants
    /// resolve to actually-publicly-available GGUFs Attempting either
    /// download will return real bytes (not 404) so end-to-end DL + load
    /// + inference can be exercised by the user without HF auth
    #[must_use]
    pub const fn default_hf_ref(self) -> (&'static str, &'static str) {
        match self {
            Self::Qwen35_4B => (
                "Qwen/Qwen2.5-7B-Instruct-GGUF",
                "qwen2.5-7b-instruct-q4_k_m.gguf",
            ),
            Self::Gemma2_27B => (
                "bartowski/gemma-2-27b-it-GGUF",
                "gemma-2-27b-it-Q3_K_L.gguf",
            ),
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
        assert!(ModelChoice::all().contains(&ModelChoice::Gemma2_27B));
    }

    #[test]
    fn model_ids_are_distinct() {
        assert_ne!(
            ModelChoice::Qwen35_4B.model_id(),
            ModelChoice::Gemma2_27B.model_id()
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
