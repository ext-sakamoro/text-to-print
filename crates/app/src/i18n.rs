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
