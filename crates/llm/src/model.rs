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
    /// MiniCPM5-2B Q4_K_M (~1.5 GB) — OpenBMB, Apache-2.0 (2026-09-07 release)
    ///
    /// 2.6B dense reasoning model、42 layer LlamaForCausalLM base + ChatML
    /// tokenizer + GQA (16:2 = 8x KV compression)、131K context iGPU /
    /// low-VRAM 環境の最軽量選択肢 (Qwen35_4B ~2GB より小さく iGPU prefill
    /// latency も更に軽減)
    ///
    /// ALICE-LLM 側は GGUF metadata の `general.architecture = "llama"` +
    /// `tokenizer.chat_template` に `<|im_start|>` を含むため既存 Llama
    /// arch + Qwen2 chat template auto-detect で無変更動作
    ///
    /// 命名: HF filename `MiniCPM5-2B-Q4_K_M.gguf` を素直に反映 rustc は
    /// `MiniCpm5_2bQ4Km` を提案するが、既存 [`Self::Qwen35_4B`] /
    /// [`Self::Gemma2_27B`] と揃えて全部 uppercase-with-underscore を採用
    #[allow(non_camel_case_types)]
    MiniCPM5_2B_Q4KM,
    /// MiniCPM5-2B Q6_K (~2.0 GB) — bartowski quant (imatrix quality boost)
    ///
    /// 品質/速度バランス OpenBMB 公式は Q4_K_M / Q8_0 のみ提供のため
    /// 中間 quant は `bartowski/MiniCPM5-2B-GGUF` 由来 (imatrix quant で
    /// 低 bit 品質向上効果)
    #[allow(non_camel_case_types)]
    MiniCPM5_2B_Q6K,
    /// MiniCPM5-2B Q8_0 (~2.5 GB) — OpenBMB, Apache-2.0
    ///
    /// 最高精度 (near-fp16)、精度 sensitive な検証用 iGPU で許容できる
    /// max quant (F16 は ~4.7GB で iGPU prefill が過負荷になる想定)
    #[allow(non_camel_case_types)]
    MiniCPM5_2B_Q8_0,
}

