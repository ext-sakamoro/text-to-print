//! Model 選択 (`alice-llm-server` sidecar 対象モデル)
//!
//! - **Qwen 2.5-3B-Instruct Q4_K_M** (~2 GB、推奨、Mac/Windows/Linux 全対応)
//!   v0.1.0-beta.1 (2026-08-07): Qwen 3.5-4B (公式未 publish) の substitute
//!   として 7B → 3B にダウンサイズ Apple M3 iGPU (limited memory) での
//!   prompt processing latency を軽減 (10-15 分 → 数十秒 target)
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
    /// Gemma 2 27B (Q3_K_L, ~14 GB) Real HF repo
    /// `bartowski/gemma-2-27b-it-GGUF` Exercises the `Gemma`
    /// [`crate::embedded_backend::ChatTemplate`] path
    Gemma2_27B,
    /// Bonsai 27B Q1_0 (~7 GB) — PrismML fork with 128-element binary
    /// ternary quantisation Uses the same Gemma-family chat template as
    /// [`Self::Gemma2_27B`] `default_hf_ref` returns a placeholder
    /// (`Project-ALICE/Bonsai-27B-Q1_0-GGUF`) that is **not yet
    /// public**; auto-download will 404 and surface
    /// `EmbeddedStatus::Error` To exercise this variant today, drop the
    /// GGUF file at
    /// `{data_dir}/models/bonsai-27b-q1_0.gguf` manually and the
    /// existing load path will pick it up (Stage 3-C.9 model_exists
    /// check short-circuits the DL wait)
    Bonsai27B,
}

impl ModelChoice {
    /// 全 variant を UI dropdown 用に列挙
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[Self::Qwen35_4B, Self::Gemma2_27B, Self::Bonsai27B]
    }

    /// OpenAI 互換 API の `model` field に載せる ID (sidecar は無視するが log に出る)
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::Qwen35_4B => "qwen3.5-4b-q4_k_m",
            Self::Gemma2_27B => "gemma-2-27b-it-q3_k_l",
            Self::Bonsai27B => "bonsai-27b-q1_0",
        }
    }

    /// UI 表示用の human readable label
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            // 2026-08-07 β: 7B → 3B に substitute (Qwen35_4B enum 名は
            // DB persisted なので保持、実 model のみ差替)
            Self::Qwen35_4B => "Qwen 2.5-3B-Instruct (Q4_K_M, ~2 GB)",
            Self::Gemma2_27B => "Gemma 2 27B (Q3_K_L, ~14GB)",
            Self::Bonsai27B => "Bonsai 27B (Q1_0, ~7GB, manual)",
        }
    }

    /// GGUF file の default 名 (models_dir 配下から探索)
    #[must_use]
    pub const fn default_filename(self) -> &'static str {
        match self {
            // 2026-08-07 β: filename も 3B 版に変更 (7B の orphan file は
            // ~/Library/Application Support/net.alicelaw.text-to-print/models/
            // qwen3.5-4b-q4_k_m.gguf に残るので手動削除推奨、~5 GB 空き回復)
            Self::Qwen35_4B => "qwen2.5-3b-instruct-q4_k_m.gguf",
            Self::Gemma2_27B => "gemma-2-27b-it-q3_k_l.gguf",
            Self::Bonsai27B => "bonsai-27b-q1_0.gguf",
        }
    }

    /// Stage 3-C.9 + 3-C.10 + 3-C.17: Hugging Face repo + filename tuple
    /// used by [`crate::downloader::download_model`] to construct the
    /// resolve URL
    ///
    /// Format: `(repo_id, file_within_repo)` The URL becomes
    /// `https://huggingface.co/{repo}/resolve/main/{file}`
    ///
    /// - `Qwen35_4B` / `Gemma2_27B`: resolve to publicly-available GGUFs
    /// - `Bonsai27B`: **placeholder** `Project-ALICE/Bonsai-27B-Q1_0-GGUF`
    ///   is not yet public Attempting the DL will 404 →
    ///   `EmbeddedStatus::Error` To exercise the variant place the
    ///   file at `models/bonsai-27b-q1_0.gguf` manually (see doc on
    ///   [`Self::Bonsai27B`])
    #[must_use]
    pub const fn default_hf_ref(self) -> (&'static str, &'static str) {
        match self {
            Self::Qwen35_4B => (
                // 2026-08-07 β: 7B → 3B ダウンサイズ Apple M3 iGPU で 7B
                // Q4_K_M + LOL_GBNF grammar constrained decoding は prompt
                // processing に 10 分以上かかる事案 (user 実測 9 分で 0%)
                // 3B は同じ Qwen 官方 repo で single file (~2 GB) 提供、
                // prompt processing 数十秒 target Qwen 3.5-4B が公式 publish
                // 済次第再検討 (enum 名 Qwen35_4B は DB persisted なので保持)
                "Qwen/Qwen2.5-3B-Instruct-GGUF",
                "qwen2.5-3b-instruct-q4_k_m.gguf",
            ),
            Self::Gemma2_27B => (
                "bartowski/gemma-2-27b-it-GGUF",
                "gemma-2-27b-it-Q3_K_L.gguf",
            ),
            Self::Bonsai27B => ("Project-ALICE/Bonsai-27B-Q1_0-GGUF", "bonsai-27b-q1_0.gguf"),
        }
    }

    /// Stage 3-C.17: whether this choice requires the user to place the
    /// GGUF file manually because the referenced HF repo is not
    /// publicly available yet Callers (Settings UI, download flow) can
    /// use this to surface a "manual placement required" hint instead
    /// of showing a generic "download failed" error
    #[must_use]
    pub const fn requires_manual_placement(self) -> bool {
        matches!(self, Self::Bonsai27B)
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
    fn all_contains_three() {
        assert_eq!(ModelChoice::all().len(), 3);
        assert!(ModelChoice::all().contains(&ModelChoice::Qwen35_4B));
        assert!(ModelChoice::all().contains(&ModelChoice::Gemma2_27B));
        assert!(ModelChoice::all().contains(&ModelChoice::Bonsai27B));
    }

    #[test]
    fn requires_manual_placement_is_bonsai_only() {
        assert!(!ModelChoice::Qwen35_4B.requires_manual_placement());
        assert!(!ModelChoice::Gemma2_27B.requires_manual_placement());
        assert!(ModelChoice::Bonsai27B.requires_manual_placement());
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
