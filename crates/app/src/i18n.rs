/// 多言語対応 (日本語 / 英語)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Ja,
    En,
}

impl Lang {
    /// システムロケールから自動検出
    pub fn detect() -> Self {
        let locale = sys_locale::get_locale().unwrap_or_default();
        if locale.starts_with("ja") {
            Self::Ja
        } else {
            Self::En
        }
    }

    /// BCP-47 language tag (`ja` / `en`), used for the manifest `prompt_lang`.
    pub fn as_bcp47(self) -> &'static str {
        match self {
            Self::Ja => "ja",
            Self::En => "en",
        }
    }
}

/// UI テキスト
pub struct T;

impl T {
    // Tabs
    pub fn generate(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "生成",
            Lang::En => "Generate",
        }
    }
    pub fn gallery(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ギャラリー",
            Lang::En => "Gallery",
        }
    }
    pub fn history(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "履歴",
            Lang::En => "History",
        }
    }
    pub fn settings(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "設定",
            Lang::En => "Settings",
        }
    }
    pub fn about(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "情報",
            Lang::En => "About",
        }
    }

    // Prompt
    pub fn text_to_3d(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Text to 3D",
            Lang::En => "Text to 3D",
        }
    }
    pub fn input_description(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "3D モデルの説明を入力:",
            Lang::En => "Describe your 3D model:",
        }
    }
    pub fn generate_button(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "生成",
            Lang::En => "Generate",
        }
    }
    pub fn generating(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "生成中...",
            Lang::En => "Generating...",
        }
    }
    pub fn generation_complete(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "生成完了",
            Lang::En => "Generation complete",
        }
    }
    pub fn daily_usage(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "本日の生成:",
            Lang::En => "Today:",
        }
    }
    pub fn daily_limit_reached(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "本日の生成上限に達しました",
            Lang::En => "Daily generation limit reached",
        }
    }
    pub fn download_requires_plan(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ダウンロードには General 以上のプランが必要です",
            Lang::En => "Download requires General plan or above",
        }
    }

    // Viewer
    pub fn enter_prompt(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "テキストを入力して「生成」を押してください",
            Lang::En => "Enter text and press Generate",
        }
    }

    // History
    pub fn generation_history(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "生成履歴",
            Lang::En => "Generation History",
        }
    }
    pub fn no_history(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "まだ生成履歴がありません",
            Lang::En => "No generation history yet",
        }
    }
    pub fn refresh(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "更新",
            Lang::En => "Refresh",
        }
    }

    // Gallery
    pub fn no_public_sdf(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "まだ公開されている 3D モデルがありません",
            Lang::En => "No public models yet",
        }
    }
    pub fn public_sdf_on_network(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "P2P ネットワーク上の公開 3D モデル",
            Lang::En => "Public models on P2P network",
        }
    }
    pub fn fork_remix(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "フォーク (リミックス):",
            Lang::En => "Fork (remix):",
        }
    }
    pub fn fork_and_publish(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "フォークして公開",
            Lang::En => "Fork & publish",
        }
    }
    pub fn select_sdf(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "3D モデルを選択してください",
            Lang::En => "Select a model",
        }
    }
    pub fn detail(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "詳細",
            Lang::En => "Details",
        }
    }

    // Settings
    pub fn current_tier(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "現在のティア:",
            Lang::En => "Current tier:",
        }
    }
    pub fn license_key(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ライセンスキー:",
            Lang::En => "License key:",
        }
    }
    pub fn apply_license(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ライセンスを適用",
            Lang::En => "Apply license",
        }
    }

    // Model downloader
    pub fn model_downloading(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "LLM モデルをダウンロード中...",
            Lang::En => "Downloading LLM model...",
        }
    }
    pub fn model_ready(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "モデル準備完了",
            Lang::En => "Model ready",
        }
    }

    // Vertices / triangles
    pub fn vertices(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "頂点:",
            Lang::En => "Vertices:",
        }
    }
    pub fn triangles(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "三角形:",
            Lang::En => "Triangles:",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_keys_have_both_languages() {
        // タブ
        assert!(!T::generate(Lang::Ja).is_empty());
        assert!(!T::generate(Lang::En).is_empty());
        assert!(!T::gallery(Lang::Ja).is_empty());
        assert!(!T::gallery(Lang::En).is_empty());
        assert!(!T::history(Lang::Ja).is_empty());
        assert!(!T::history(Lang::En).is_empty());
        assert!(!T::settings(Lang::Ja).is_empty());
        assert!(!T::settings(Lang::En).is_empty());
    }

    #[test]
    fn ja_contains_japanese() {
        assert!(T::generating(Lang::Ja).contains('中'));
        assert!(T::generation_history(Lang::Ja).contains('履'));
    }

    #[test]
    fn en_contains_ascii() {
        assert!(
            T::generating(Lang::En)
                .chars()
                .all(|c| c.is_ascii() || c == '.')
        );
    }

    #[test]
    fn detect_returns_valid_lang() {
        let lang = Lang::detect();
        assert!(lang == Lang::Ja || lang == Lang::En);
    }
}