impl ModelChoice {
    /// 全 variant を UI dropdown 用に列挙
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::Qwen35_4B,
            Self::Gemma2_27B,
            Self::Bonsai27B,
            Self::MiniCPM5_2B_Q4KM,
            Self::MiniCPM5_2B_Q6K,
            Self::MiniCPM5_2B_Q8_0,
        ]
    }

    /// OpenAI 互換 API の `model` field に載せる ID (sidecar は無視するが log に出る)
    #[must_use]
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::Qwen35_4B => "qwen3.5-4b-q4_k_m",
            Self::Gemma2_27B => "gemma-2-27b-it-q3_k_l",
            Self::Bonsai27B => "bonsai-27b-q1_0",
            Self::MiniCPM5_2B_Q4KM => "minicpm5-2b-q4_k_m",
            Self::MiniCPM5_2B_Q6K => "minicpm5-2b-q6_k",
            Self::MiniCPM5_2B_Q8_0 => "minicpm5-2b-q8_0",
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
            Self::MiniCPM5_2B_Q4KM => "MiniCPM 5-2B (Q4_K_M, ~1.5 GB, iGPU 推奨)",
            Self::MiniCPM5_2B_Q6K => "MiniCPM 5-2B (Q6_K, ~2.0 GB, balanced)",
            Self::MiniCPM5_2B_Q8_0 => "MiniCPM 5-2B (Q8_0, ~2.5 GB, 高精度)",
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
            Self::MiniCPM5_2B_Q4KM => "MiniCPM5-2B-Q4_K_M.gguf",
            Self::MiniCPM5_2B_Q6K => "MiniCPM5-2B-Q6_K.gguf",
            Self::MiniCPM5_2B_Q8_0 => "MiniCPM5-2B-Q8_0.gguf",
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
            // 公式 Q4_K_M / Q8_0 は OpenBMB (Apache-2.0)、Q6_K は公式に無い
            // ため bartowski quant を採用 (imatrix quality boost あり)
            Self::MiniCPM5_2B_Q4KM => ("openbmb/MiniCPM5-2B-GGUF", "MiniCPM5-2B-Q4_K_M.gguf"),
            Self::MiniCPM5_2B_Q6K => ("bartowski/MiniCPM5-2B-GGUF", "MiniCPM5-2B-Q6_K.gguf"),
            Self::MiniCPM5_2B_Q8_0 => ("openbmb/MiniCPM5-2B-GGUF", "MiniCPM5-2B-Q8_0.gguf"),
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
    use std::collections::HashSet;

    #[test]
    fn default_is_qwen() {
        assert_eq!(ModelChoice::default(), ModelChoice::Qwen35_4B);
    }

    #[test]
    fn all_contains_six() {
        assert_eq!(ModelChoice::all().len(), 6);
        assert!(ModelChoice::all().contains(&ModelChoice::Qwen35_4B));
        assert!(ModelChoice::all().contains(&ModelChoice::Gemma2_27B));
        assert!(ModelChoice::all().contains(&ModelChoice::Bonsai27B));
        assert!(ModelChoice::all().contains(&ModelChoice::MiniCPM5_2B_Q4KM));
        assert!(ModelChoice::all().contains(&ModelChoice::MiniCPM5_2B_Q6K));
        assert!(ModelChoice::all().contains(&ModelChoice::MiniCPM5_2B_Q8_0));
    }

    #[test]
    fn requires_manual_placement_is_bonsai_only() {
        assert!(!ModelChoice::Qwen35_4B.requires_manual_placement());
        assert!(!ModelChoice::Gemma2_27B.requires_manual_placement());
        assert!(ModelChoice::Bonsai27B.requires_manual_placement());
        // MiniCPM5 3 quant は openbmb / bartowski の public HF repo からの
        // 自動 DL が可能で、manual placement は不要
        assert!(!ModelChoice::MiniCPM5_2B_Q4KM.requires_manual_placement());
        assert!(!ModelChoice::MiniCPM5_2B_Q6K.requires_manual_placement());
        assert!(!ModelChoice::MiniCPM5_2B_Q8_0.requires_manual_placement());
    }

    #[test]
    fn model_ids_are_distinct() {
        let ids: HashSet<&'static str> = ModelChoice::all().iter().map(|m| m.model_id()).collect();
        assert_eq!(
            ids.len(),
            ModelChoice::all().len(),
            "全 variant の model_id が unique"
        );
    }

    #[test]
    fn default_filenames_are_distinct() {
        let names: HashSet<&'static str> = ModelChoice::all()
            .iter()
            .map(|m| m.default_filename())
            .collect();
        assert_eq!(
            names.len(),
            ModelChoice::all().len(),
            "全 variant の default_filename が unique"
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

    #[test]
    fn minicpm5_hf_refs_point_to_public_repos() {
        // Q4_K_M / Q8_0 は OpenBMB 公式、Q6_K は bartowski 公式提供に無い
        // Q6_K の代替として採用
        let (repo, file) = ModelChoice::MiniCPM5_2B_Q4KM.default_hf_ref();
        assert_eq!(repo, "openbmb/MiniCPM5-2B-GGUF");
        assert!(file.ends_with("Q4_K_M.gguf"));

        let (repo, file) = ModelChoice::MiniCPM5_2B_Q6K.default_hf_ref();
        assert_eq!(repo, "bartowski/MiniCPM5-2B-GGUF");
        assert!(file.ends_with("Q6_K.gguf"));

        let (repo, file) = ModelChoice::MiniCPM5_2B_Q8_0.default_hf_ref();
        assert_eq!(repo, "openbmb/MiniCPM5-2B-GGUF");
        assert!(file.ends_with("Q8_0.gguf"));
    }
}
