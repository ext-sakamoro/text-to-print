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

    // R3 tab reorder (2026-08-23): templates / customizer are primary,
    // LLM natural language input is marked Experimental
    pub fn templates_section(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "テンプレート (推奨) — クリックで即生成、LLM 経由なし",
            Lang::En => "Templates (recommended) — instant generation, no LLM",
        }
    }
    pub fn customizer_section(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "カスタマイザー (推奨) — サイズ指定して生成、LLM 経由なし",
            Lang::En => "Customizer (recommended) — parametric generation, no LLM",
        }
    }
    pub fn experimental_llm_header(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🧪 実験機能: LLM 自然言語入力 (単純形状のみ推奨)",
            Lang::En => "🧪 Experimental: LLM natural-language input (simple shapes only)",
        }
    }
    pub fn experimental_llm_hint(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "現在のローカル LLM (Qwen 3-4B) は複合形状 (マグカップ / 花瓶 等) を正しく生成できません 複雑な形状は上の テンプレート / カスタマイザー、または 設定 → BYO LLM で Claude / GPT / Gemini 等の高性能 API を接続してください"
            }
            Lang::En => {
                "The default local LLM (Qwen 3-4B) cannot compose complex shapes (mugs, vases, etc.) reliably For complex shapes use Templates / Customizer above, or connect a high-capacity API (Claude / GPT / Gemini) via Settings → BYO LLM"
            }
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

    // ─── about.rs ───────────────────────────────────────────────
    pub fn about_heading(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "About text-to-print",
            Lang::En => "About text-to-print",
        }
    }
    pub fn about_beta_warning(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "BETA バージョンのため、text-to-print のリポジトリは Private となっています"
            }
            Lang::En => "BETA period: the text-to-print repository is Private",
        }
    }
    pub fn about_description(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "text-to-print は自然言語プロンプトから LOL DSL を生成し、Bambu Lab 3MF を作成する スタンドアローン desktop アプリケーションです"
            }
            Lang::En => {
                "text-to-print is a standalone desktop app that generates LOL DSL from natural language prompts and produces Bambu Lab 3MF files"
            }
        }
    }
    pub fn about_version(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Version",
            Lang::En => "Version",
        }
    }
    pub fn about_support_header(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Support the project",
            Lang::En => "Support the project",
        }
    }
    pub fn about_support_body(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "text-to-print は OSS です Free tier で恒久的に利用できます 開発を応援したい 場合は Ko-fi で少額サポートを受け付けています (任意)"
            }
            Lang::En => {
                "text-to-print is OSS with a permanent Free tier If you want to support development, small tips are welcome on Ko-fi (optional)"
            }
        }
    }
    pub fn about_support_kofi(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "☕ Support on Ko-fi",
            Lang::En => "☕ Support on Ko-fi",
        }
    }
    pub fn about_credits(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Credits",
            Lang::En => "Credits",
        }
    }
    pub fn about_credits_author(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Author: Moroya Sakamoto <sakamoro@alicelaw.net>",
            Lang::En => "Author: Moroya Sakamoto <sakamoro@alicelaw.net>",
        }
    }
    pub fn about_credits_license(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "License: MIT",
            Lang::En => "License: MIT",
        }
    }
    pub fn about_credits_repo(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Repository: https://github.com/ext-sakamoto/text-to-print",
            Lang::En => "Repository: https://github.com/ext-sakamoto/text-to-print",
        }
    }
    pub fn about_credits_deps(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Third-party OSS:",
            Lang::En => "Third-party OSS:",
        }
    }

    // ─── viewer.rs ──────────────────────────────────────────────
    pub fn generation_failed(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "生成に失敗しました",
            Lang::En => "Generation failed",
        }
    }
    pub fn mesh_preview_label(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Mesh preview",
            Lang::En => "Mesh preview",
        }
    }

    // ─── history.rs ─────────────────────────────────────────────
    pub fn published_tag(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "(公開)",
            Lang::En => "(published)",
        }
    }

    // ─── share_confirm.rs ───────────────────────────────────────
    pub fn share_confirm_title(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Gallery に公開しますか?",
            Lang::En => "Publish to Gallery?",
        }
    }
    pub fn share_confirm_body(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "この生成物 (LOL DSL + prompt + 品質シグナル) を Gallery に公開しますか?",
            Lang::En => "Publish this creation (LOL DSL + prompt + quality signals) to Gallery?",
        }
    }
    pub fn share_confirm_visibility(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "公開すると他の user から fork / 参考にされる可能性があります",
            Lang::En => "Once published, other users may fork or reference your creation",
        }
    }
    pub fn share_confirm_audit(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "公開しなくても local audit ログには残ります (Settings から確認可)",
            Lang::En => "Local audit log records the creation regardless (viewable in Settings)",
        }
    }
    pub fn share_confirm_publish_once(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "今回だけ公開",
            Lang::En => "Publish this time",
        }
    }
    pub fn share_confirm_skip(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "公開しない",
            Lang::En => "Skip",
        }
    }
    pub fn share_confirm_always(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "毎回自動公開 (以降 dialog 出ない)",
            Lang::En => "Auto-publish always (no more dialogs)",
        }
    }
    pub fn share_confirm_always_hover(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "Settings > プロフィール からいつでも off に戻せます auto 中も Paid tier に変更すれば自動で upload 停止"
            }
            Lang::En => {
                "Toggle off any time from Settings > Profile Switching to Paid tier also stops auto-upload"
            }
        }
    }

    // ─── gallery.rs ─────────────────────────────────────────────
    pub fn gallery_heading(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Gallery",
            Lang::En => "Gallery",
        }
    }
    pub fn gallery_public_count(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "公開中 3D モデル",
            Lang::En => "Published 3D models",
        }
    }
    pub fn gallery_items_suffix(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "件",
            Lang::En => "items",
        }
    }
    pub fn gallery_fetch_failed(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "取得失敗",
            Lang::En => "Fetch failed",
        }
    }
    pub fn gallery_loading(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "取得中...",
            Lang::En => "Loading...",
        }
    }
    pub fn gallery_refresh_button(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "↻ 更新",
            Lang::En => "↻ Refresh",
        }
    }
    pub fn gallery_empty(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "まだ公開されている 3D モデルがありません",
            Lang::En => "No 3D models have been published yet",
        }
    }
    pub fn gallery_empty_hint(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "公開された生成物がここに表示されます",
            Lang::En => "Published creations will appear here",
        }
    }
    pub fn gallery_network_error(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "network に接続できないか、Gallery サーバーが応答していません",
            Lang::En => "Cannot connect to the network, or Gallery server is not responding",
        }
    }
    pub fn gallery_network_hint(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "Settings > プロフィール から DID を確認、ネットワーク復帰後に再試行してください"
            }
            Lang::En => "Check your DID in Settings > Profile and retry after network is back",
        }
    }
    pub fn gallery_select_prompt(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "3D モデルを選択してください",
            Lang::En => "Select a 3D model",
        }
    }
    pub fn gallery_detail_heading(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "詳細",
            Lang::En => "Details",
        }
    }
    pub fn gallery_show_preview(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "3D プレビューで表示",
            Lang::En => "Show in 3D preview",
        }
    }
    pub fn gallery_edit_and_regen(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "✏️ 編集して再生成",
            Lang::En => "✏️ Edit and regenerate",
        }
    }
    pub fn gallery_edit_and_regen_hover(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "この LOL DSL を生成 tab の実験機能欄に流し込みます 編集後に「生成 (LLM)」で再生成"
            }
            Lang::En => {
                "Load this LOL DSL into the experimental section of the Generate tab Edit and click 'Generate (LLM)' to regenerate"
            }
        }
    }
    pub fn gallery_delete_own(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🗑 削除 (自分の post)",
            Lang::En => "🗑 Delete (own post)",
        }
    }
    pub fn gallery_fork_publish(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "フォークして公開",
            Lang::En => "Fork and publish",
        }
    }
    pub fn gallery_source_label(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "LOL ソース:",
            Lang::En => "LOL source:",
        }
    }
    pub fn gallery_id_label(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ID",
            Lang::En => "ID",
        }
    }
    pub fn gallery_author_label(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Author",
            Lang::En => "Author",
        }
    }
    pub fn gallery_did_label(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "DID",
            Lang::En => "DID",
        }
    }
    pub fn gallery_created_label(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Created",
            Lang::En => "Created",
        }
    }
    pub fn gallery_fork_prompt(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "フォーク (リミックス):",
            Lang::En => "Fork (remix):",
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
