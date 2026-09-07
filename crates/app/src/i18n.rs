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

    // ─── settings.rs: plan / license ────────────────────────────
    pub fn settings_current_plan(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "現在のプラン:",
            Lang::En => "Current plan:",
        }
    }
    pub fn settings_tier_free_label(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Free (LoRA share あり)",
            Lang::En => "Free (LoRA share enabled)",
        }
    }
    pub fn settings_beta_plan_gated(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "BETA バージョンのためプランを選択することができません",
            Lang::En => "Plan selection is disabled during BETA",
        }
    }
    pub fn settings_pro_description(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Pro プランは無制限生成 + 完全 offline (LoRA 共有 OFF 強制)",
            Lang::En => "Pro plan: unlimited generation + fully offline (LoRA share forced off)",
        }
    }
    pub fn settings_pro_coming_soon(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Pro subscription is coming in v0.2.0 (Beta では unavailable)",
            Lang::En => "Pro subscription is coming in v0.2.0 (unavailable during BETA)",
        }
    }
    pub fn settings_pro_planned(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "計画: 個人向け Pro プラン ¥3,000/月 or ¥30,000/年 (完全 offline + 無制限生成)"
            }
            Lang::En => {
                "Planned: personal Pro plan ¥3,000/mo or ¥30,000/yr (fully offline + unlimited generation)"
            }
        }
    }
    pub fn settings_buy_monthly(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Buy Monthly ¥3,000/月",
            Lang::En => "Buy Monthly ¥3,000/mo",
        }
    }
    pub fn settings_buy_yearly(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Buy Yearly ¥30,000/年 (-17%)",
            Lang::En => "Buy Yearly ¥30,000/yr (-17%)",
        }
    }
    pub fn settings_email_invalid(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "有効な email 形式で入力してください",
            Lang::En => "Please enter a valid email",
        }
    }
    pub fn settings_enterprise_prompt(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "複数ユーザー / 商用 / カスタム機能:",
            Lang::En => "Multi-user / commercial / custom features:",
        }
    }
    pub fn settings_contact_button(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "問合わせ",
            Lang::En => "Contact",
        }
    }
    pub fn settings_mailer_launch_fail(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "メーラー起動失敗",
            Lang::En => "Failed to launch mail client",
        }
    }
    pub fn settings_license_input_heading(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ライセンスキー入力",
            Lang::En => "Enter license key",
        }
    }
    pub fn settings_license_input_hint(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Stripe 決済後に email で届いたキーを貼付してください",
            Lang::En => "Paste the key you received by email after Stripe payment",
        }
    }
    pub fn settings_license_clear(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ライセンスをクリア (Free に戻す)",
            Lang::En => "Clear license (revert to Free)",
        }
    }

    // ─── settings.rs: LLM backend ────────────────────────────────
    pub fn settings_embedded_status(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Embedded 状態:",
            Lang::En => "Embedded status:",
        }
    }
    pub fn settings_embedded_hover(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "Embedded は alice-llm を rlib 直リンクで実行します 初回選択時は GGUF ロードに ~30 秒 model DL 完了までは Loading 状態 生成 request は Ready 前は Sidecar にフォールバックします"
            }
            Lang::En => {
                "Embedded runs alice-llm as a linked rlib First-time selection takes ~30s for GGUF load Loading state until model DL completes Generation requests fall back to Sidecar until Ready"
            }
        }
    }
    pub fn settings_current(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "現在:",
            Lang::En => "Current:",
        }
    }
    pub fn settings_execution_mode_hover(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "CPU: Llama3Model 直呼び (mmap dequant on demand) GPU: wgpu backend (Metal / Vulkan / DX12) 経由 GpuModel 切替時は Embedded backend を再ロードします adapter 不在時は Error → 手動で CPU に戻して下さい"
            }
            Lang::En => {
                "CPU: direct Llama3Model call (mmap dequant on demand) GPU: via wgpu backend (Metal / Vulkan / DX12) GpuModel switch reloads Embedded backend Fall back to CPU manually if adapter is missing (Error state)"
            }
        }
    }
    pub fn settings_custom_gguf_heading(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Custom GGUF (Model 選択より優先)",
            Lang::En => "Custom GGUF (overrides Model selection)",
        }
    }
    pub fn settings_custom_gguf_unset(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "(未設定、上の Model dropdown を使用)",
            Lang::En => "(unset, using Model dropdown above)",
        }
    }
    pub fn settings_gguf_select_button(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "GGUF ファイル選択",
            Lang::En => "Select GGUF file",
        }
    }
    pub fn settings_gguf_select_hover(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "HF DL を skip して指定 path から直接ロード",
            Lang::En => "Skip HF download and load directly from the specified path",
        }
    }
    pub fn settings_gguf_clear_hover(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "override 解除、上の Model dropdown に戻す",
            Lang::En => "Clear override, revert to Model dropdown above",
        }
    }
    pub fn settings_manual_placement_prefix(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "手動配置要: HF repo 非公開のため",
            Lang::En => "Manual placement required (HF repo private):",
        }
    }
    pub fn settings_manual_placement_suffix(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "を models_dir に配置",
            Lang::En => "into models_dir",
        }
    }
    pub fn settings_bonsai_manual_hover(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "PrismML fork Q1_0 (128-element binary ternary) の Bonsai 27B は現在 HF 非公開 GGUF ファイルを手動で models/bonsai-27b-q1_0.gguf に配置すると Embedded backend が拾います (Stage 3-C.9 の model_exists 経路)"
            }
            Lang::En => {
                "Bonsai 27B (PrismML fork Q1_0, 128-element binary ternary) is currently private on HF Place the GGUF file at models/bonsai-27b-q1_0.gguf and the Embedded backend will pick it up (Stage 3-C.9 model_exists path)"
            }
        }
    }
    pub fn settings_grammar_checkbox(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "LOL DSL grammar 強制 (GBNF)",
            Lang::En => "Enforce LOL DSL grammar (GBNF)",
        }
    }
    pub fn settings_grammar_hover(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "オンにすると生成 request に text_to_print_llm::grammar_lol::LOL_GBNF (253 行) を付随して送信し、alice-llm 側で mask_logits_by_grammar を毎 token 適用します 出力は parse_lol でパース保証 (シンタックス誤り 0) オフにすると free-form output (デバッグ / 別 grammar 検証時用)"
            }
            Lang::En => {
                "When on, generation requests bundle text_to_print_llm::grammar_lol::LOL_GBNF (253 lines) and alice-llm applies mask_logits_by_grammar per token Output is guaranteed parseable by parse_lol (zero syntax errors) When off, free-form output (for debugging or alternate grammar tests)"
            }
        }
    }

    // ─── settings.rs: profile / share ────────────────────────────
    pub fn settings_profile_section(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "プロフィール (Gallery 表示名)",
            Lang::En => "Profile (Gallery display name)",
        }
    }
    pub fn settings_nickname_hint(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Gallery で表示される名前 (空欄なら DID)",
            Lang::En => "Name displayed in Gallery (DID if empty)",
        }
    }
    pub fn settings_nickname_hover(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "Gallery タブで他人が見る表示名 空欄のままなら DID (did:key:...) の先頭 12 char + 末尾 6 char が表示されます 変更しても過去に公開した post には反映されません (最新の nickname は次回公開時から反映)"
            }
            Lang::En => {
                "Display name others see in the Gallery tab If empty, first 12 + last 6 chars of DID (did:key:...) are shown Changes do not propagate to previously published posts (latest nickname applies from next publish)"
            }
        }
    }
    pub fn settings_lora_share_checkbox(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "LoRA 学習データ提供に協力する",
            Lang::En => "Contribute to LoRA training data",
        }
    }
    pub fn settings_lora_share_hover(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "オンにすると生成した LOL DSL + 品質シグナル (prompt / LOL 原文 / mesh SHA-256 / retry_count / safety_violations 等) が ALICE-LOL LoRA 学習セットに送信対象化されます Free tier default オン、Paid tier は完全 offline\n\n送信されないもの: Apple/Google/Microsoft アカウント ID / machine ID / file path / license key / crash report / P2P share pending キュー\n\n詳細: docs/SHARE.md"
            }
            Lang::En => {
                "When on, generated LOL DSL + quality signals (prompt / LOL source / mesh SHA-256 / retry_count / safety_violations, etc.) become upload candidates for the ALICE-LOL LoRA training set Free tier default on, Paid tier is fully offline\n\nNever sent: Apple/Google/Microsoft account ID / machine ID / file path / license key / crash report / P2P share pending queue\n\nDetails: docs/SHARE.md"
            }
        }
    }
    pub fn settings_share_status_active(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "現在: 共有中 (LoRA 品質向上に貢献)",
            Lang::En => "Status: sharing (contributing to LoRA quality)",
        }
    }
    pub fn settings_share_status_paid(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "現在: 有料 tier のため自動 opt-out (アップロードしません)",
            Lang::En => "Status: auto opt-out (Paid tier, no uploads)",
        }
    }
    pub fn settings_share_status_off(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "現在: opt-out (アップロードしません)",
            Lang::En => "Status: opt-out (no uploads)",
        }
    }
    pub fn settings_upload_queue(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "アップロード待ち: 実キュー",
            Lang::En => "Upload queue: pending",
        }
    }
    pub fn settings_upload_queue_dryrun(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "件 / dry-run",
            Lang::En => "items / dry-run",
        }
    }
    pub fn settings_upload_queue_items(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "件",
            Lang::En => "items",
        }
    }
    pub fn settings_auto_publish_checkbox(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "毎回自動公開 (公開確認 dialog を表示しない)",
            Lang::En => "Auto-publish always (no confirm dialog)",
        }
    }
    pub fn settings_auto_publish_hover(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "オフ (default) だと生成完了ごとに Gallery 公開確認 dialog が出ます オンにすると dialog なしで自動公開されます (Free tier で share on の時のみ、Paid tier は常時 upload しない)"
            }
            Lang::En => {
                "Off (default): Gallery confirm dialog appears after each generation On: auto-publish without dialog (Free tier with share on only; Paid tier never uploads)"
            }
        }
    }
    pub fn settings_share_details_link(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "詳細な送信内容と opt-out 手順 (docs/SHARE.md)",
            Lang::En => "Detailed data flow and opt-out procedure (docs/SHARE.md)",
        }
    }

    // ─── settings.rs: crash reports ──────────────────────────────
    pub fn settings_crash_checkbox(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "クラッシュ発生時にローカル report を保存する",
            Lang::En => "Save local crash reports on crash",
        }
    }
    pub fn settings_crash_hover(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "オンにするとクラッシュ発生時に crash_reports/{uuid}.json が data_dir に保存されます (現状 upload なし、backend #36 実装後に opt-in で送信予定) オフにするとログのみ"
            }
            Lang::En => {
                "When on, crash_reports/{uuid}.json is saved to data_dir on crash (no upload yet; opt-in send planned after backend #36) Off: log only"
            }
        }
    }
    pub fn settings_crash_saved(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "保存済 report:",
            Lang::En => "Saved reports:",
        }
    }
    pub fn settings_crash_open_folder(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "フォルダを開く",
            Lang::En => "Open folder",
        }
    }

    // ─── settings.rs: usage / limits ─────────────────────────────
    pub fn settings_today_usage(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "本日の使用量:",
            Lang::En => "Today's usage:",
        }
    }
    pub fn settings_times_unit(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "回",
            Lang::En => "times",
        }
    }
    pub fn settings_limit_unlimited(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "生成上限: 無制限",
            Lang::En => "Generation limit: unlimited",
        }
    }
    pub fn settings_limit_daily_prefix(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "生成上限:",
            Lang::En => "Generation limit:",
        }
    }
    pub fn settings_limit_per_day(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "回/日",
            Lang::En => "times/day",
        }
    }

    // ─── settings.rs: advanced / network ─────────────────────────
    pub fn settings_advanced_section(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Advanced (次回起動時に反映)",
            Lang::En => "Advanced (applied on next launch)",
        }
    }
    pub fn settings_endpoint_hint(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "空 = 既定 (Cloudflare)",
            Lang::En => "Empty = default (Cloudflare)",
        }
    }
    pub fn settings_endpoint_hover(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "空欄なら https://text-to-print.alicelaw.net/api/presets を使用",
            Lang::En => "Empty uses https://text-to-print.alicelaw.net/api/presets",
        }
    }
    pub fn settings_preset_sync_checkbox(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "起動時に preset library を同期する",
            Lang::En => "Sync preset library at startup",
        }
    }
    pub fn settings_port_default_hint(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "(既定 8000、使用中なら +1 で自動 fallback)",
            Lang::En => "(default 8000, auto-fallback +1 if in use)",
        }
    }
    pub fn settings_port_save_error(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "port save 失敗",
            Lang::En => "Port save failed",
        }
    }
    pub fn settings_port_range_error(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "port は 1024-65535 の整数",
            Lang::En => "Port must be an integer 1024-65535",
        }
    }
    pub fn settings_endpoint_save_error(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "endpoint save 失敗",
            Lang::En => "Endpoint save failed",
        }
    }
    pub fn settings_save_next_launch(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "保存完了 次回起動時に反映",
            Lang::En => "Saved (takes effect on next launch)",
        }
    }

    // ─── settings.rs: BYO LLM ────────────────────────────────────
    pub fn settings_byo_intro(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "リモート LLM API 設定 (OpenAI / Anthropic / Google / Ollama 等)",
            Lang::En => "Remote LLM API configuration (OpenAI / Anthropic / Google / Ollama, etc.)",
        }
    }
    pub fn settings_byo_not_selected(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "現在の Inference backend は BYO LLM ではありません 上の picker で 'BYO LLM' を選択すると有効"
            }
            Lang::En => {
                "Current inference backend is not BYO LLM Select 'BYO LLM' in the picker above to enable"
            }
        }
    }
    pub fn settings_byo_legend(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "★ = アクティブ (生成に使用中) / ● = 保存済 (未アクティブ)",
            Lang::En => "★ = active (used for generation) / ● = saved (not active)",
        }
    }
    pub fn settings_byo_free_hint(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "💡 無料で試すなら Google (Gemini 2.5 Flash) 推奨",
            Lang::En => "💡 For free testing, Google (Gemini 2.5 Flash) is recommended",
        }
    }
    pub fn settings_byo_select_provider(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "上のボタンで provider を選択",
            Lang::En => "Select a provider using the buttons above",
        }
    }
    pub fn settings_byo_model_hint(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "例: OpenAI: gpt-5 / o1 / gpt-4o-mini",
            Lang::En => "e.g. OpenAI: gpt-5 / o1 / gpt-4o-mini",
        }
    }
    pub fn settings_byo_max_tokens_hint(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "(既定 256、大きくすると 1 生成コスト増)",
            Lang::En => "(default 256, larger = higher cost per generation)",
        }
    }
    pub fn settings_byo_temperature_hint(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "(0.0-2.0、既定 0.7)",
            Lang::En => "(0.0-2.0, default 0.7)",
        }
    }
    pub fn settings_byo_reasoning_unset(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "(未指定)",
            Lang::En => "(unset)",
        }
    }
    pub fn settings_byo_reasoning_hover(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "OpenAI GPT-5/o-series は 'minimal' 推奨 (silent thinking 課金抑制)",
            Lang::En => {
                "OpenAI GPT-5/o-series: 'minimal' recommended (limits silent-thinking billing)"
            }
        }
    }
    pub fn settings_byo_api_key_hint(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "保存済でも空欄表示 新規入力で上書き",
            Lang::En => "Shown blank even if saved New input overwrites",
        }
    }
    pub fn settings_byo_keychain_saved(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "✅ Keychain に保存済 (起動毎に empty 表示は正常動作)",
            Lang::En => "✅ Saved to Keychain (empty display at each launch is normal)",
        }
    }
    pub fn settings_byo_keychain_unset(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "⚠️ 未保存 API key を入力して「保存」を押してください",
            Lang::En => "⚠️ Not saved Enter API key and press Save",
        }
    }
    pub fn settings_byo_keychain_error(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "❌ Keychain 読出エラー",
            Lang::En => "❌ Keychain read error",
        }
    }
    pub fn settings_save_button(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "保存",
            Lang::En => "Save",
        }
    }
    pub fn settings_byo_test_button(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "テスト送信 (~16 tokens)",
            Lang::En => "Test send (~16 tokens)",
        }
    }
    pub fn settings_byo_test_hover(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "フォーム内容で 1 回だけ生成 (最大 30 秒 UI ブロック)",
            Lang::En => "Generate once with the form's contents (may block UI up to 30s)",
        }
    }
    pub fn settings_delete_button(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "削除",
            Lang::En => "Delete",
        }
    }
    pub fn settings_byo_activate_button(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "この provider をアクティブ化",
            Lang::En => "Activate this provider",
        }
    }
    pub fn settings_byo_activate_hover(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "生成時にこの provider を使うよう切替",
            Lang::En => "Switch to use this provider for generation",
        }
    }
    pub fn settings_byo_cost_estimate_prefix(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "予想コスト: 約",
            Lang::En => "Estimated cost: ~",
        }
    }
    pub fn settings_byo_cost_estimate_suffix(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "/ 生成 (system_prompt ~4K in + ~500 out トークン想定、rate は 2026-08-23 時点、実際は provider の pricing page で確認)"
            }
            Lang::En => {
                "/ generation (assumes ~4K in + ~500 out tokens, rate as of 2026-08-23, verify on provider's pricing page)"
            }
        }
    }
    pub fn settings_byo_cost_unknown(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "予想コスト: rate table に model なし (Custom / 独自 model 使用時)",
            Lang::En => "Estimated cost: model not in rate table (Custom / user-provided model)",
        }
    }

    // ─── settings.rs: form error / success messages ──────────────
    pub fn settings_max_tokens_positive_int(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "max_tokens は正の整数",
            Lang::En => "max_tokens must be a positive integer",
        }
    }
    pub fn settings_temperature_range(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "temperature は 0.0-2.0 の実数",
            Lang::En => "temperature must be a real number 0.0-2.0",
        }
    }
    pub fn settings_endpoint_empty(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "endpoint が空",
            Lang::En => "endpoint is empty",
        }
    }
    pub fn settings_model_empty(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "model が空",
            Lang::En => "model is empty",
        }
    }
    pub fn settings_keychain_save_fail(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Keychain 保存失敗",
            Lang::En => "Keychain save failed",
        }
    }
    pub fn settings_api_key_missing(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "API key が未入力 (Keychain にも保存なし)",
            Lang::En => "API key not entered (nor saved in Keychain)",
        }
    }
    pub fn settings_keychain_read_fail(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Keychain 読出失敗",
            Lang::En => "Keychain read failed",
        }
    }
    pub fn settings_db_save_fail(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "DB 保存失敗",
            Lang::En => "DB save failed",
        }
    }
    pub fn settings_saved_ok(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "保存完了",
            Lang::En => "Saved",
        }
    }
    pub fn settings_db_delete_fail(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "DB 削除失敗",
            Lang::En => "DB delete failed",
        }
    }
    pub fn settings_keychain_delete_fail(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Keychain 削除失敗",
            Lang::En => "Keychain delete failed",
        }
    }
    pub fn settings_deleted_ok(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "削除完了",
            Lang::En => "Deleted",
        }
    }
    pub fn settings_active_save_fail(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "active provider 保存失敗",
            Lang::En => "Failed to save active provider",
        }
    }
    pub fn settings_provider_not_saved(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "この provider は未保存 まず '保存' して下さい",
            Lang::En => "This provider is not saved; press 'Save' first",
        }
    }
    pub fn settings_no_api_key_saved(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Keychain に API key なし まず '保存' して下さい",
            Lang::En => "No API key in Keychain; press 'Save' first",
        }
    }
    pub fn settings_activated_ok_suffix(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "をアクティブ化しました",
            Lang::En => "activated",
        }
    }
    pub fn settings_api_key_both_missing(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "API key が form / Keychain のどちらにもなし",
            Lang::En => "API key is missing from both form and Keychain",
        }
    }
    pub fn settings_test_success_prefix(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "成功:",
            Lang::En => "Success:",
        }
    }
    pub fn settings_test_fail_prefix(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "失敗:",
            Lang::En => "Failure:",
        }
    }
    pub fn settings_test_timeout(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "失敗: 30 秒でタイムアウト",
            Lang::En => "Failure: timed out after 30s",
        }
    }
    pub fn settings_license_invalid(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "無効なキー",
            Lang::En => "Invalid key",
        }
    }
    pub fn settings_license_system_missing(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ライセンスシステム未設定",
            Lang::En => "License system not configured",
        }
    }
    pub fn settings_license_verify_fail(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "検証失敗",
            Lang::En => "Verification failed",
        }
    }
    pub fn settings_license_reverted_free(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Free に戻しました",
            Lang::En => "Reverted to Free",
        }
    }
    pub fn settings_db_update_fail(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "DB 更新失敗",
            Lang::En => "DB update failed",
        }
    }
    pub fn settings_plan_updated_prefix(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "プランに更新しました (有効期限",
            Lang::En => "Plan updated (expires",
        }
    }
    pub fn settings_email_invalid_short(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "email が未入力または不正です",
            Lang::En => "Email is missing or invalid",
        }
    }
    pub fn settings_checkout_fetching(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "の checkout URL を取得中...",
            Lang::En => "fetching checkout URL...",
        }
    }
    pub fn settings_response_parse_fail(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "レスポンス parse 失敗",
            Lang::En => "Response parse failed",
        }
    }
    pub fn settings_browser_launch_fail(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "browser 起動失敗",
            Lang::En => "Browser launch failed",
        }
    }

    // ─── settings.rs: extra section headers / labels (Phase 2) ────
    pub fn settings_heading(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "設定",
            Lang::En => "Settings",
        }
    }
    pub fn settings_section_license(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ライセンス / サブスクリプション",
            Lang::En => "License / Subscription",
        }
    }
    pub fn settings_section_llm(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "LLM",
            Lang::En => "LLM",
        }
    }
    pub fn settings_section_byo(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "BYO LLM (OpenAI / Claude / Gemini / Ollama)",
            Lang::En => "BYO LLM (OpenAI / Claude / Gemini / Ollama)",
        }
    }
    pub fn settings_section_lora_share(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "LoRA 共有",
            Lang::En => "LoRA share",
        }
    }
    pub fn settings_section_crash(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "クラッシュレポート",
            Lang::En => "Crash reports",
        }
    }
    pub fn settings_section_network(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ネットワーク",
            Lang::En => "Network",
        }
    }
    pub fn settings_upgrade_pro(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Pro にアップグレード",
            Lang::En => "Upgrade to Pro",
        }
    }
    pub fn settings_enterprise_plan(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Enterprise プラン",
            Lang::En => "Enterprise plan",
        }
    }
    pub fn settings_coming_soon(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "v0.2.0 で提供予定",
            Lang::En => "Coming soon in v0.2.0",
        }
    }
    pub fn settings_email_label(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "メール:",
            Lang::En => "Email:",
        }
    }
    pub fn settings_inference_backend_label(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "推論バックエンド:",
            Lang::En => "Inference backend:",
        }
    }
    pub fn settings_execution_mode_label(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "実行モード:",
            Lang::En => "Execution mode:",
        }
    }
    pub fn settings_gguf_select_hover_extra(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "HF DL を skip して指定 path から直接ロード qwen2.5-14b-instruct-q4_k_m.gguf 等の大型モデルを user 側で DL / 配置して使用"
            }
            Lang::En => {
                "Skip HF download and load directly from the specified path Use to run large models (e.g. qwen2.5-14b-instruct-q4_k_m.gguf) that the user has downloaded/placed"
            }
        }
    }
    pub fn settings_clear_button(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "クリア",
            Lang::En => "Clear",
        }
    }
    pub fn settings_endpoint_alice_llm(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Endpoint (alice-llm-server):",
            Lang::En => "Endpoint (alice-llm-server):",
        }
    }
    pub fn settings_model_label(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "モデル:",
            Lang::En => "Model:",
        }
    }
    pub fn settings_temperature_label(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Temperature",
            Lang::En => "Temperature",
        }
    }
    pub fn settings_did_label(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "DID",
            Lang::En => "DID",
        }
    }
    pub fn settings_profile_id_label(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Profile ID",
            Lang::En => "Profile ID",
        }
    }
    pub fn settings_preset_endpoint_label(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Preset endpoint:",
            Lang::En => "Preset endpoint:",
        }
    }
    pub fn settings_endpoint_hover_extra(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "空欄なら https://text-to-print.alicelaw.net/api/presets を使用 (self-hosted mirror / proxy 経由時のみ変更)"
            }
            Lang::En => {
                "Empty uses https://text-to-print.alicelaw.net/api/presets (change only when using a self-hosted mirror or proxy)"
            }
        }
    }
    pub fn settings_sidecar_port_label(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Sidecar port:",
            Lang::En => "Sidecar port:",
        }
    }
    pub fn settings_save_network_button(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ネットワーク設定を保存",
            Lang::En => "Save network settings",
        }
    }
    pub fn settings_byo_intro_extra(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "リモート LLM API 設定 (OpenAI / Anthropic / Google / Ollama 等) 保存された provider のうち 1 つを 'アクティブ' として生成に使用します"
            }
            Lang::En => {
                "Remote LLM API configuration (OpenAI / Anthropic / Google / Ollama, etc.) One of the saved providers is used as the 'active' one for generation"
            }
        }
    }
    pub fn settings_provider_preset_label(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Provider preset:",
            Lang::En => "Provider preset:",
        }
    }
    pub fn settings_byo_free_hint_extra(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "💡 無料で試すなら Google (Gemini 2.5 Flash) 推奨 AI Studio (aistudio.google.com/apikey) で API key 取得、無料枠 ~1500 req/day"
            }
            Lang::En => {
                "💡 For free testing, Google (Gemini 2.5 Flash) is recommended Get an API key at AI Studio (aistudio.google.com/apikey); free tier ~1500 req/day"
            }
        }
    }
    pub fn settings_byo_endpoint_label(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Endpoint:",
            Lang::En => "Endpoint:",
        }
    }
    pub fn settings_byo_model_label(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Model:",
            Lang::En => "Model:",
        }
    }
    pub fn settings_byo_model_hint_extra(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "例: OpenAI: gpt-5 / o1 / gpt-4o-mini Anthropic: claude-sonnet-4-5 / claude-opus-4-7 Google: gemini-2.5-pro / gemini-2.5-flash Ollama: qwen2.5-14b-instruct"
            }
            Lang::En => {
                "e.g. OpenAI: gpt-5 / o1 / gpt-4o-mini Anthropic: claude-sonnet-4-5 / claude-opus-4-7 Google: gemini-2.5-pro / gemini-2.5-flash Ollama: qwen2.5-14b-instruct"
            }
        }
    }
    pub fn settings_byo_max_tokens_label(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Max tokens:",
            Lang::En => "Max tokens:",
        }
    }
    pub fn settings_byo_temperature_label(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Temperature:",
            Lang::En => "Temperature:",
        }
    }
    pub fn settings_byo_reasoning_label(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Reasoning effort:",
            Lang::En => "Reasoning effort:",
        }
    }
    pub fn settings_byo_reasoning_hover_extra(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "OpenAI GPT-5/o-series は 'minimal' 推奨 (silent thinking 課金抑制) Google Gemini 2.5 は 'none' 推奨 (silent thinking 抑制) Anthropic / Ollama は空欄で OK"
            }
            Lang::En => {
                "OpenAI GPT-5/o-series: 'minimal' recommended (limits silent-thinking billing) Google Gemini 2.5: 'none' recommended (limits silent thinking) Anthropic / Ollama: leave blank"
            }
        }
    }
    pub fn settings_byo_api_key_label(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "API key:",
            Lang::En => "API key:",
        }
    }
    pub fn settings_byo_cost_unknown_extra(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "予想コスト: rate table に model なし (Custom / 独自 model 使用時) API 課金は provider の pricing page で確認"
            }
            Lang::En => {
                "Estimated cost: model not in rate table (Custom / user-provided model) Check the provider's pricing page for billing rates"
            }
        }
    }
    pub fn settings_http_error(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "HTTP エラー",
            Lang::En => "HTTP error",
        }
    }
    pub fn settings_plan_updated_expires(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "有効期限",
            Lang::En => "expires",
        }
    }
    pub fn settings_plan_updated_updated(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "プランに更新しました",
            Lang::En => "plan updated",
        }
    }
    pub fn settings_paid_ui_disabled_hover(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "v0.2.0 で提供予定",
            Lang::En => "Coming soon in v0.2.0",
        }
    }

    // ─── prompt.rs (Phase 3) ────────────────────────────────────
    pub fn prompt_p000(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "+Z 正面 (front)",
            Lang::En => "+Z front",
        }
    }
    pub fn prompt_p001(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "-Z 背面 (back)",
            Lang::En => "-Z back",
        }
    }
    pub fn prompt_p002(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "+Y 上面 (top)",
            Lang::En => "+Y top",
        }
    }
    pub fn prompt_p003(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "-Y 底面 (bottom)",
            Lang::En => "-Y bottom",
        }
    }
    pub fn prompt_p004(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "+X 右側面 (right)",
            Lang::En => "+X right",
        }
    }
    pub fn prompt_p005(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "-X 左側面 (left)",
            Lang::En => "-X left",
        }
    }
    pub fn prompt_p006(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "本日の生成: {usage} 回 (β 制限なし)",
            Lang::En => "Today: {usage} runs (β no cap)",
        }
    }
    pub fn prompt_p007(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "本日の生成: {} / {} 回",
            Lang::En => "Today: {} / {} runs",
        }
    }
    pub fn prompt_p008(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "3D モデルの説明を入力 (Enter で生成 / Shift+Enter で改行):",
            Lang::En => "Describe your 3D model (Enter to generate / Shift+Enter for newline):",
        }
    }
    pub fn prompt_p009(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "例: 20mm の立方体、上面に直径 5mm の穴",
            Lang::En => "e.g. 20mm cube with a 5mm hole on top",
        }
    }
    pub fn prompt_p010(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "生成 (LLM)",
            Lang::En => "Generate (LLM)",
        }
    }
    pub fn prompt_p011(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "本日の生成上限に達しました",
            Lang::En => "Daily generation limit reached",
        }
    }
    pub fn prompt_p012(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "生成中...",
            Lang::En => "Generating...",
        }
    }
    pub fn prompt_p013(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "生成完了",
            Lang::En => "Generation complete",
        }
    }
    pub fn prompt_p014(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "頂点数: {} / 三角形数: {}",
            Lang::En => "Vertices: {} / Triangles: {}",
        }
    }
    pub fn prompt_p015(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "保存先: {}",
            Lang::En => "Saved to: {}",
        }
    }
    pub fn prompt_p016(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "オーバーハング: {:.1}% ({} / {} 面) 最大壁角 {:.1}°",
            Lang::En => "Overhang: {:.1}% ({} / {} faces) max wall angle {:.1}°",
        }
    }
    pub fn prompt_p017(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "G-code: {} 層 / 推定 {} 分 / フィラメント {:.2} m",
            Lang::En => "G-code: {} layers / est {} min / filament {:.2} m",
        }
    }
    pub fn prompt_p018(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "安全性 ({}): {} / 反り {}",
            Lang::En => "Safety ({}): {} / warp {}",
        }
    }
    pub fn prompt_p019(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "要注意",
            Lang::En => "Warning",
        }
    }
    pub fn prompt_p020(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "LOL ソース",
            Lang::En => "LOL source",
        }
    }
    pub fn prompt_p021(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ダウンロードには General 以上のプランが必要です",
            Lang::En => "Download requires General plan or higher",
        }
    }
    pub fn prompt_p022(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "エラー: {msg}",
            Lang::En => "Error: {msg}",
        }
    }
    pub fn prompt_p023(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "エクスポート失敗: {err}",
            Lang::En => "Export failed: {err}",
        }
    }
    pub fn prompt_p024(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "LLM モデルをダウンロード中...",
            Lang::En => "Downloading LLM model...",
        }
    }
    pub fn prompt_p025(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "モデルDLエラー: {e}",
            Lang::En => "Model download error: {e}",
        }
    }
    pub fn prompt_p026(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "モデル準備中...",
            Lang::En => "Preparing model...",
        }
    }
    pub fn prompt_p027(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "LLM 起動待機中 (モデル準備完了後に自動起動)",
            Lang::En => "LLM waiting to start (auto-starts after model ready)",
        }
    }
    pub fn prompt_p028(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "LLM sidecar を起動中...",
            Lang::En => "Starting LLM sidecar...",
        }
    }
    pub fn prompt_p029(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "LLM sidecar 起動失敗: {msg}",
            Lang::En => "LLM sidecar start failed: {msg}",
        }
    }
    pub fn prompt_p030(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "対処: `cargo install --path ~/ALICE-LLM --features server` で \
                     alice-llm-server を PATH に配置、または Settings の Endpoint に \
                     既存の OpenAI 互換 endpoint (例: Ollama) を指定してください"
            }
            Lang::En => {
                "Action: run `cargo install --path ~/ALICE-LLM --features server` to place alice-llm-server in PATH, or set Settings > Endpoint to an existing OpenAI-compatible endpoint (e.g. Ollama)"
            }
        }
    }
    pub fn prompt_p031(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "保存先",
            Lang::En => "Saved to",
        }
    }
    pub fn prompt_p032(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "{phase_label} 実行中... 経過 {:02}:{:02}",
            Lang::En => "{phase_label} running... elapsed {:02}:{:02}",
        }
    }
    pub fn prompt_p033(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "エクスポート形式:",
            Lang::En => "Export format:",
        }
    }
    pub fn prompt_p034(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "保存...",
            Lang::En => "Save...",
        }
    }
    pub fn prompt_p035(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "4色 export (Bambu AMS 対応)",
            Lang::En => "4-color export (Bambu AMS)",
        }
    }
    pub fn prompt_p036(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "正面画像を palette 化して色ごとに 3MF 分割 (Bambu Lab AMS / Prusa MMU)",
            Lang::En => {
                "Palettize the front image and split into per-color 3MF (Bambu Lab AMS / Prusa MMU)"
            }
        }
    }
    pub fn prompt_p037(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "色数:",
            Lang::En => "Colors:",
        }
    }
    pub fn prompt_p038(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "投影軸:",
            Lang::En => "Projection axis:",
        }
    }
    pub fn prompt_p039(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "正面画像を選択...",
            Lang::En => "Choose front image...",
        }
    }
    pub fn prompt_p040(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "(未選択)",
            Lang::En => "(not selected)",
        }
    }
    pub fn prompt_p041(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "4色 3MF を生成",
            Lang::En => "Generate 4-color 3MF",
        }
    }
    pub fn prompt_p042(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "4色エクスポート失敗: {err}",
            Lang::En => "4-color export failed: {err}",
        }
    }
    pub fn prompt_p043(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "生成完了: {} 色 / {} ファイル",
            Lang::En => "Complete: {} colors / {} files",
        }
    }
    pub fn prompt_p044(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "フォルダを開く",
            Lang::En => "Open folder",
        }
    }
    pub fn prompt_p045(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "画像読み込み失敗: {e}",
            Lang::En => "Image load failed: {e}",
        }
    }
    pub fn prompt_p046(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "{} (未対応)",
            Lang::En => "{} (unsupported)",
        }
    }
    pub fn prompt_p047(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "テンプレート生成失敗: {e}",
            Lang::En => "Template generation failed: {e}",
        }
    }
    pub fn prompt_p048(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "LLM が有効な LOL DSL を生成できませんでした ({clean_err})\n\n\
                                 対処: プロンプトを短く / 具体的に書き直すか、テンプレート / \
                                 カスタマイザーをお使いください (LLM 経路より高速で確実)"
            }
            Lang::En => {
                "The LLM could not produce valid LOL DSL ({clean_err})\n\nAction: shorten or clarify the prompt, or use the Templates / Customizer path (faster and more reliable than the LLM path)"
            }
        }
    }
    pub fn prompt_p049(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "テンプレート (クリックで即生成、LLM 経由しない)",
            Lang::En => "Templates (click to generate instantly, no LLM)",
        }
    }
    pub fn prompt_p050(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "内蔵",
            Lang::En => "bundled",
        }
    }
    pub fn prompt_p051(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "カスタマイザー (サイズ指定して生成、LLM 経由しない)",
            Lang::En => "Customizer (specify size to generate, no LLM)",
        }
    }
    pub fn prompt_p052(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "📦 Gridfinity bin (42mm grid × 7mm 高さ)",
            Lang::En => "📦 Gridfinity bin (42mm grid × 7mm height)",
        }
    }
    pub fn prompt_p053(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "詳細設定 (dividers + 壁厚)",
            Lang::En => "Advanced (dividers + wall thickness)",
        }
    }
    pub fn prompt_p054(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "内部仕切り (dividers) を有効化",
            Lang::En => "Enable inner dividers",
        }
    }
    pub fn prompt_p055(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "壁厚 (mm):",
            Lang::En => "Wall thickness (mm):",
        }
    }
    pub fn prompt_p056(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "底厚 (mm):",
            Lang::En => "Floor thickness (mm):",
        }
    }
    pub fn prompt_p057(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "外形寸法: {ext_x_mm:.1} × {ext_y_mm:.1} × {ext_h_mm:.1}mm",
            Lang::En => "Outer dimensions: {ext_x_mm:.1} × {ext_y_mm:.1} × {ext_h_mm:.1}mm",
        }
    }
    pub fn prompt_p058(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "作成: {label}",
            Lang::En => "Create: {label}",
        }
    }
    pub fn prompt_p059(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🗒 付箋ホルダー (Post-it 3×3 / 3×5 inch 対応)",
            Lang::En => "🗒 Sticky-note holder (Post-it 3×3 / 3×5 inch)",
        }
    }
    pub fn prompt_p060(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "pad 幅 (mm):",
            Lang::En => "pad width (mm):",
        }
    }
    pub fn prompt_p061(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "pad 深さ (mm):",
            Lang::En => "pad depth (mm):",
        }
    }
    pub fn prompt_p062(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "高さ (mm):",
            Lang::En => "Height (mm):",
        }
    }
    pub fn prompt_p063(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "付箋ホルダー {}×{}×{}mm",
            Lang::En => "Sticky-note holder {}×{}×{}mm",
        }
    }
    pub fn prompt_p064(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "💳 名刺ホルダー (JP 91×55 / US 89×51 / EU 85.6×54)",
            Lang::En => "💳 Business-card holder (JP 91×55 / US 89×51 / EU 85.6×54)",
        }
    }
    pub fn prompt_p065(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "card 幅 (mm):",
            Lang::En => "card width (mm):",
        }
    }
    pub fn prompt_p066(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "card 高さ (mm):",
            Lang::En => "card height (mm):",
        }
    }
    pub fn prompt_p067(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "slot 厚 (mm):",
            Lang::En => "slot thickness (mm):",
        }
    }
    pub fn prompt_p068(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "収納枚数目安: 約 {capacity} 枚",
            Lang::En => "Approx capacity: {capacity} cards",
        }
    }
    pub fn prompt_p069(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "名刺ホルダー {}×{}mm ({}枚)",
            Lang::En => "Business-card holder {}×{}mm ({} cards)",
        }
    }
    pub fn prompt_p070(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "✏ ペン立て (single-compartment 円筒)",
            Lang::En => "✏ Pen cup (single-compartment cylinder)",
        }
    }
    pub fn prompt_p071(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "内径 (mm):",
            Lang::En => "Inner diameter (mm):",
        }
    }
    pub fn prompt_p072(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ペン立て Ø{}×{}mm",
            Lang::En => "Pen cup Ø{}×{}mm",
        }
    }
    pub fn prompt_p073(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "外形 Ø{outer_dia:.1}mm (壁厚 2mm)",
            Lang::En => "Outer Ø{outer_dia:.1}mm (2mm wall)",
        }
    }
    pub fn prompt_p074(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "📱 スマホ / タブレット スタンド (L 字 + 上部 slot)",
            Lang::En => "📱 Phone / tablet stand (L-shape + top slot)",
        }
    }
    pub fn prompt_p075(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "slot 幅 (mm):",
            Lang::En => "slot width (mm):",
        }
    }
    pub fn prompt_p076(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "back 高さ (mm):",
            Lang::En => "back height (mm):",
        }
    }
    pub fn prompt_p077(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "cable 穴径 (mm):",
            Lang::En => "cable hole diameter (mm):",
        }
    }
    pub fn prompt_p078(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "穴なし",
            Lang::En => "no hole",
        }
    }
    pub fn prompt_p079(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🎧 ヘッドホンホルダー (wall-mount + hook)",
            Lang::En => "🎧 Headphone holder (wall-mount + hook)",
        }
    }
    pub fn prompt_p080(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "arm 長 (mm):",
            Lang::En => "arm length (mm):",
        }
    }
    pub fn prompt_p081(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "headband 幅 (mm):",
            Lang::En => "headband width (mm):",
        }
    }
    pub fn prompt_p082(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "mount 幅 (mm):",
            Lang::En => "mount width (mm):",
        }
    }
    pub fn prompt_p083(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ヘッドホンホルダー arm{}mm×hb{}mm×mount{}mm",
            Lang::En => "Headphone holder arm{}mm × hb{}mm × mount{}mm",
        }
    }
    pub fn prompt_p084(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🔧 机下 clamp mount (C 字クランプ + 締付ネジ)",
            Lang::En => "🔧 Under-desk clamp mount (C-clamp + tightening screw)",
        }
    }
    pub fn prompt_p085(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "desk 厚 (mm):",
            Lang::En => "desk thickness (mm):",
        }
    }
    pub fn prompt_p086(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "clamp 幅 (mm):",
            Lang::En => "clamp width (mm):",
        }
    }
    pub fn prompt_p087(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "screw 径 (mm):",
            Lang::En => "screw diameter (mm):",
        }
    }
    pub fn prompt_p088(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "両面テープ",
            Lang::En => "double-sided tape",
        }
    }
    pub fn prompt_p089(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "机下 mount desk{}mm × clamp{}mm ({screw_note})",
            Lang::En => "Under-desk mount desk{}mm × clamp{}mm ({screw_note})",
        }
    }
    pub fn prompt_p090(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🗄 卓上シェルフ (平板 + 左右 2 脚)",
            Lang::En => "🗄 Desk shelf (flat top + 2 legs)",
        }
    }
    pub fn prompt_p091(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "shelf 幅 (mm):",
            Lang::En => "shelf width (mm):",
        }
    }
    pub fn prompt_p092(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "shelf 奥行 (mm):",
            Lang::En => "shelf depth (mm):",
        }
    }
    pub fn prompt_p093(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "leg 高 (mm):",
            Lang::En => "leg height (mm):",
        }
    }
    pub fn prompt_p094(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "シェルフ {}×{}mm × 脚{}mm",
            Lang::En => "Shelf {}×{}mm × leg {}mm",
        }
    }
    pub fn prompt_p095(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "注: 幅 315mm 超えは Bambu H2D 単一プリント不可 (要分割)",
            Lang::En => "Note: widths > 315mm exceed Bambu H2D single print (needs splitting)",
        }
    }
    pub fn prompt_p096(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🖥 モニターライザー (プラットフォーム + 2 脚 + cable)",
            Lang::En => "🖥 Monitor riser (platform + 2 legs + cable)",
        }
    }
    pub fn prompt_p097(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "幅 (mm):",
            Lang::En => "Width (mm):",
        }
    }
    pub fn prompt_p098(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "奥行 (mm):",
            Lang::En => "Depth (mm):",
        }
    }
    pub fn prompt_p099(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "モニターライザー {}×{}×{}mm",
            Lang::En => "Monitor riser {}×{}×{}mm",
        }
    }
    pub fn prompt_p100(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "cable 穴 Ø40mm 標準装備、単一プリント想定 (280mm 以下)",
            Lang::En => "Cable hole Ø40mm standard, single-print scope (≤ 280mm)",
        }
    }
    pub fn prompt_p101(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🥤 コースター (round + rim)",
            Lang::En => "🥤 Coaster (round + rim)",
        }
    }
    pub fn prompt_p102(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "直径 (mm):",
            Lang::En => "Diameter (mm):",
        }
    }
    pub fn prompt_p103(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "全厚 (mm):",
            Lang::En => "Total thickness (mm):",
        }
    }
    pub fn prompt_p104(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "コースター Ø{}×{}mm",
            Lang::En => "Coaster Ø{}×{}mm",
        }
    }
    pub fn prompt_p105(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "rim 2.5mm 幅 × 1.5mm 高 (液滴 catch)",
            Lang::En => "Rim 2.5mm width × 1.5mm height (drop catch)",
        }
    }
    pub fn prompt_p106(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🧻 ティッシュボックスカバー (bottom open + top slot)",
            Lang::En => "🧻 Tissue-box cover (bottom open + top slot)",
        }
    }
    pub fn prompt_p107(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "内部 長 (mm):",
            Lang::En => "Inner length (mm):",
        }
    }
    pub fn prompt_p108(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "内部 幅 (mm):",
            Lang::En => "Inner width (mm):",
        }
    }
    pub fn prompt_p109(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "内部 高 (mm):",
            Lang::En => "Inner height (mm):",
        }
    }
    pub fn prompt_p110(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ティッシュカバー 内 {}×{}×{}mm",
            Lang::En => "Tissue cover inner {}×{}×{}mm",
        }
    }
    pub fn prompt_p111(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: US rect (231×116×53) / Cube (114×114×127) / Square (114×114×100)"
            }
            Lang::En => "Presets: US rect (231×116×53) / Cube (114×114×127) / Square (114×114×100)",
        }
    }
    pub fn prompt_p112(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "📦 収納 BOX (top open、基本形)",
            Lang::En => "📦 Storage box (top open, basic)",
        }
    }
    pub fn prompt_p113(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "収納 BOX 内 {}×{}×{}mm",
            Lang::En => "Storage box inner {}×{}×{}mm",
        }
    }
    pub fn prompt_p114(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: Small (80×60×40) / Medium (150×100×60) / Large (200×150×80)"
            }
            Lang::En => "Presets: Small (80×60×40) / Medium (150×100×60) / Large (200×150×80)",
        }
    }
    pub fn prompt_p115(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "注: lid + hinge は future sprint、現状は top open 基本形",
            Lang::En => "Note: lid + hinge is a future sprint; current is top-open basic",
        }
    }
    pub fn prompt_p116(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🔌 ケーブルクリップ (snap-fit)",
            Lang::En => "🔌 Cable clip (snap-fit)",
        }
    }
    pub fn prompt_p117(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ケーブル直径 (mm):",
            Lang::En => "Cable diameter (mm):",
        }
    }
    pub fn prompt_p118(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "クリップ長 (mm):",
            Lang::En => "Clip length (mm):",
        }
    }
    pub fn prompt_p119(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ケーブルクリップ Ø{}×L{}mm",
            Lang::En => "Cable clip Ø{}×L{}mm",
        }
    }
    pub fn prompt_p120(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "プリセット目安: USB-C (Ø4.5/L22) / HDMI (Ø7/L28) / 電源 (Ø9/L36)",
            Lang::En => "Presets: USB-C (Ø4.5/L22) / HDMI (Ø7/L28) / power (Ø9/L36)",
        }
    }
    pub fn prompt_p121(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "💡 LED strip channel (U 溝、上端開口)",
            Lang::En => "💡 LED strip channel (U-groove, top open)",
        }
    }
    pub fn prompt_p122(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "strip PCB 幅 (mm):",
            Lang::En => "strip PCB width (mm):",
        }
    }
    pub fn prompt_p123(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "全長 (mm):",
            Lang::En => "Total length (mm):",
        }
    }
    pub fn prompt_p124(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "プリセット目安: SMD3528 (8mm) / WS2812B 標準 (10mm) / 高密度 (12mm)",
            Lang::En => "Presets: SMD3528 (8mm) / WS2812B std (10mm) / high density (12mm)",
        }
    }
    pub fn prompt_p125(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🎴 カードトレー (finger notch 付き)",
            Lang::En => "🎴 Card tray (with finger notch)",
        }
    }
    pub fn prompt_p126(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "カード幅 (mm):",
            Lang::En => "card width (mm):",
        }
    }
    pub fn prompt_p127(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "カード高さ (mm):",
            Lang::En => "card height (mm):",
        }
    }
    pub fn prompt_p128(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "tray 内深さ (mm):",
            Lang::En => "tray inner depth (mm):",
        }
    }
    pub fn prompt_p129(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "カードトレー {}×{}×深{}mm",
            Lang::En => "Card tray {}×{}× depth {}mm",
        }
    }
    pub fn prompt_p130(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: Poker (63×88) / Mini Euro (44×68) / Std Euro (59×92) / Tarot (70×120)"
            }
            Lang::En => {
                "Presets: Poker (63×88) / Mini Euro (44×68) / Std Euro (59×92) / Tarot (70×120)"
            }
        }
    }
    pub fn prompt_p131(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "目安: 深 30mm ≈ 100-150 cards、finger notch r=9mm 固定",
            Lang::En => "Guide: depth 30mm ≈ 100-150 cards, finger notch r=9mm fixed",
        }
    }
    pub fn prompt_p132(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🎲 トークン井戸 (row 配置 count well)",
            Lang::En => "🎲 Token well (row of counting wells)",
        }
    }
    pub fn prompt_p133(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "well 直径 (mm):",
            Lang::En => "well diameter (mm):",
        }
    }
    pub fn prompt_p134(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "well 深さ (mm):",
            Lang::En => "well depth (mm):",
        }
    }
    pub fn prompt_p135(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "well 個数:",
            Lang::En => "well count:",
        }
    }
    pub fn prompt_p136(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "トークン井戸 Ø{}×深{}mm × {}",
            Lang::En => "Token well Ø{}× depth {}mm × {}",
        }
    }
    pub fn prompt_p137(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: shallow token (10-15mm) / dice (20-25mm) / miniatures (30-40mm)"
            }
            Lang::En => "Presets: shallow token (10-15mm) / dice (20-25mm) / miniatures (30-40mm)",
        }
    }
    pub fn prompt_p138(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🔧 レンチホルダー (row 状 slot、min-max 等間隔)",
            Lang::En => "🔧 Wrench holder (row of slots, evenly spaced min-max)",
        }
    }
    pub fn prompt_p139(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "最小サイズ (mm):",
            Lang::En => "min size (mm):",
        }
    }
    pub fn prompt_p140(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "最大サイズ (mm):",
            Lang::En => "max size (mm):",
        }
    }
    pub fn prompt_p141(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "slot 個数:",
            Lang::En => "slot count:",
        }
    }
    pub fn prompt_p142(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "レンチホルダー {}-{}mm × {}",
            Lang::En => "Wrench holder {}-{}mm × {}",
        }
    }
    pub fn prompt_p143(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: Metric 8-19 (6 slot) / 8-24 (8 slot) / SAE 6-25 (1/4-1 inch)"
            }
            Lang::En => "Presets: Metric 8-19 (6 slot) / 8-24 (8 slot) / SAE 6-25 (1/4-1 inch)",
        }
    }
    pub fn prompt_p144(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🔩 ソケットレール (base + row post)",
            Lang::En => "🔩 Socket rail (base + row of posts)",
        }
    }
    pub fn prompt_p145(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "post 直径 (mm):",
            Lang::En => "post diameter (mm):",
        }
    }
    pub fn prompt_p146(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "post 高さ (mm):",
            Lang::En => "post height (mm):",
        }
    }
    pub fn prompt_p147(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "post 個数:",
            Lang::En => "post count:",
        }
    }
    pub fn prompt_p148(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ソケットレール Ø{}×H{}mm × {}",
            Lang::En => "Socket rail Ø{}×H{}mm × {}",
        }
    }
    pub fn prompt_p149(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Drive 目安: 1/4\"=6.0mm / 3/8\"=9.2mm / 1/2\"=12.4mm / 3/4\"=18.7mm",
            Lang::En => "Drive sizes: 1/4\"=6.0mm / 3/8\"=9.2mm / 1/2\"=12.4mm / 3/4\"=18.7mm",
        }
    }
    pub fn prompt_p150(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                " bit 想定 (across-flats 6.85mm × depth 14mm 固定) fn show_hex_bit_holder_customizer(ui: &mut egui::Ui, state: &mut AppState) {     ui.label(egui::RichText::new("
            }
            Lang::En => {
                " bit spec (across-flats 6.85mm × depth 14mm) fn show_hex_bit_holder_customizer(ui: &mut egui::Ui, state: &mut AppState) {     ui.label(egui::RichText::new("
            }
        }
    }
    pub fn prompt_p151(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "行数:",
            Lang::En => "rows:",
        }
    }
    pub fn prompt_p152(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "列数:",
            Lang::En => "columns:",
        }
    }
    pub fn prompt_p153(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "hole 間 pitch (mm):",
            Lang::En => "hole pitch (mm):",
        }
    }
    pub fn prompt_p154(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ビットホルダー {}×{} @ {}mm",
            Lang::En => "Bit holder {}×{} @ {}mm",
        }
    }
    pub fn prompt_p155(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: hex 6.85mm across-flats / depth 14mm (1/4\" bit 標準)",
            Lang::En => "Fixed: hex 6.85mm across-flats / depth 14mm (1/4\" bit standard)",
        }
    }
    pub fn prompt_p156(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🥧 Raspberry Pi ケース (standoff + port opening)",
            Lang::En => "🥧 Raspberry Pi case (standoff + port opening)",
        }
    }
    pub fn prompt_p157(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "PCB 幅 (mm):",
            Lang::En => "PCB width (mm):",
        }
    }
    pub fn prompt_p158(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "PCB 奥行 (mm):",
            Lang::En => "PCB depth (mm):",
        }
    }
    pub fn prompt_p159(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "内部高さ (mm):",
            Lang::En => "Inner height (mm):",
        }
    }
    pub fn prompt_p160(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "RPi ケース {}×{}×{}mm",
            Lang::En => "RPi case {}×{}×{}mm",
        }
    }
    pub fn prompt_p161(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "プリセット目安: RPi 5/4 (85×56、cooler 25 / bare 15) / Zero 2W (65×30×15)",
            Lang::En => "Presets: RPi 5/4 (85×56, cooler 25 / bare 15) / Zero 2W (65×30×15)",
        }
    }
    pub fn prompt_p162(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: 4 corner standoff Ø6mm × H5mm + M2.5 pilot、port opening 60mm 幅",
            Lang::En => "Fixed: 4 corner standoff Ø6mm × H5mm + M2.5 pilot, port opening 60mm wide",
        }
    }
    pub fn prompt_p163(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🔌 ESP32/Arduino エンクロージャ (friction、USB opening)",
            Lang::En => "🔌 ESP32/Arduino enclosure (friction, USB opening)",
        }
    }
    pub fn prompt_p164(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "MCU ケース {}×{}×{}mm",
            Lang::En => "MCU case {}×{}×{}mm",
        }
    }
    pub fn prompt_p165(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: ESP32 (51.6×28.4×15) / Arduino Uno (68.6×53.4×20) / Nano (45×18×12)"
            }
            Lang::En => {
                "Presets: ESP32 (51.6×28.4×15) / Arduino Uno (68.6×53.4×20) / Nano (45×18×12)"
            }
        }
    }
    pub fn prompt_p166(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: USB opening 短辺 9×5mm (USB-C 想定、Micro/Type-A は別途)",
            Lang::En => {
                "Fixed: USB opening short side 9×5mm (USB-C assumed; Micro/Type-A separately)"
            }
        }
    }
    pub fn prompt_p167(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🔋 18650 バッテリーホルダー (row 状 cavity)",
            Lang::En => "🔋 18650 battery holder (row of cavities)",
        }
    }
    pub fn prompt_p168(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "cell 個数:",
            Lang::En => "cell count:",
        }
    }
    pub fn prompt_p169(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "inter-cell 壁厚 (mm):",
            Lang::En => "inter-cell wall (mm):",
        }
    }
    pub fn prompt_p170(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "端部 floor 厚 (mm):",
            Lang::En => "end floor thickness (mm):",
        }
    }
    pub fn prompt_p171(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: cell Ø18.6mm × L68mm (18650 Li-ion 標準 + FDM clearance)",
            Lang::En => "Fixed: cell Ø18.6mm × L68mm (18650 Li-ion std + FDM clearance)",
        }
    }
    pub fn prompt_p172(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "floor=0 → 両端貫通 / floor>0 → 片端閉塞 (spring 保持)、素材は PETG/ABS 推奨"
            }
            Lang::En => {
                "floor=0 → open both ends / floor>0 → one end closed (spring retention); PETG/ABS recommended"
            }
        }
    }
    pub fn prompt_p173(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🪥 歯ブラシホルダー (row 状 hole、top 開口)",
            Lang::En => "🪥 Toothbrush holder (row of holes, top open)",
        }
    }
    pub fn prompt_p174(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "hole 個数:",
            Lang::En => "hole count:",
        }
    }
    pub fn prompt_p175(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "hole 直径 (mm):",
            Lang::En => "hole diameter (mm):",
        }
    }
    pub fn prompt_p176(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "hole 深さ (mm):",
            Lang::En => "hole depth (mm):",
        }
    }
    pub fn prompt_p177(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "歯ブラシホルダー {} × Ø{}×H{}mm",
            Lang::En => "Toothbrush holder {} × Ø{}×H{}mm",
        }
    }
    pub fn prompt_p178(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: manual (Ø15) / electric Sonicare (Ø32) / electric Oral-B (Ø40)"
            }
            Lang::En => "Presets: manual (Ø15) / electric Sonicare (Ø32) / electric Oral-B (Ø40)",
        }
    }
    pub fn prompt_p179(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "素材: PETG 推奨 (moisture resistance)、drainage 穴は user 側で追加加工推奨"
            }
            Lang::En => {
                "Material: PETG recommended (moisture resistance); drill drainage holes on your side"
            }
        }
    }
    pub fn prompt_p180(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🪛 ドリルビットホルダー (row 状 hole、min-max 補間)",
            Lang::En => "🪛 Drill-bit holder (row of holes, min-max interpolated)",
        }
    }
    pub fn prompt_p181(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "最小径 (mm):",
            Lang::En => "min diameter (mm):",
        }
    }
    pub fn prompt_p182(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "最大径 (mm):",
            Lang::En => "max diameter (mm):",
        }
    }
    pub fn prompt_p183(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "プリセット目安: Metric 3-13mm × 11 (1mm step) / 1-10mm × 19 (0.5mm step)",
            Lang::En => "Presets: Metric 3-13mm × 11 (1mm step) / 1-10mm × 19 (0.5mm step)",
        }
    }
    pub fn prompt_p184(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🔧 プライヤーラック (row 状 rect slot)",
            Lang::En => "🔧 Pliers rack (row of rectangular slots)",
        }
    }
    pub fn prompt_p185(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "slot 深さ (mm):",
            Lang::En => "slot depth (mm):",
        }
    }
    pub fn prompt_p186(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "プライヤーラック {} × W{}×D{}mm",
            Lang::En => "Pliers rack {} × W{}×D{}mm",
        }
    }
    pub fn prompt_p187(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "プリセット目安: needle-nose (W10) / combi (W15) / tongue-groove (W20-25)",
            Lang::En => "Presets: needle-nose (W10) / combi (W15) / tongue-groove (W20-25)",
        }
    }
    pub fn prompt_p188(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "jar 個数:",
            Lang::En => "jar count:",
        }
    }
    pub fn prompt_p189(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "jar 直径 (mm):",
            Lang::En => "jar diameter (mm):",
        }
    }
    pub fn prompt_p190(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "jar 高さ (mm):",
            Lang::En => "jar height (mm):",
        }
    }
    pub fn prompt_p191(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "プリセット目安: small (Ø42×H75) / std (Ø48×H100) / large (Ø52×H120)",
            Lang::En => "Presets: small (Ø42×H75) / std (Ø48×H100) / large (Ø52×H120)",
        }
    }
    pub fn prompt_p192(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: recess 深 5mm、shelf 厚 5mm、front lip 高 = jar_height × 15%",
            Lang::En => {
                "Fixed: recess depth 5mm, shelf thickness 5mm, front lip = jar_height × 15%"
            }
        }
    }
    pub fn prompt_p193(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🥚 卵トレー (2D grid、egg cup Ø40mm 固定)",
            Lang::En => "🥚 Egg tray (2D grid, egg cup Ø40mm fixed)",
        }
    }
    pub fn prompt_p194(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "cup 深さ (mm):",
            Lang::En => "cup depth (mm):",
        }
    }
    pub fn prompt_p195(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "卵トレー {}×{} × 深{}mm",
            Lang::En => "Egg tray {}×{} × depth {}mm",
        }
    }
    pub fn prompt_p196(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "プリセット目安: 12-egg tray (4×3) / 6-egg (3×2) / 4×4 (16-egg 大量)",
            Lang::En => "Presets: 12-egg tray (4×3) / 6-egg (3×2) / 4×4 (16-egg bulk)",
        }
    }
    pub fn prompt_p197(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "固定: egg cup Ø40mm (large egg spec)、pitch 50mm、素材 PETG 推奨 (冷蔵庫用)"
            }
            Lang::En => {
                "Fixed: egg cup Ø40mm (large egg spec), pitch 50mm, PETG recommended (fridge use)"
            }
        }
    }
    pub fn prompt_p198(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🍴 キッチンツールキャディ (row 状 large compartment)",
            Lang::En => "🍴 Kitchen utensil caddy (row of large compartments)",
        }
    }
    pub fn prompt_p199(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "compartment 個数:",
            Lang::En => "compartment count:",
        }
    }
    pub fn prompt_p200(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "compartment 内径 (mm):",
            Lang::En => "compartment inner diameter (mm):",
        }
    }
    pub fn prompt_p201(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "compartment 高さ (mm):",
            Lang::En => "compartment height (mm):",
        }
    }
    pub fn prompt_p202(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ツールキャディ {} × Ø{}×H{}mm",
            Lang::En => "Tool caddy {} × Ø{}×H{}mm",
        }
    }
    pub fn prompt_p203(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: small (Ø45-50、whisk/peeler) / large (Ø60-70、spatula/ladle)"
            }
            Lang::En => "Presets: small (Ø45-50, whisk/peeler) / large (Ø60-70, spatula/ladle)",
        }
    }
    pub fn prompt_p204(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "素材: PETG 推奨 (水濺ね対応)、drainage 穴は user 側で追加加工",
            Lang::En => "Material: PETG recommended (water splash); add drainage holes yourself",
        }
    }
    pub fn prompt_p205(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🎞 フィラメントスプールホルダー (base + 垂直 peg)",
            Lang::En => "🎞 Filament spool holder (base + vertical peg)",
        }
    }
    pub fn prompt_p206(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "spool 外径 (mm):",
            Lang::En => "spool outer diameter (mm):",
        }
    }
    pub fn prompt_p207(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "spool 幅 (mm):",
            Lang::En => "spool width (mm):",
        }
    }
    pub fn prompt_p208(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "bore 内径 (mm):",
            Lang::En => "bore inner diameter (mm):",
        }
    }
    pub fn prompt_p209(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "スプールホルダー Ø{}×W{}×bore{}mm",
            Lang::En => "Spool holder Ø{}×W{}×bore{}mm",
        }
    }
    pub fn prompt_p210(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: 1kg (Ø200×W68×bore52) / 250g (Ø125×W45×bore30) / 2kg (Ø250×W80×bore70)"
            }
            Lang::En => {
                "Presets: 1kg (Ø200×W68×bore52) / 250g (Ø125×W45×bore30) / 2kg (Ø250×W80×bore70)"
            }
        }
    }
    pub fn prompt_p211(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: base_thickness 5mm、peg clearance 1mm (slide fit)、peg 追加高 20mm",
            Lang::En => {
                "Fixed: base_thickness 5mm, peg clearance 1mm (slide fit), peg additional height 20mm"
            }
        }
    }
    pub fn prompt_p212(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🔩 ノズルホルダー (row 状 M6 nozzle hole)",
            Lang::En => "🔩 Nozzle holder (row of M6 nozzle holes)",
        }
    }
    pub fn prompt_p213(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ノズルホルダー {} hole × Ø{}×D{}mm",
            Lang::En => "Nozzle holder {} hole × Ø{}×D{}mm",
        }
    }
    pub fn prompt_p214(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "プリセット目安: E3D V6/Bambu M6 (Ø8×D6) / large hotend (Ø10-12×D8)",
            Lang::En => "Presets: E3D V6/Bambu M6 (Ø8×D6) / large hotend (Ø10-12×D8)",
        }
    }
    pub fn prompt_p215(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Label は user 側で別途印刷 or Sharpie 書込み推奨",
            Lang::En => "Print labels separately or write with a Sharpie",
        }
    }
    pub fn prompt_p216(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🏗 ビルドプレートラック (row 状 vertical slot)",
            Lang::En => "🏗 Build plate rack (row of vertical slots)",
        }
    }
    pub fn prompt_p217(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "rack 全高 (mm):",
            Lang::En => "rack total height (mm):",
        }
    }
    pub fn prompt_p218(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "プレートラック {} slot × spacing {}mm × H{}mm",
            Lang::En => "Plate rack {} slot × spacing {}mm × H{}mm",
        }
    }
    pub fn prompt_p219(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: Ender/Bambu 235mm (H200) / Bambu 256mm (H225) / Voron 350mm (H300)"
            }
            Lang::En => {
                "Presets: Ender/Bambu 235mm (H200) / Bambu 256mm (H225) / Voron 350mm (H300)"
            }
        }
    }
    pub fn prompt_p220(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: slot width 5.5mm (5mm plate + 0.5mm clearance)、depth 60mm",
            Lang::En => "Fixed: slot width 5.5mm (5mm plate + 0.5mm clearance), depth 60mm",
        }
    }
    pub fn prompt_p221(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🍴 カトラリートレー (drawer 引き出し用、long slot)",
            Lang::En => "🍴 Cutlery tray (for drawers, long slots)",
        }
    }
    pub fn prompt_p222(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "slot 長 (mm):",
            Lang::En => "slot length (mm):",
        }
    }
    pub fn prompt_p223(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "カトラリートレー {} slot × W{}×L{}mm",
            Lang::En => "Cutlery tray {} slot × W{}×L{}mm",
        }
    }
    pub fn prompt_p224(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: fork (W30-35) / knife (W25-30) / spoon (W50-55)、長さ 220mm 標準"
            }
            Lang::En => {
                "Presets: fork (W30-35) / knife (W25-30) / spoon (W50-55), length 220mm standard"
            }
        }
    }
    pub fn prompt_p225(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "💊 薬箱 (2D grid rect cell、weekly pill box)",
            Lang::En => "💊 Pill box (2D grid rect cells, weekly)",
        }
    }
    pub fn prompt_p226(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "cell 内寸 (mm):",
            Lang::En => "cell inner size (mm):",
        }
    }
    pub fn prompt_p227(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "薬箱 {}×{} × cell {}mm",
            Lang::En => "Pill box {}×{} × cell {}mm",
        }
    }
    pub fn prompt_p228(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: weekly AM/PM (7×2×20) / small daily (3×1×15) / large (7×4×25)"
            }
            Lang::En => "Presets: weekly AM/PM (7×2×20) / small daily (3×1×15) / large (7×4×25)",
        }
    }
    pub fn prompt_p229(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: cell 深 15mm、wall 1.5mm、floor 1.5mm",
            Lang::En => "Fixed: cell depth 15mm, wall 1.5mm, floor 1.5mm",
        }
    }
    pub fn prompt_p230(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "magnet 個数:",
            Lang::En => "magnet count:",
        }
    }
    pub fn prompt_p231(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "magnet 直径 (mm):",
            Lang::En => "magnet diameter (mm):",
        }
    }
    pub fn prompt_p232(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "マグネットバー {} × Ø{} spacing {}mm",
            Lang::En => "Magnet bar {} × Ø{} spacing {}mm",
        }
    }
    pub fn prompt_p233(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: kitchen knife rail (8×Ø6×30) / small tool (5×Ø8×25) / large (12×Ø10×40)"
            }
            Lang::En => {
                "Presets: kitchen knife rail (8×Ø6×30) / small tool (5×Ø8×25) / large (12×Ø10×40)"
            }
        }
    }
    pub fn prompt_p234(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "固定: bar 厚 5mm、bar 高 15mm、magnet 埋込 2mm (magnet は user 側で press-fit 挿入)"
            }
            Lang::En => {
                "Fixed: bar thickness 5mm, height 15mm, magnet embed 2mm (press-fit magnet on your side)"
            }
        }
    }
    pub fn prompt_p235(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "💨 ヘアドライヤーホルダー (大径 holster)",
            Lang::En => "💨 Hair-dryer holder (large holster)",
        }
    }
    pub fn prompt_p236(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "barrel 内径 (mm):",
            Lang::En => "barrel inner diameter (mm):",
        }
    }
    pub fn prompt_p237(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "holster 深さ (mm):",
            Lang::En => "holster depth (mm):",
        }
    }
    pub fn prompt_p238(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ドライヤーホルダー Ø{}×D{}mm",
            Lang::En => "Dryer holder Ø{}×D{}mm",
        }
    }
    pub fn prompt_p239(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "プリセット目安: Dyson Supersonic (Ø85) / 汎用 (Ø45-90) / 業務用 (Ø100+)",
            Lang::En => "Presets: Dyson Supersonic (Ø85) / generic (Ø45-90) / commercial (Ø100+)",
        }
    }
    pub fn prompt_p240(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: 内 clearance 2mm、floor 5mm (荷重 400-700g 想定)",
            Lang::En => "Fixed: inner clearance 2mm, floor 5mm (load 400-700g)",
        }
    }
    pub fn prompt_p241(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "☕ K-Cup ホルダー (2D grid capsule wells)",
            Lang::En => "☕ K-Cup holder (2D grid capsule wells)",
        }
    }
    pub fn prompt_p242(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "capsule 直径 (mm):",
            Lang::En => "capsule diameter (mm):",
        }
    }
    pub fn prompt_p243(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "K-Cup ホルダー {}×{} × Ø{}mm",
            Lang::En => "K-Cup holder {}×{} × Ø{}mm",
        }
    }
    pub fn prompt_p244(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: K-Cup (Ø53) / Nespresso Original (Ø39) / Dolce Gusto (Ø55)"
            }
            Lang::En => "Presets: K-Cup (Ø53) / Nespresso Original (Ø39) / Dolce Gusto (Ø55)",
        }
    }
    pub fn prompt_p245(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: capsule 深 40mm、pitch = capsule + 3.5mm、floor 3mm",
            Lang::En => "Fixed: capsule depth 40mm, pitch = capsule + 3.5mm, floor 3mm",
        }
    }
    pub fn prompt_p246(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🔩 ヘックスキーホルダー (Allen key、block-style)",
            Lang::En => "🔩 Hex-key holder (Allen key, block-style)",
        }
    }
    pub fn prompt_p247(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "key 個数:",
            Lang::En => "key count:",
        }
    }
    pub fn prompt_p248(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "最小 key 幅 (mm):",
            Lang::En => "min key size (mm):",
        }
    }
    pub fn prompt_p249(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "最大 key 幅 (mm):",
            Lang::En => "max key size (mm):",
        }
    }
    pub fn prompt_p250(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ヘックスキーホルダー {}-{}mm × {}",
            Lang::En => "Hex-key holder {}-{}mm × {}",
        }
    }
    pub fn prompt_p251(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "プリセット目安: Metric 9-piece (1.5-10mm) / SAE 12-piece (0.05-3/8 inch)",
            Lang::En => "Presets: Metric 9-piece (1.5-10mm) / SAE 12-piece (0.05-3/8 inch)",
        }
    }
    pub fn prompt_p252(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: hole 深 18mm、clearance 0.3mm/side (key + 0.6mm total)",
            Lang::En => "Fixed: hole depth 18mm, clearance 0.3mm/side (key + 0.6mm total)",
        }
    }
    pub fn prompt_p253(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🎞 Wrap/Foil ロールホルダー (半円 cradle)",
            Lang::En => "🎞 Wrap/Foil roll holder (half-circle cradle)",
        }
    }
    pub fn prompt_p254(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "roll 外径 (mm):",
            Lang::En => "roll outer diameter (mm):",
        }
    }
    pub fn prompt_p255(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "roll 幅 (mm):",
            Lang::En => "roll width (mm):",
        }
    }
    pub fn prompt_p256(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Wrap ホルダー Ø{}×W{}mm",
            Lang::En => "Wrap holder Ø{}×W{}mm",
        }
    }
    pub fn prompt_p257(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: 12\" foil (Ø55×W305) / 18\" restaurant (Ø55×W457) / plastic wrap (Ø45×W305)"
            }
            Lang::En => {
                "Presets: 12\" foil (Ø55×W305) / 18\" restaurant (Ø55×W457) / plastic wrap (Ø45×W305)"
            }
        }
    }
    pub fn prompt_p258(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: cradle depth ratio 60%、roll clearance 1.5mm/side",
            Lang::En => "Fixed: cradle depth ratio 60%, roll clearance 1.5mm/side",
        }
    }
    pub fn prompt_p259(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🧦 靴下 divider (frame + partition walls)",
            Lang::En => "🧦 Sock divider (frame + partition walls)",
        }
    }
    pub fn prompt_p260(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "cell 幅 (mm):",
            Lang::En => "cell width (mm):",
        }
    }
    pub fn prompt_p261(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "靴下 divider {} cell × W{}×H{}mm",
            Lang::En => "Sock divider {} cell × W{}×H{}mm",
        }
    }
    pub fn prompt_p262(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "プリセット目安: sock (4×80×89) / underwear (4×100×89) / bra (3×150×89)",
            Lang::En => "Presets: sock (4×80×89) / underwear (4×100×89) / bra (3×150×89)",
        }
    }
    pub fn prompt_p263(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: cell 奥行 100mm、wall 2.5mm、floor 2mm",
            Lang::En => "Fixed: cell depth 100mm, wall 2.5mm, floor 2mm",
        }
    }
    pub fn prompt_p264(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🧼 石鹸トレー (tray + drain slots)",
            Lang::En => "🧼 Soap tray (tray + drain slots)",
        }
    }
    pub fn prompt_p265(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "tray 内 長 (mm):",
            Lang::En => "tray inner length (mm):",
        }
    }
    pub fn prompt_p266(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "tray 内 幅 (mm):",
            Lang::En => "tray inner width (mm):",
        }
    }
    pub fn prompt_p267(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "drain slot 個数:",
            Lang::En => "drain slot count:",
        }
    }
    pub fn prompt_p268(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "石鹸トレー L{}×W{}mm × {} drain",
            Lang::En => "Soap tray L{}×W{}mm × {} drain",
        }
    }
    pub fn prompt_p269(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: dual-bottle shampoo (L200×W90) / bar soap (L100×W70) / large tray (L280×W140)"
            }
            Lang::En => {
                "Presets: dual-bottle shampoo (L200×W90) / bar soap (L100×W70) / large tray (L280×W140)"
            }
        }
    }
    pub fn prompt_p270(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "固定: tray 深 12mm、drain slot 幅 3mm、wall 2.5mm、floor 2mm、素材 PETG 推奨"
            }
            Lang::En => {
                "Fixed: tray depth 12mm, drain slot width 3mm, wall 2.5mm, floor 2mm, PETG recommended"
            }
        }
    }
    pub fn prompt_p271(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🪒 カミソリホルダー (wall-mount + mount hole)",
            Lang::En => "🪒 Razor holder (wall-mount + mount hole)",
        }
    }
    pub fn prompt_p272(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "mount 穴径 (mm):",
            Lang::En => "mount hole diameter (mm):",
        }
    }
    pub fn prompt_p273(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "カミソリホルダー W{}×D{}mm × mount Ø{}",
            Lang::En => "Razor holder W{}×D{}mm × mount Ø{}",
        }
    }
    pub fn prompt_p274(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "プリセット目安: Mach3/Fusion cartridge (W12×D22) / safety razor (W10×D25)",
            Lang::En => "Presets: Mach3/Fusion cartridge (W12×D22) / safety razor (W10×D25)",
        }
    }
    pub fn prompt_p275(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: backplate 80×60mm、素材 PETG 推奨 (moisture resistance)",
            Lang::En => "Fixed: backplate 80×60mm, PETG recommended (moisture resistance)",
        }
    }
    pub fn prompt_p276(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🥢 箸ホルダー (row 状 narrow long slots)",
            Lang::En => "🥢 Chopstick holder (row of narrow long slots)",
        }
    }
    pub fn prompt_p277(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "pair 個数:",
            Lang::En => "pair count:",
        }
    }
    pub fn prompt_p278(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "箸ホルダー {} pair × W{}×L{}mm",
            Lang::En => "Chopstick holder {} pair × W{}×L{}mm",
        }
    }
    pub fn prompt_p279(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: adult (W13×L260) / cooking (W15×L310) / children (W10×L180)"
            }
            Lang::En => "Presets: adult (W13×L260) / cooking (W15×L310) / children (W10×L180)",
        }
    }
    pub fn prompt_p280(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🎨 フィラメントスウォッチホルダー (2D grid narrow slots)",
            Lang::En => "🎨 Filament swatch holder (2D grid narrow slots)",
        }
    }
    pub fn prompt_p281(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "swatch 幅 (mm):",
            Lang::En => "swatch width (mm):",
        }
    }
    pub fn prompt_p282(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "スウォッチホルダー {}×{} × W{}mm",
            Lang::En => "Swatch holder {}×{} × W{}mm",
        }
    }
    pub fn prompt_p283(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: standard card (32×70mm) / small square (24×24mm) / full card (60×24.5mm)"
            }
            Lang::En => {
                "Presets: standard card (32×70mm) / small square (24×24mm) / full card (60×24.5mm)"
            }
        }
    }
    pub fn prompt_p284(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: swatch 高 70mm、厚 4.5mm、wall 2mm、floor 3mm",
            Lang::En => "Fixed: swatch height 70mm, thickness 4.5mm, wall 2mm, floor 3mm",
        }
    }
    pub fn prompt_p285(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🧻 トイレットペーパーホルダー (wall-mount backplate + axle)",
            Lang::En => "🧻 Toilet-paper holder (wall-mount backplate + axle)",
        }
    }
    pub fn prompt_p286(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ロール内径 (mm):",
            Lang::En => "roll inner diameter (mm):",
        }
    }
    pub fn prompt_p287(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ロール幅 = 軸長 (mm):",
            Lang::En => "roll width = axle length (mm):",
        }
    }
    pub fn prompt_p288(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "backplate 厚 (mm):",
            Lang::En => "backplate thickness (mm):",
        }
    }
    pub fn prompt_p289(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "TP ホルダー 内径Ø{} × W{}mm × 板{}mm",
            Lang::En => "TP holder inner Ø{} × W{}mm × plate {}mm",
        }
    }
    pub fn prompt_p290(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "プリセット目安: standard (Ø40×W110×5) / thick backplate (Ø40×W110×8)",
            Lang::En => "Presets: standard (Ø40×W110×5) / thick backplate (Ø40×W110×8)",
        }
    }
    pub fn prompt_p291(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: backplate 80×80mm、M4 mount hole 2 個 (上部左右)",
            Lang::En => "Fixed: backplate 80×80mm, M4 mount hole × 2 (upper left/right)",
        }
    }
    pub fn prompt_p292(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "💾 SD カードホルダー (2D grid narrow slots)",
            Lang::En => "💾 SD card holder (2D grid narrow slots)",
        }
    }
    pub fn prompt_p293(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "SD カードホルダー {}×{} × W{}mm",
            Lang::En => "SD card holder {}×{} × W{}mm",
        }
    }
    pub fn prompt_p294(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "プリセット目安: SD full (24×32mm) / microSD (15×11mm)",
            Lang::En => "Presets: SD full (24×32mm) / microSD (15×11mm)",
        }
    }
    pub fn prompt_p295(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: カード高 32mm、厚 2.5mm、wall 1.5mm、floor 2mm",
            Lang::En => "Fixed: card height 32mm, thickness 2.5mm, wall 1.5mm, floor 2mm",
        }
    }
    pub fn prompt_p296(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🔧 ドライバーラック (row 状 large cyl hole)",
            Lang::En => "🔧 Driver rack (row of large cyl holes)",
        }
    }
    pub fn prompt_p297(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "slot 直径 (mm):",
            Lang::En => "slot diameter (mm):",
        }
    }
    pub fn prompt_p298(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ラック高さ (mm):",
            Lang::En => "rack height (mm):",
        }
    }
    pub fn prompt_p299(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ドライバーラック {} slot × Ø{} × H{}mm",
            Lang::En => "Driver rack {} slot × Ø{} × H{}mm",
        }
    }
    pub fn prompt_p300(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "プリセット目安: standard (8 × Ø25 × H100) / precision (12 × Ø15 × H80)",
            Lang::En => "Presets: standard (8 × Ø25 × H100) / precision (12 × Ø15 × H80)",
        }
    }
    pub fn prompt_p301(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: wall 3mm、floor 5mm、床開口なし (handle 上向き挿入)",
            Lang::En => "Fixed: wall 3mm, floor 5mm, no bottom opening (handle inserted upward)",
        }
    }
    pub fn prompt_p302(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🌸 綿棒/コットン ディスペンサー (open top cyl + inner cavity)",
            Lang::En => "🌸 Cotton swab/ball dispenser (open-top cyl + inner cavity)",
        }
    }
    pub fn prompt_p303(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "収容目安個数:",
            Lang::En => "approx capacity:",
        }
    }
    pub fn prompt_p304(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "cavity 内径 (mm):",
            Lang::En => "cavity inner diameter (mm):",
        }
    }
    pub fn prompt_p305(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "全高 (mm):",
            Lang::En => "Total height (mm):",
        }
    }
    pub fn prompt_p306(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "コットン ディスペンサー {} 個 × Ø{}mm × H{}mm",
            Lang::En => "Cotton dispenser {} pcs × Ø{}mm × H{}mm",
        }
    }
    pub fn prompt_p307(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "プリセット目安: standard (80 × Ø90 × H100) / large (150 × Ø110 × H130)",
            Lang::En => "Presets: standard (80 × Ø90 × H100) / large (150 × Ø110 × H130)",
        }
    }
    pub fn prompt_p308(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: wall 2.5mm、floor 2.5mm、count は informational (SDF に非反映)",
            Lang::En => {
                "Fixed: wall 2.5mm, floor 2.5mm; count is informational (not reflected in SDF)"
            }
        }
    }
    pub fn prompt_p309(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🧽 スポンジホルダー (drain hole 付き rect tray)",
            Lang::En => "🧽 Sponge holder (rect tray with drain holes)",
        }
    }
    pub fn prompt_p310(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "tray 長 (mm):",
            Lang::En => "tray length (mm):",
        }
    }
    pub fn prompt_p311(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "tray 幅 (mm):",
            Lang::En => "tray width (mm):",
        }
    }
    pub fn prompt_p312(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "drain hole 個数:",
            Lang::En => "drain hole count:",
        }
    }
    pub fn prompt_p313(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "スポンジホルダー L{} × W{}mm × {} drain",
            Lang::En => "Sponge holder L{} × W{}mm × {} drain",
        }
    }
    pub fn prompt_p314(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "プリセット目安: standard (L200×W100×8) / large sink (L280×W130×12)",
            Lang::En => "Presets: standard (L200×W100×8) / large sink (L280×W130×12)",
        }
    }
    pub fn prompt_p315(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: tray 深 30mm、drain Ø6mm、wall 2.5mm、floor 2.5mm",
            Lang::En => "Fixed: tray depth 30mm, drain Ø6mm, wall 2.5mm, floor 2.5mm",
        }
    }
    pub fn prompt_p316(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🔨 クランプ壁掛けラック (row 状 hook + backplate)",
            Lang::En => "🔨 Clamp wall rack (row of hooks + backplate)",
        }
    }
    pub fn prompt_p317(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "hook 個数:",
            Lang::En => "hook count:",
        }
    }
    pub fn prompt_p318(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "各 hook 幅 (mm):",
            Lang::En => "each hook width (mm):",
        }
    }
    pub fn prompt_p319(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: standard (5 × W30 × H150) / large workshop (8 × W50 × H250)"
            }
            Lang::En => "Presets: standard (5 × W30 × H150) / large workshop (8 × W50 × H250)",
        }
    }
    pub fn prompt_p320(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: hook 深 25mm、opening 15mm、backplate 厚 5mm、M4 mount hole 2 個",
            Lang::En => {
                "Fixed: hook depth 25mm, opening 15mm, backplate thickness 5mm, M4 mount hole × 2"
            }
        }
    }
    pub fn prompt_p321(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "spool 行数:",
            Lang::En => "spool rows:",
        }
    }
    pub fn prompt_p322(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "spool 列数:",
            Lang::En => "spool columns:",
        }
    }
    pub fn prompt_p323(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "プリセット目安: 4 spool 2×2 (1kg PLA 4本 × Ø68) / 2 spool 2×1 (Ø68)",
            Lang::En => "Presets: 4 spool 2×2 (1kg PLA 4 × Ø68) / 2 spool 2×1 (Ø68)",
        }
    }
    pub fn prompt_p324(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: spool 幅 70mm、wall 3mm、floor 3mm (lid + 除湿剤 slot は別 print)",
            Lang::En => {
                "Fixed: spool width 70mm, wall 3mm, floor 3mm (lid + desiccant slot printed separately)"
            }
        }
    }
    pub fn prompt_p325(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🛡 屋外用 IP54 密閉筐体 (raspi_case + gasket groove)",
            Lang::En => "🛡 Outdoor IP54 sealed enclosure (raspi_case + gasket groove)",
        }
    }
    pub fn prompt_p326(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "内部 奥行 (mm):",
            Lang::En => "inner depth (mm):",
        }
    }
    pub fn prompt_p327(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "内部 高さ (mm):",
            Lang::En => "inner height (mm):",
        }
    }
    pub fn prompt_p328(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "IP54 筐体 内部 W{}×D{}×H{}mm",
            Lang::En => "IP54 enclosure inner W{}×D{}×H{}mm",
        }
    }
    pub fn prompt_p329(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: Arduino UNO (120×80×45) / Raspi 5 (100×70×35) / large (160×110×60)"
            }
            Lang::En => {
                "Presets: Arduino UNO (120×80×45) / Raspi 5 (100×70×35) / large (160×110×60)"
            }
        }
    }
    pub fn prompt_p330(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: 壁 3.5mm、gasket 溝 W2×D1.5mm (O-ring 対応、lid は別 print)",
            Lang::En => {
                "Fixed: wall 3.5mm, gasket groove W2×D1.5mm (O-ring); lid printed separately"
            }
        }
    }
    pub fn prompt_p331(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "💍 ジュエリー段付きスタンド (multi-tier disk stack)",
            Lang::En => "💍 Jewelry tiered stand (multi-tier disk stack)",
        }
    }
    pub fn prompt_p332(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "tier 段数:",
            Lang::En => "tier count:",
        }
    }
    pub fn prompt_p333(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "最下段直径 (mm):",
            Lang::En => "bottom-tier diameter (mm):",
        }
    }
    pub fn prompt_p334(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ジュエリー スタンド {} tier × Ø{}mm × H{}mm",
            Lang::En => "Jewelry stand {} tier × Ø{}mm × H{}mm",
        }
    }
    pub fn prompt_p335(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: standard 3 tier (Ø100×H100) / small 2 tier (Ø80×H70) / large 4 tier (Ø130×H150)"
            }
            Lang::En => {
                "Presets: standard 3 tier (Ø100×H100) / small 2 tier (Ø80×H70) / large 4 tier (Ø130×H150)"
            }
        }
    }
    pub fn prompt_p336(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: tier 厚 5mm、pillar Ø10mm、上段ほど 70% 小径 (wedding cake style)",
            Lang::En => {
                "Fixed: tier thickness 5mm, pillar Ø10mm, upper tiers scale 70% (wedding-cake style)"
            }
        }
    }
    pub fn prompt_p337(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "📱 充電ドック (base + tilted upright + USB-C 貫通、multi-component)",
            Lang::En => {
                "📱 Charging dock (base + tilted upright + USB-C pass-through, multi-component)"
            }
        }
    }
    pub fn prompt_p338(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "base 幅 (mm):",
            Lang::En => "base width (mm):",
        }
    }
    pub fn prompt_p339(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "upright 高さ (mm):",
            Lang::En => "upright height (mm):",
        }
    }
    pub fn prompt_p340(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "USB-C 貫通穴 Ø (mm):",
            Lang::En => "USB-C pass-through Ø (mm):",
        }
    }
    pub fn prompt_p341(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "充電ドック W{} × H{}mm × Ø{} 貫通",
            Lang::En => "Charging dock W{} × H{}mm × Ø{} pass-through",
        }
    }
    pub fn prompt_p342(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: standard (80×100×Ø8) / small (60×80×Ø6) / large tablet (120×150×Ø10)"
            }
            Lang::En => {
                "Presets: standard (80×100×Ø8) / small (60×80×Ø6) / large tablet (120×150×Ø10)"
            }
        }
    }
    pub fn prompt_p343(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: base 60mm 奥行 × 6mm 厚、upright 4mm 厚、15deg 傾斜、charger 下配線",
            Lang::En => {
                "Fixed: base 60mm depth × 6mm thick, upright 4mm thick, 15deg tilt, charger wiring below"
            }
        }
    }
    pub fn prompt_p344(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🍳 まな板ラック (tall vertical slots)",
            Lang::En => "🍳 Cutting board rack (tall vertical slots)",
        }
    }
    pub fn prompt_p345(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "まな板ラック {} slot × W{}mm × H{}mm",
            Lang::En => "Cutting board rack {} slot × W{}mm × H{}mm",
        }
    }
    pub fn prompt_p346(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: standard (3 slot × W12 × H220) / large (4 slot × W20 × H280)"
            }
            Lang::En => "Presets: standard (3 slot × W12 × H220) / large (4 slot × W20 × H280)",
        }
    }
    pub fn prompt_p347(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: slot 深 200mm、wall 4mm、floor 8mm (まな板重量支え)",
            Lang::En => {
                "Fixed: slot depth 200mm, wall 4mm, floor 8mm (supports cutting board weight)"
            }
        }
    }
    pub fn prompt_p348(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "📼 テープ dispenser (4 component composite + tear edge)",
            Lang::En => "📼 Tape dispenser (4-component composite + tear edge)",
        }
    }
    pub fn prompt_p349(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ロール幅 (mm):",
            Lang::En => "roll width (mm):",
        }
    }
    pub fn prompt_p350(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "テープ dispenser Ø{} × W{} × wall{}mm",
            Lang::En => "Tape dispenser Ø{} × W{} × wall {}mm",
        }
    }
    pub fn prompt_p351(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: 包装用 standard (Ø76×W50) / セロハンテープ (Ø25×W15) / 養生 (Ø90×W48)"
            }
            Lang::En => {
                "Presets: packaging standard (Ø76×W50) / cellophane (Ø25×W15) / masking (Ø90×W48)"
            }
        }
    }
    pub fn prompt_p352(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "固定: 外径 Ø150 hood、tear edge 30deg、base plate + back wall + hood + Z-axis axle 4 component"
            }
            Lang::En => {
                "Fixed: outer Ø150 hood, tear edge 30deg, base plate + back wall + hood + Z-axis axle (4 components)"
            }
        }
    }
    pub fn prompt_p353(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🚿 シャワー用棚 (multi-tier wall-mount tray、multi-component)",
            Lang::En => "🚿 Shower shelf (multi-tier wall-mount tray, multi-component)",
        }
    }
    pub fn prompt_p354(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "tier 長 (mm):",
            Lang::En => "tier length (mm):",
        }
    }
    pub fn prompt_p355(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "tier 奥行 (mm):",
            Lang::En => "tier depth (mm):",
        }
    }
    pub fn prompt_p356(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "シャワー棚 {} tier × L{} × D{}mm",
            Lang::En => "Shower shelf {} tier × L{} × D{}mm",
        }
    }
    pub fn prompt_p357(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "プリセット目安: standard 2 tier (L250×D120) / large 3 tier (L300×D150)",
            Lang::En => "Presets: standard 2 tier (L250×D120) / large 3 tier (L300×D150)",
        }
    }
    pub fn prompt_p358(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "固定: tier 深 40mm、tier 間隔 100mm、drain 6 hole/tier Ø5mm、M4 mount hole 2 個"
            }
            Lang::En => {
                "Fixed: tier depth 40mm, tier spacing 100mm, drain 6 hole/tier Ø5mm, M4 mount hole × 2"
            }
        }
    }
    pub fn prompt_p359(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "📏 ノギスホルダー (wall-mount backplate + N slot)",
            Lang::En => "📏 Caliper holder (wall-mount backplate + N slot)",
        }
    }
    pub fn prompt_p360(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ノギス長 (mm):",
            Lang::En => "caliper length (mm):",
        }
    }
    pub fn prompt_p361(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "throat 深さ (mm):",
            Lang::En => "throat depth (mm):",
        }
    }
    pub fn prompt_p362(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "収納個数:",
            Lang::En => "storage count:",
        }
    }
    pub fn prompt_p363(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "ノギスホルダー L{} × throat {}mm × {} 個",
            Lang::En => "Caliper holder L{} × throat {}mm × {} pcs",
        }
    }
    pub fn prompt_p364(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: standard 3 (Mitutoyo 150mm digital) / large 6 (200mm digital)"
            }
            Lang::En => "Presets: standard 3 (Mitutoyo 150mm digital) / large 6 (200mm digital)",
        }
    }
    pub fn prompt_p365(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: slot 幅 15mm、backplate 5mm 厚、4 隅 M4 mount hole",
            Lang::En => "Fixed: slot width 15mm, backplate 5mm thick, 4-corner M4 mount hole",
        }
    }
    pub fn prompt_p366(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "📎 袋クリップ整理 (縦 slot rack)",
            Lang::En => "📎 Bag-clip organizer (vertical slot rack)",
        }
    }
    pub fn prompt_p367(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "袋クリップ {} slot × W{} × H{}mm",
            Lang::En => "Bag clip {} slot × W{} × H{}mm",
        }
    }
    pub fn prompt_p368(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "プリセット目安: standard (8 × W8 × H100) / large (12 × W10 × H120)",
            Lang::En => "Presets: standard (8 × W8 × H100) / large (12 × W10 × H120)",
        }
    }
    pub fn prompt_p369(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: slot 奥行 30mm、wall 2.5mm、floor 3mm",
            Lang::En => "Fixed: slot depth 30mm, wall 2.5mm, floor 3mm",
        }
    }
    pub fn prompt_p370(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🥫 缶ラック (gravity feed tilted shelf、multi-tier)",
            Lang::En => "🥫 Can rack (gravity-feed tilted shelf, multi-tier)",
        }
    }
    pub fn prompt_p371(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "段数:",
            Lang::En => "tier count:",
        }
    }
    pub fn prompt_p372(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "缶直径 (mm):",
            Lang::En => "can diameter (mm):",
        }
    }
    pub fn prompt_p373(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "傾斜角 (deg):",
            Lang::En => "tilt (deg):",
        }
    }
    pub fn prompt_p374(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "缶ラック {} tier × Ø{}mm × {}deg tilt",
            Lang::En => "Can rack {} tier × Ø{}mm × {}deg tilt",
        }
    }
    pub fn prompt_p375(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: standard (2 tier × Coke 350ml Ø66 × 10deg) / short can (3 tier × Ø55 × 12deg)"
            }
            Lang::En => {
                "Presets: standard (2 tier × Coke 350ml Ø66 × 10deg) / short can (3 tier × Ø55 × 12deg)"
            }
        }
    }
    pub fn prompt_p376(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: 6 缶/段、shelf 3mm、side wall 3mm、front lip 15mm",
            Lang::En => "Fixed: 6 cans/tier, shelf 3mm, side wall 3mm, front lip 15mm",
        }
    }
    pub fn prompt_p377(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "💡 LED hub 筐体 (raspi_case + LED window + antenna hole)",
            Lang::En => "💡 LED hub enclosure (raspi_case + LED window + antenna hole)",
        }
    }
    pub fn prompt_p378(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "LED hub 筐体 内部 W{}×D{}×H{}mm",
            Lang::En => "LED hub enclosure inner W{}×D{}×H{}mm",
        }
    }
    pub fn prompt_p379(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: standard smart hub (80×60×30) / large IoT gateway (120×80×50)"
            }
            Lang::En => "Presets: standard smart hub (80×60×30) / large IoT gateway (120×80×50)",
        }
    }
    pub fn prompt_p380(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "固定: 壁 3mm、LED window 40×15mm (front)、antenna Ø12mm (top-right corner)"
            }
            Lang::En => {
                "Fixed: wall 3mm, LED window 40×15mm (front), antenna Ø12mm (top-right corner)"
            }
        }
    }
    pub fn prompt_p381(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "💄 メイク整理 (2D grid multi-cell)",
            Lang::En => "💄 Makeup organizer (2D grid multi-cell)",
        }
    }
    pub fn prompt_p382(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "cell 一辺 (mm):",
            Lang::En => "cell side (mm):",
        }
    }
    pub fn prompt_p383(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "メイク整理 {}×{} × □{}mm",
            Lang::En => "Makeup organizer {}×{} × □{}mm",
        }
    }
    pub fn prompt_p384(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "プリセット目安: standard (3×4 × 45) / small palette (2×3 × 60) / brush rack (4×6 × 30)"
            }
            Lang::En => {
                "Presets: standard (3×4 × 45) / small palette (2×3 × 60) / brush rack (4×6 × 30)"
            }
        }
    }
    pub fn prompt_p385(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "固定: cell 深 40mm、wall 2mm、floor 2.5mm",
            Lang::En => "Fixed: cell depth 40mm, wall 2mm, floor 2.5mm",
        }
    }
    pub fn prompt_p386(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🖥 VESA マウント板 (75/100 規格、4 隅穴)",
            Lang::En => "🖥 VESA mount plate (75/100 spec, 4-corner hole)",
        }
    }
    pub fn prompt_p387(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "板厚 (mm):",
            Lang::En => "plate thickness (mm):",
        }
    }
    pub fn prompt_p388(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "穴規格 M:",
            Lang::En => "hole spec M:",
        }
    }
    pub fn prompt_p389(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "VESA {}×{} M{} 座ぐり (板厚 {}mm)",
            Lang::En => "VESA {}×{} M{} counterbore (plate {}mm)",
        }
    }
    pub fn prompt_p390(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🔩 L 型ブラケット (両 arm ネジ穴列)",
            Lang::En => "🔩 L-bracket (screw holes on both arms)",
        }
    }
    pub fn prompt_p391(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "水平 arm 長 (mm):",
            Lang::En => "horizontal arm length (mm):",
        }
    }
    pub fn prompt_p392(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "垂直 arm 高 (mm):",
            Lang::En => "vertical arm height (mm):",
        }
    }
    pub fn prompt_p393(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "穴数/arm:",
            Lang::En => "holes/arm:",
        }
    }
    pub fn prompt_p394(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "L型ブラケット M{}×{}穴 ({}×{}×{}mm)",
            Lang::En => "L bracket M{}×{} holes ({}×{}×{}mm)",
        }
    }
    pub fn prompt_p395(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🥧 Raspberry Pi マウント板 (M2.5 pattern)",
            Lang::En => "🥧 Raspberry Pi mount plate (M2.5 pattern)",
        }
    }
    pub fn prompt_p396(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "VESA 外周穴数:",
            Lang::En => "VESA outer hole count:",
        }
    }
    pub fn prompt_p397(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "0=なし, 4=VESA 対応",
            Lang::En => "0=none, 4=VESA",
        }
    }
    pub fn prompt_p398(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Raspberry Pi マウント板 model={} extras={}",
            Lang::En => "Raspberry Pi mount plate model={} extras={}",
        }
    }
    pub fn prompt_p399(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "⚙ フランジマウント (円形、PCD 上に穴)",
            Lang::En => "⚙ Flange mount (round, holes on PCD)",
        }
    }
    pub fn prompt_p400(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "外径 (mm):",
            Lang::En => "outer diameter (mm):",
        }
    }
    pub fn prompt_p401(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "穴個数:",
            Lang::En => "hole count:",
        }
    }
    pub fn prompt_p402(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🪵 アリ継ぎ (10° テーパー、male/female)",
            Lang::En => "🪵 Dovetail joint (10° taper, male/female)",
        }
    }
    pub fn prompt_p403(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "底辺幅 (mm):",
            Lang::En => "base width (mm):",
        }
    }
    pub fn prompt_p404(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "深さ (mm):",
            Lang::En => "depth (mm):",
        }
    }
    pub fn prompt_p405(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "性別:",
            Lang::En => "gender:",
        }
    }
    pub fn prompt_p406(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "アリ継ぎ {} ({}×{}×{}mm)",
            Lang::En => "Dovetail {} ({}×{}×{}mm)",
        }
    }
    pub fn prompt_p407(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "長さ (mm):",
            Lang::En => "length (mm):",
        }
    }
    pub fn prompt_p408(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "梁長 (mm):",
            Lang::En => "beam length (mm):",
        }
    }
    pub fn prompt_p409(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "梁幅 (mm):",
            Lang::En => "beam width (mm):",
        }
    }
    pub fn prompt_p410(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "梁厚 (mm):",
            Lang::En => "beam thickness (mm):",
        }
    }
    pub fn prompt_p411(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "hook 高 (mm):",
            Lang::En => "hook height (mm):",
        }
    }
    pub fn prompt_p412(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🏛 Boss array (ネジ受け柱の格子)",
            Lang::En => "🏛 Boss array (grid of screw-receiving posts)",
        }
    }
    pub fn prompt_p413(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "boss 高 (mm):",
            Lang::En => "boss height (mm):",
        }
    }
    pub fn prompt_p414(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "⚙ 軸受マウント板 (608ZZ/688ZZ/6001/6202)",
            Lang::En => "⚙ Bearing mount plate (608ZZ/688ZZ/6001/6202)",
        }
    }
    pub fn prompt_p415(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "軸受マウント Ø{} 板厚{}mm style{}",
            Lang::En => "Bearing mount Ø{} plate {}mm style {}",
        }
    }
    pub fn prompt_p416(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🕳 配線通しグロメット (デスク板穴、家具 flat-pack)",
            Lang::En => "🕳 Cable pass-through grommet (desk hole, flat-pack furniture)",
        }
    }
    pub fn prompt_p417(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🪟 カーテンレール壁掛けブラケット",
            Lang::En => "🪟 Curtain rod wall bracket",
        }
    }
    pub fn prompt_p418(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "rod 径 (mm):",
            Lang::En => "rod diameter (mm):",
        }
    }
    pub fn prompt_p419(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "壁からの突出 (mm):",
            Lang::En => "wall protrusion (mm):",
        }
    }
    pub fn prompt_p420(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "カーテンブラケット Ø{} 突出{}mm",
            Lang::En => "Curtain bracket Ø{} protrusion {}mm",
        }
    }
    pub fn prompt_p421(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🤖 Arduino マウント板 (Uno/Mega/Nano)",
            Lang::En => "🤖 Arduino mount plate (Uno/Mega/Nano)",
        }
    }
    pub fn prompt_p422(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "VESA 外周穴:",
            Lang::En => "VESA outer holes:",
        }
    }
    pub fn prompt_p423(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "Arduino {} マウント板 (extras={})",
            Lang::En => "Arduino {} mount plate (extras={})",
        }
    }
    pub fn prompt_p424(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "⚙ サーボマウント (SG90 / MG996R)",
            Lang::En => "⚙ Servo mount (SG90 / MG996R)",
        }
    }
    pub fn prompt_p425(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "サーボマウント {}",
            Lang::En => "Servo mount {}",
        }
    }

    // ─── main.rs header fmt fns (Phase 4) ──────────────────────
    pub fn header_usage_uncapped(
        tier: impl std::fmt::Debug,
        usage: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("{:?} | {} 回", tier, usage),
            Lang::En => format!("{:?} | {} runs", tier, usage),
        }
    }

    // ─── prompt.rs fmt fns (Phase 3, String-returning) ─────────
    pub fn prompt_fmt_usage_beta(usage: impl std::fmt::Display, l: Lang) -> String {
        match l {
            Lang::Ja => format!("本日の生成: {usage} 回 (β 制限なし)", usage = usage),
            Lang::En => format!("Today: {usage} runs (β no cap)", usage = usage),
        }
    }
    pub fn prompt_fmt_usage_limit(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("本日の生成: {} / {} 回", a, b),
            Lang::En => format!("Today: {} / {} runs", a, b),
        }
    }
    pub fn prompt_fmt_verts_tris(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("頂点数: {} / 三角形数: {}", a, b),
            Lang::En => format!("Vertices: {} / Triangles: {}", a, b),
        }
    }
    pub fn prompt_fmt_saved_to(a: impl std::fmt::Display, l: Lang) -> String {
        match l {
            Lang::Ja => format!("保存先: {}", a),
            Lang::En => format!("Saved to: {}", a),
        }
    }
    pub fn prompt_fmt_overhang(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        d: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!(
                "オーバーハング: {:.1}% ({} / {} 面) 最大壁角 {:.1}°",
                a, b, c, d
            ),
            Lang::En => format!(
                "Overhang: {:.1}% ({} / {} faces) max wall angle {:.1}°",
                a, b, c, d
            ),
        }
    }
    pub fn prompt_fmt_gcode(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("G-code: {} 層 / 推定 {} 分 / フィラメント {:.2} m", a, b, c),
            Lang::En => format!("G-code: {} layers / est {} min / filament {:.2} m", a, b, c),
        }
    }
    pub fn prompt_fmt_safety(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("安全性 ({}): {} / 反り {}", a, b, c),
            Lang::En => format!("Safety ({}): {} / warp {}", a, b, c),
        }
    }
    pub fn prompt_fmt_error_msg(msg: impl std::fmt::Display, l: Lang) -> String {
        match l {
            Lang::Ja => format!("エラー: {msg}", msg = msg),
            Lang::En => format!("Error: {msg}", msg = msg),
        }
    }
    pub fn prompt_fmt_export_failed(err: impl std::fmt::Display, l: Lang) -> String {
        match l {
            Lang::Ja => format!("エクスポート失敗: {err}", err = err),
            Lang::En => format!("Export failed: {err}", err = err),
        }
    }
    pub fn prompt_fmt_model_dl_error(e: impl std::fmt::Display, l: Lang) -> String {
        match l {
            Lang::Ja => format!("モデルDLエラー: {e}", e = e),
            Lang::En => format!("Model DL error: {e}", e = e),
        }
    }
    pub fn prompt_fmt_sidecar_start_fail(msg: impl std::fmt::Display, l: Lang) -> String {
        match l {
            Lang::Ja => format!("LLM sidecar 起動失敗: {msg}", msg = msg),
            Lang::En => format!("LLM sidecar start failed: {msg}", msg = msg),
        }
    }
    pub fn prompt_fmt_phase_running(
        phase_label: impl std::fmt::Display,
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!(
                "{phase_label} 実行中... 経過 {:02}:{:02}",
                a,
                b,
                phase_label = phase_label
            ),
            Lang::En => format!(
                "{phase_label} running... elapsed {:02}:{:02}",
                a,
                b,
                phase_label = phase_label
            ),
        }
    }
    pub fn prompt_fmt_color4_fail(err: impl std::fmt::Display, l: Lang) -> String {
        match l {
            Lang::Ja => format!("4色エクスポート失敗: {err}", err = err),
            Lang::En => format!("4-color export failed: {err}", err = err),
        }
    }
    pub fn prompt_fmt_color4_done(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("生成完了: {} 色 / {} ファイル", a, b),
            Lang::En => format!("Complete: {} colors / {} files", a, b),
        }
    }
    pub fn prompt_fmt_image_load_fail(e: impl std::fmt::Display, l: Lang) -> String {
        match l {
            Lang::Ja => format!("画像読み込み失敗: {e}", e = e),
            Lang::En => format!("Image load failed: {e}", e = e),
        }
    }
    pub fn prompt_fmt_unsupported_suffix(a: impl std::fmt::Display, l: Lang) -> String {
        match l {
            Lang::Ja => format!("{} (未対応)", a),
            Lang::En => format!("{} (unsupported)", a),
        }
    }
    pub fn prompt_fmt_template_gen_fail(e: impl std::fmt::Display, l: Lang) -> String {
        match l {
            Lang::Ja => format!("テンプレート生成失敗: {e}", e = e),
            Lang::En => format!("Template generation failed: {e}", e = e),
        }
    }
    pub fn prompt_fmt_outer_dim_3f(
        ext_x_mm: impl std::fmt::Display,
        ext_y_mm: impl std::fmt::Display,
        ext_h_mm: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!(
                "外形寸法: {ext_x_mm:.1} × {ext_y_mm:.1} × {ext_h_mm:.1}mm",
                ext_x_mm = ext_x_mm,
                ext_y_mm = ext_y_mm,
                ext_h_mm = ext_h_mm
            ),
            Lang::En => format!(
                "Outer: {ext_x_mm:.1} × {ext_y_mm:.1} × {ext_h_mm:.1}mm",
                ext_x_mm = ext_x_mm,
                ext_y_mm = ext_y_mm,
                ext_h_mm = ext_h_mm
            ),
        }
    }
    pub fn prompt_fmt_sticky_note_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("付箋ホルダー {}×{}×{}mm", a, b, c),
            Lang::En => format!("Sticky-note holder {}×{}×{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_capacity_cards(capacity: impl std::fmt::Display, l: Lang) -> String {
        match l {
            Lang::Ja => format!("収納枚数目安: 約 {capacity} 枚", capacity = capacity),
            Lang::En => format!("Approx capacity: {capacity} cards", capacity = capacity),
        }
    }
    pub fn prompt_fmt_business_card_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("名刺ホルダー {}×{}mm ({}枚)", a, b, c),
            Lang::En => format!("Business-card holder {}×{}mm ({} cards)", a, b, c),
        }
    }
    pub fn prompt_fmt_pen_cup_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("ペン立て Ø{}×{}mm", a, b),
            Lang::En => format!("Pen cup Ø{}×{}mm", a, b),
        }
    }
    pub fn prompt_fmt_pen_cup_outer(outer_dia: impl std::fmt::Display, l: Lang) -> String {
        match l {
            Lang::Ja => format!("外形 Ø{outer_dia:.1}mm (壁厚 2mm)", outer_dia = outer_dia),
            Lang::En => format!("Outer Ø{outer_dia:.1}mm (2mm wall)", outer_dia = outer_dia),
        }
    }
    pub fn prompt_fmt_headphone_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("ヘッドホンホルダー arm{}mm×hb{}mm×mount{}mm", a, b, c),
            Lang::En => format!("Headphone holder arm{}mm × hb{}mm × mount{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_under_desk_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        screw_note: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!(
                "机下 mount desk{}mm × clamp{}mm ({screw_note})",
                a,
                b,
                screw_note = screw_note
            ),
            Lang::En => format!(
                "Under-desk mount desk{}mm × clamp{}mm ({screw_note})",
                a,
                b,
                screw_note = screw_note
            ),
        }
    }
    pub fn prompt_fmt_shelf_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("シェルフ {}×{}mm × 脚{}mm", a, b, c),
            Lang::En => format!("Shelf {}×{}mm × leg {}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_monitor_riser_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("モニターライザー {}×{}×{}mm", a, b, c),
            Lang::En => format!("Monitor riser {}×{}×{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_coaster_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("コースター Ø{}×{}mm", a, b),
            Lang::En => format!("Coaster Ø{}×{}mm", a, b),
        }
    }
    pub fn prompt_fmt_tissue_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("ティッシュカバー 内 {}×{}×{}mm", a, b, c),
            Lang::En => format!("Tissue cover inner {}×{}×{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_storage_box_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("収納 BOX 内 {}×{}×{}mm", a, b, c),
            Lang::En => format!("Storage box inner {}×{}×{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_cable_clip_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("ケーブルクリップ Ø{}×L{}mm", a, b),
            Lang::En => format!("Cable clip Ø{}×L{}mm", a, b),
        }
    }
    pub fn prompt_fmt_card_tray_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("カードトレー {}×{}×深{}mm", a, b, c),
            Lang::En => format!("Card tray {}×{}× depth {}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_token_well_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("トークン井戸 Ø{}×深{}mm × {}", a, b, c),
            Lang::En => format!("Token well Ø{}× depth {}mm × {}", a, b, c),
        }
    }
    pub fn prompt_fmt_wrench_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("レンチホルダー {}-{}mm × {}", a, b, c),
            Lang::En => format!("Wrench holder {}-{}mm × {}", a, b, c),
        }
    }
    pub fn prompt_fmt_socket_rail_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("ソケットレール Ø{}×H{}mm × {}", a, b, c),
            Lang::En => format!("Socket rail Ø{}×H{}mm × {}", a, b, c),
        }
    }
    pub fn prompt_fmt_bit_holder_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("ビットホルダー {}×{} @ {}mm", a, b, c),
            Lang::En => format!("Bit holder {}×{} @ {}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_rpi_case_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("RPi ケース {}×{}×{}mm", a, b, c),
            Lang::En => format!("RPi case {}×{}×{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_mcu_case_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("MCU ケース {}×{}×{}mm", a, b, c),
            Lang::En => format!("MCU case {}×{}×{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_toothbrush_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("歯ブラシホルダー {} × Ø{}×H{}mm", a, b, c),
            Lang::En => format!("Toothbrush holder {} × Ø{}×H{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_pliers_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("プライヤーラック {} × W{}×D{}mm", a, b, c),
            Lang::En => format!("Pliers rack {} × W{}×D{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_egg_tray_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("卵トレー {}×{} × 深{}mm", a, b, c),
            Lang::En => format!("Egg tray {}×{} × depth {}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_utensil_caddy_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("ツールキャディ {} × Ø{}×H{}mm", a, b, c),
            Lang::En => format!("Tool caddy {} × Ø{}×H{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_spool_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("スプールホルダー Ø{}×W{}×bore{}mm", a, b, c),
            Lang::En => format!("Spool holder Ø{}×W{}×bore{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_nozzle_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("ノズルホルダー {} hole × Ø{}×D{}mm", a, b, c),
            Lang::En => format!("Nozzle holder {} hole × Ø{}×D{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_plate_rack_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("プレートラック {} slot × spacing {}mm × H{}mm", a, b, c),
            Lang::En => format!("Plate rack {} slot × spacing {}mm × H{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_cutlery_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("カトラリートレー {} slot × W{}×L{}mm", a, b, c),
            Lang::En => format!("Cutlery tray {} slot × W{}×L{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_pill_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("薬箱 {}×{} × cell {}mm", a, b, c),
            Lang::En => format!("Pill box {}×{} × cell {}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_magnetic_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("マグネットバー {} × Ø{} spacing {}mm", a, b, c),
            Lang::En => format!("Magnet bar {} × Ø{} spacing {}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_hairdryer_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("ドライヤーホルダー Ø{}×D{}mm", a, b),
            Lang::En => format!("Dryer holder Ø{}×D{}mm", a, b),
        }
    }
    pub fn prompt_fmt_kcup_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("K-Cup ホルダー {}×{} × Ø{}mm", a, b, c),
            Lang::En => format!("K-Cup holder {}×{} × Ø{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_hexkey_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("ヘックスキーホルダー {}-{}mm × {}", a, b, c),
            Lang::En => format!("Hex-key holder {}-{}mm × {}", a, b, c),
        }
    }
    pub fn prompt_fmt_wrap_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("Wrap ホルダー Ø{}×W{}mm", a, b),
            Lang::En => format!("Wrap holder Ø{}×W{}mm", a, b),
        }
    }
    pub fn prompt_fmt_sock_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("靴下 divider {} cell × W{}×H{}mm", a, b, c),
            Lang::En => format!("Sock divider {} cell × W{}×H{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_soap_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("石鹸トレー L{}×W{}mm × {} drain", a, b, c),
            Lang::En => format!("Soap tray L{}×W{}mm × {} drain", a, b, c),
        }
    }
    pub fn prompt_fmt_razor_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("カミソリホルダー W{}×D{}mm × mount Ø{}", a, b, c),
            Lang::En => format!("Razor holder W{}×D{}mm × mount Ø{}", a, b, c),
        }
    }
    pub fn prompt_fmt_chopstick_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("箸ホルダー {} pair × W{}×L{}mm", a, b, c),
            Lang::En => format!("Chopstick holder {} pair × W{}×L{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_swatch_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("スウォッチホルダー {}×{} × W{}mm", a, b, c),
            Lang::En => format!("Swatch holder {}×{} × W{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_tp_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("TP ホルダー 内径Ø{} × W{}mm × 板{}mm", a, b, c),
            Lang::En => format!("TP holder inner Ø{} × W{}mm × plate {}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_sd_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("SD カードホルダー {}×{} × W{}mm", a, b, c),
            Lang::En => format!("SD card holder {}×{} × W{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_driver_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("ドライバーラック {} slot × Ø{} × H{}mm", a, b, c),
            Lang::En => format!("Driver rack {} slot × Ø{} × H{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_cotton_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("コットン ディスペンサー {} 個 × Ø{}mm × H{}mm", a, b, c),
            Lang::En => format!("Cotton dispenser {} pcs × Ø{}mm × H{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_sponge_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("スポンジホルダー L{} × W{}mm × {} drain", a, b, c),
            Lang::En => format!("Sponge holder L{} × W{}mm × {} drain", a, b, c),
        }
    }
    pub fn prompt_fmt_ip54_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("IP54 筐体 内部 W{}×D{}×H{}mm", a, b, c),
            Lang::En => format!("IP54 enclosure inner W{}×D{}×H{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_jewelry_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("ジュエリー スタンド {} tier × Ø{}mm × H{}mm", a, b, c),
            Lang::En => format!("Jewelry stand {} tier × Ø{}mm × H{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_phone_dock_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("充電ドック W{} × H{}mm × Ø{} 貫通", a, b, c),
            Lang::En => format!("Charging dock W{} × H{}mm × Ø{} pass-through", a, b, c),
        }
    }
    pub fn prompt_fmt_cutting_board_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("まな板ラック {} slot × W{}mm × H{}mm", a, b, c),
            Lang::En => format!("Cutting-board rack {} slot × W{}mm × H{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_tape_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("テープ dispenser Ø{} × W{} × wall{}mm", a, b, c),
            Lang::En => format!("Tape dispenser Ø{} × W{} × wall {}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_shower_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("シャワー棚 {} tier × L{} × D{}mm", a, b, c),
            Lang::En => format!("Shower shelf {} tier × L{} × D{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_caliper_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("ノギスホルダー L{} × throat {}mm × {} 個", a, b, c),
            Lang::En => format!("Caliper holder L{} × throat {}mm × {} pcs", a, b, c),
        }
    }
    pub fn prompt_fmt_bagclip_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("袋クリップ {} slot × W{} × H{}mm", a, b, c),
            Lang::En => format!("Bag clip {} slot × W{} × H{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_can_rack_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("缶ラック {} tier × Ø{}mm × {}deg tilt", a, b, c),
            Lang::En => format!("Can rack {} tier × Ø{}mm × {}deg tilt", a, b, c),
        }
    }
    pub fn prompt_fmt_led_hub_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("LED hub 筐体 内部 W{}×D{}×H{}mm", a, b, c),
            Lang::En => format!("LED hub enclosure inner W{}×D{}×H{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_makeup_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("メイク整理 {}×{} × □{}mm", a, b, c),
            Lang::En => format!("Makeup organizer {}×{} × □{}mm", a, b, c),
        }
    }
    pub fn prompt_fmt_vesa_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        d: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("VESA {}×{} M{} 座ぐり (板厚 {}mm)", a, b, c, d),
            Lang::En => format!("VESA {}×{} M{} counterbore (plate {}mm)", a, b, c, d),
        }
    }
    pub fn prompt_fmt_l_bracket_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        d: impl std::fmt::Display,
        e: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("L型ブラケット M{}×{}穴 ({}×{}×{}mm)", a, b, c, d, e),
            Lang::En => format!("L-bracket M{}×{} holes ({}×{}×{}mm)", a, b, c, d, e),
        }
    }
    pub fn prompt_fmt_raspi_mount_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("Raspberry Pi マウント板 model={} extras={}", a, b),
            Lang::En => format!("Raspberry Pi mount plate model={} extras={}", a, b),
        }
    }
    pub fn prompt_fmt_dovetail_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        d: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("アリ継ぎ {} ({}×{}×{}mm)", a, b, c, d),
            Lang::En => format!("Dovetail {} ({}×{}×{}mm)", a, b, c, d),
        }
    }
    pub fn prompt_fmt_bearing_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        c: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("軸受マウント Ø{} 板厚{}mm style{}", a, b, c),
            Lang::En => format!("Bearing mount Ø{} plate {}mm style {}", a, b, c),
        }
    }
    pub fn prompt_fmt_curtain_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("カーテンブラケット Ø{} 突出{}mm", a, b),
            Lang::En => format!("Curtain bracket Ø{} protrusion {}mm", a, b),
        }
    }
    pub fn prompt_fmt_arduino_label(
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
        l: Lang,
    ) -> String {
        match l {
            Lang::Ja => format!("Arduino {} マウント板 (extras={})", a, b),
            Lang::En => format!("Arduino {} mount plate (extras={})", a, b),
        }
    }
    pub fn prompt_fmt_servo_label(a: impl std::fmt::Display, l: Lang) -> String {
        match l {
            Lang::Ja => format!("サーボマウント {}", a),
            Lang::En => format!("Servo mount {}", a),
        }
    }
    pub fn prompt_lit_sidecar_help_msg(l: Lang) -> &'static str {
        match l {
            Lang::Ja => {
                "対処: `cargo install --path ~/ALICE-LLM --features server` で alice-llm-server を PATH に配置、または Settings の Endpoint に 既存の OpenAI 互換 endpoint (例: Ollama) を指定してください"
            }
            Lang::En => {
                "Action: run `cargo install --path ~/ALICE-LLM --features server` to place alice-llm-server in PATH, or set Settings > Endpoint to an existing OpenAI-compatible endpoint (e.g. Ollama)"
            }
        }
    }

    pub fn prompt_fmt_llm_invalid_dsl(clean_err: impl std::fmt::Display, l: Lang) -> String {
        match l {
            Lang::Ja => format!(
                "LLM が有効な LOL DSL を生成できませんでした ({clean_err})\n\n\
                 対処: プロンプトを短く / 具体的に書き直すか、テンプレート / \
                 カスタマイザーをお使いください (LLM 経路より高速で確実)",
                clean_err = clean_err,
            ),
            Lang::En => format!(
                "The LLM could not produce valid LOL DSL ({clean_err})\n\n\
                 Action: shorten or clarify the prompt, or use the Templates / \
                 Customizer path (faster and more reliable than the LLM path)",
                clean_err = clean_err,
            ),
        }
    }

    pub fn prompt_hex_bit_holder_header(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "🔧 ヘックスビットホルダー (grid 状 hex hole、1/4\" bit 想定)",
            Lang::En => "🔧 Hex-bit holder (grid of hex holes, 1/4\" bit)",
        }
    }

    pub fn prompt_create(l: Lang) -> &'static str {
        match l {
            Lang::Ja => "作成",
            Lang::En => "Create",
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
