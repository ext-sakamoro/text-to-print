use egui::Ui;
use serde::{Deserialize, Serialize};

use crate::state::AppState;
use text_to_print_core::db::LlmProviderConfigRow;
use text_to_print_core::keychain;
use text_to_print_core::license::{LicenseKey, LicenseVerifier};
use text_to_print_core::tier::Tier;
use text_to_print_llm::backend_kind::{BackendKind, ExecutionMode};
use text_to_print_llm::model::ModelChoice;
use text_to_print_llm::openai_compat_backend::{
    OpenAiCompatBackend, OpenAiCompatConfig, OpenAiCompatProvider,
};

pub const LICENSE_PUBLIC_KEY: [u8; 32] = [
    0x05, 0x7f, 0x9c, 0x5f, 0xdb, 0x3f, 0x6f, 0x93, 0x69, 0x11, 0xd9, 0x16, 0x85, 0xb1, 0x82, 0x5b,
    0xcb, 0x3a, 0x75, 0xff, 0x18, 0x39, 0x12, 0xd5, 0x26, 0x6e, 0xf4, 0x75, 0x34, 0xec, 0xae, 0xc2,
];

/// Backend checkout endpoint override (dev use `TTP_CHECKOUT_ENDPOINT=http://localhost:8787/stripe/checkout-session`)
pub const DEFAULT_CHECKOUT_ENDPOINT: &str =
    "https://text-to-print.alicelaw.net/stripe/checkout-session";

/// Enterprise inquiry destination (mailto:) opened by the "Contact for
/// Enterprise" button in Settings
pub const ENTERPRISE_MAILTO: &str =
    "mailto:enterprise@alicelaw.net?subject=text-to-print%20Enterprise%20plan";

#[derive(Default)]
pub struct SettingsState {
    pub license_input: String,
    pub license_message: Option<(String, bool)>,
    /// Email typed into the Upgrade to Pro form (persisted across the
    /// session but not to DB — user re-enters after re-open)
    pub checkout_email: String,
    /// Latest checkout attempt outcome for UI feedback
    pub checkout_message: Option<(String, bool)>,
    /// 2026-08-23 Network Settings — preset endpoint input buffer (空 = default)
    pub presets_endpoint_input: String,
    /// 2026-08-23 Network Settings — sidecar port input buffer (string 経由で
    /// TextEdit と bind、save 時に u16 parse)
    pub sidecar_port_input: String,
    /// 2026-08-23 Network Settings — save 結果 message (ok=true / error=false)
    pub network_message: Option<(String, bool)>,
    /// 2026-08-23 Network Settings 初期 load 済みフラグ (SettingsState 生成後
    /// 1 回だけ DB から値を pull、以降は user 編集値を保持)
    pub network_loaded: bool,
    /// BYO LLM (2026-08-23): form buffer for the currently-being-edited
    /// OpenAI-compat provider See [`ByoLlmSettings`] for field docs
    pub byo_llm: ByoLlmSettings,
}

/// UI form state for the BYO LLM (bring-your-own OpenAI-compat provider)
/// section of Settings The runtime backend lives in
/// [`AppState::openai_compat`]; this struct only holds unsaved edits
///
/// The `form_api_key` field is cleared after a successful save so the
/// key never lingers in application memory beyond the Keychain
#[derive(Default)]
pub struct ByoLlmSettings {
    /// Which provider preset is being edited in the form Independent of
    /// `state.db.get_openai_compat_active_provider()` (the currently
    /// active provider for generation)
    pub form_provider: Option<OpenAiCompatProvider>,
    pub form_endpoint: String,
    pub form_model: String,
    /// TextEdit-bound string; parsed to `u32` on save
    pub form_max_tokens: String,
    /// TextEdit-bound string; parsed to `f32` on save
    pub form_temperature: String,
    /// `"minimal"` (OpenAI) / `"none"` (Gemini) / empty (Anthropic /
    /// Custom) Blank string maps to `None` (omitted from request body)
    pub form_reasoning_effort: String,
    /// API key input Password-style widget (masked), cleared after save
    /// Never persisted to disk — only forwarded to OS Keychain
    pub form_api_key: String,
    /// Persist / test result message (green = success, red = error)
    pub message: Option<(String, bool)>,
    /// Cached list of already-configured provider slugs (from
    /// `list_llm_provider_configs`) Used to render "quick-load" buttons
    /// so the user can switch between saved provider configs
    pub configured_providers: Vec<String>,
    /// Which provider is currently the active generation target Loaded
    /// from `profiles.openai_compat_active_provider`
    pub active_provider: String,
    /// One-shot init flag SettingsState is reused across UI frames;
    /// this ensures we hit the DB once at startup, not on every repaint
    pub loaded: bool,
}

/// Wire format for `POST /stripe/checkout-session` (must match
/// `text_to_print_worker::checkout::CheckoutRequest`)
#[derive(Debug, Clone, Serialize)]
pub struct CheckoutRequestBody {
    pub plan: String,
    pub user_email: String,
}

/// Wire format for the response (must match
/// `text_to_print_worker::checkout::CheckoutResponse`)
#[derive(Debug, Clone, Deserialize)]
pub struct CheckoutResponseBody {
    pub url: String,
    /// Retained for future subscription-status polling; currently we
    /// only open `url` in the browser and let Stripe drive the flow
    #[serde(default)]
    #[allow(dead_code)]
    pub session_id: String,
}

/// Returns the checkout endpoint URL, honoring `TTP_CHECKOUT_ENDPOINT`
/// for local dev against `wrangler dev` on localhost:8787
pub fn checkout_endpoint() -> String {
    std::env::var("TTP_CHECKOUT_ENDPOINT").unwrap_or_else(|_| DEFAULT_CHECKOUT_ENDPOINT.to_string())
}

/// Client-side validation for the checkout email — matches the loose
/// server-side rule in `text_to_print_worker::checkout::handle` so the
/// UI can reject obvious typos before the round trip
pub fn is_plausible_email(input: &str) -> bool {
    let s = input.trim();
    !s.is_empty() && s.contains('@') && s.len() <= 320 && !s.contains(' ')
}

pub fn show(ui: &mut Ui, state: &mut AppState, settings: &mut SettingsState) {
    ui.heading("Settings");
    ui.separator();

    // ライセンス / サブスクリプション
    ui.collapsing("License / Subscription", |ui| {
        // 現在の tier 表示 (Free / Pro / Enterprise / General)
        ui.horizontal(|ui| {
            ui.label("現在のプラン:");
            let (label, color) = match state.tier {
                Tier::Free => ("Free (LoRA share あり)", egui::Color32::LIGHT_GRAY),
                Tier::General => ("General", egui::Color32::LIGHT_BLUE),
                Tier::Pro => ("Pro", egui::Color32::GREEN),
                Tier::Enterprise => ("Enterprise", egui::Color32::GOLD),
            };
            ui.colored_label(color, label);
        });
        ui.colored_label(
            ui.style().visuals.warn_fg_color,
            "BETA バージョンのためプランを選択することができません",
        );

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(6.0);

        // ── Upgrade to Pro (Free tier のみ表示) ────────────────
        //
        // v0.1.0 β release では Paid tier UI は disabled 表示 (Coming
        // soon) Stripe backend deploy (Phase S3) 完了後に PAID_UI_ENABLED
        // を true に変えるだけで購入 flow 復活 Enterprise mailto と
        // License key 入力欄は generalized use case なので β 段階でも
        // 有効化 (既存 license holder が activate できる経路を残す)
        const PAID_UI_ENABLED: bool = false;
        if matches!(state.tier, Tier::Free) {
            ui.label(egui::RichText::new("Upgrade to Pro").strong());
            if PAID_UI_ENABLED {
                ui.label("Pro プランは無制限生成 + 完全 offline (LoRA 共有 OFF 強制)");
            } else {
                ui.colored_label(
                    ui.style().visuals.warn_fg_color,
                    "Pro subscription is coming in v0.2.0 (Beta では unavailable)",
                );
                ui.label(
                    "計画: 個人向け Pro プラン ¥3,000/月 or ¥30,000/年 (完全 offline + 無制限生成)",
                );
            }
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label("Email:");
                ui.add_enabled(
                    PAID_UI_ENABLED,
                    egui::TextEdit::singleline(&mut settings.checkout_email),
                );
            });
            let email_ok = PAID_UI_ENABLED && is_plausible_email(&settings.checkout_email);

            ui.horizontal(|ui| {
                let monthly = ui
                    .add_enabled(email_ok, egui::Button::new("Buy Monthly ¥3,000/月"))
                    .on_disabled_hover_text("Coming soon in v0.2.0");
                if monthly.clicked() {
                    spawn_checkout(state, settings, "pro_monthly");
                }
                let yearly = ui
                    .add_enabled(email_ok, egui::Button::new("Buy Yearly ¥30,000/年 (-17%)"))
                    .on_disabled_hover_text("Coming soon in v0.2.0");
                if yearly.clicked() {
                    spawn_checkout(state, settings, "pro_yearly");
                }
            });

            if PAID_UI_ENABLED && !email_ok && !settings.checkout_email.is_empty() {
                ui.colored_label(
                    ui.style().visuals.warn_fg_color,
                    "有効な email 形式で入力してください",
                );
            }
            if let Some((msg, ok)) = &settings.checkout_message {
                let color = if *ok {
                    egui::Color32::LIGHT_BLUE
                } else {
                    egui::Color32::RED
                };
                ui.colored_label(color, msg);
            }

            ui.add_space(6.0);
            ui.label(egui::RichText::new("Enterprise plan").strong());
            ui.horizontal(|ui| {
                ui.label("複数ユーザー / 商用 / カスタム機能:");
                if ui.button("問合わせ").clicked()
                    && let Err(e) = open::that(ENTERPRISE_MAILTO)
                {
                    settings.checkout_message = Some((format!("メーラー起動失敗: {e}"), false));
                }
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(6.0);
        }

        // ── License key 入力 (受信 email から貼付) ───────────────
        ui.label(egui::RichText::new("ライセンスキー入力").strong());
        ui.label("Stripe 決済後に email で届いたキーを貼付してください");
        ui.add_space(2.0);
        ui.text_edit_multiline(&mut settings.license_input);

        ui.horizontal(|ui| {
            if ui.button("ライセンスを適用").clicked() && !settings.license_input.trim().is_empty()
            {
                apply_license(state, settings);
            }
            if !matches!(state.tier, Tier::Free)
                && ui.button("ライセンスをクリア (Free に戻す)").clicked()
            {
                clear_license(state, settings);
            }
        });

        if let Some((msg, success)) = &settings.license_message {
            let color = if *success {
                egui::Color32::GREEN
            } else {
                egui::Color32::RED
            };
            ui.colored_label(color, msg);
        }
    });

    ui.add_space(8.0);

    // LLM 設定
    ui.collapsing("LLM", |ui| {
        // Stage 3-C.6: inference backend picker
        // BYO LLM (2026-08-23): added OpenAiCompat variant for remote
        // API providers (OpenAI / Anthropic / Google / Ollama)
        ui.label("Inference backend:");
        let current_kind = state.backend_kind;
        let mut new_kind = current_kind;
        ui.vertical(|ui| {
            ui.selectable_value(
                &mut new_kind,
                BackendKind::Sidecar,
                BackendKind::Sidecar.label(),
            );
            ui.selectable_value(
                &mut new_kind,
                BackendKind::Embedded,
                BackendKind::Embedded.label(),
            );
            ui.selectable_value(
                &mut new_kind,
                BackendKind::OpenAiCompat,
                BackendKind::OpenAiCompat.label(),
            );
        });
        if new_kind != current_kind {
            state.switch_backend_kind(new_kind);
        }
        // Embedded load status (only meaningful when Embedded is selected)
        if state.backend_kind == BackendKind::Embedded {
            let status = state.embedded_status();
            ui.label(format!("Embedded 状態: {}", status.label()))
                .on_hover_text(
                    "Embedded は alice-llm を rlib 直リンクで実行します 初回選択時は GGUF ロードに ~30 秒 model DL 完了までは Loading 状態 生成 request は Ready 前は Sidecar にフォールバックします",
                );
            // Stage 3-C.12: CPU / GPU picker (only relevant to Embedded)
            ui.add_space(4.0);
            ui.label("Execution mode:");
            let prev_mode = state.execution_mode;
            let mut new_mode = prev_mode;
            ui.horizontal(|ui| {
                ui.selectable_value(&mut new_mode, ExecutionMode::Cpu, ExecutionMode::Cpu.label());
                ui.selectable_value(&mut new_mode, ExecutionMode::Gpu, ExecutionMode::Gpu.label());
            });
            if new_mode != prev_mode {
                state.switch_execution_mode(new_mode);
            }
            ui.label(format!("現在: {}", state.execution_mode.label()))
                .on_hover_text(
                    "CPU: Llama3Model 直呼び (mmap dequant on demand) GPU: wgpu backend (Metal / Vulkan / DX12) 経由 GpuModel 切替時は Embedded backend を再ロードします adapter 不在時は Error → 手動で CPU に戻して下さい",
                );

            // BYO LLM (2026-08-23): custom GGUF path override
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Custom GGUF (Model 選択より優先)").strong());
            let display_path = state
                .custom_gguf_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "(未設定、上の Model dropdown を使用)".to_string());
            ui.label(
                egui::RichText::new(display_path)
                    .small()
                    .color(if state.custom_gguf_path.is_some() {
                        egui::Color32::LIGHT_GREEN
                    } else {
                        ui.style().visuals.weak_text_color()
                    }),
            );
            ui.horizontal(|ui| {
                if ui
                    .button("GGUF ファイル選択")
                    .on_hover_text(
                        "HF DL を skip して指定 path から直接ロード \
                         qwen2.5-14b-instruct-q4_k_m.gguf 等の大型モデルを \
                         user 側で DL / 配置して使用",
                    )
                    .clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("GGUF", &["gguf"])
                        .pick_file()
                {
                    state.set_custom_gguf_path(Some(path));
                }
                if state.custom_gguf_path.is_some()
                    && ui
                        .button("クリア")
                        .on_hover_text("override 解除、上の Model dropdown に戻す")
                        .clicked()
                {
                    state.set_custom_gguf_path(None);
                }
            });
        }
        ui.add_space(6.0);

        ui.label("Endpoint (alice-llm-server):");
        ui.text_edit_singleline(&mut state.llm_config.endpoint);

        ui.add_space(4.0);
        ui.label("Model:");
        // Stage 3-C.9: capture the previous choice so we can detect a
        // change after the ComboBox mutates state and invoke the model-
        // swap hook (only meaningful in Embedded mode)
        let prev_choice = state.llm_config.model_choice;
        egui::ComboBox::from_id_salt("llm_model_choice")
            .selected_text(state.llm_config.model_choice.label())
            .show_ui(ui, |ui| {
                for choice in ModelChoice::all() {
                    ui.selectable_value(
                        &mut state.llm_config.model_choice,
                        *choice,
                        choice.label(),
                    );
                }
            });
        if state.llm_config.model_choice != prev_choice {
            state.on_model_choice_changed(prev_choice);
        }
        // Stage 3-C.17: surface manual-placement requirement for the
        // Bonsai variant (HF repo not public yet) The download flow
        // still tries but 404s; this label tells the user why and where
        // to place the file if they have it
        if state.llm_config.model_choice.requires_manual_placement() {
            ui.colored_label(
                ui.style().visuals.warn_fg_color,
                format!(
                    "手動配置要: HF repo 非公開のため {} を models_dir に配置",
                    state.llm_config.model_choice.default_filename()
                ),
            )
            .on_hover_text(
                "PrismML fork Q1_0 (128-element binary ternary) の Bonsai 27B は現在 HF 非公開 GGUF ファイルを手動で models/bonsai-27b-q1_0.gguf に配置すると Embedded backend が拾います (Stage 3-C.9 の model_exists 経路)",
            );
        }

        ui.add_space(4.0);
        ui.label(format!("Temperature: {:.1}", state.llm_config.temperature));
        ui.add(egui::Slider::new(
            &mut state.llm_config.temperature,
            0.0..=2.0,
        ));

        // Stage 3-C.14: LOL GBNF grammar-constrained decoding toggle
        ui.add_space(6.0);
        let mut enforce = state.enforce_lol_grammar;
        if ui
            .checkbox(&mut enforce, "LOL DSL grammar 強制 (GBNF)")
            .on_hover_text(
                "オンにすると生成 request に text_to_print_llm::grammar_lol::LOL_GBNF (253 行) を付随して送信し、alice-llm 側で mask_logits_by_grammar を毎 token 適用します 出力は parse_lol でパース保証 (シンタックス誤り 0) オフにすると free-form output (デバッグ / 別 grammar 検証時用)",
            )
            .changed()
        {
            state.enforce_lol_grammar = enforce;
            if let Err(e) = state
                .db
                .set_enforce_lol_grammar(&state.profile_id, enforce)
            {
                tracing::warn!(error = %e, "failed to persist enforce_lol_grammar toggle");
            }
        }
    });

    ui.add_space(8.0);

    // BYO LLM (2026-08-23): OpenAI-compat provider configuration
    ui.collapsing("BYO LLM (OpenAI / Claude / Gemini / Ollama)", |ui| {
        show_byo_llm(ui, state, &mut settings.byo_llm);
    });

    ui.add_space(8.0);

    // LoRA share opt-out (Stage 5 T5.2)
    ui.collapsing("LoRA share", |ui| {
        let mut share = state.share_lol_dsl;
        if ui
            .checkbox(&mut share, "LoRA 学習データ提供に協力する")
            .on_hover_text(
                "オンにすると生成した LOL DSL + 品質シグナル (prompt / LOL 原文 / mesh SHA-256 / retry_count / safety_violations 等) が ALICE-LOL LoRA 学習セットに送信対象化されます Free tier default オン、Paid tier は完全 offline\n\n送信されないもの: Apple/Google/Microsoft アカウント ID / machine ID / file path / license key / crash report / P2P share pending キュー\n\n詳細: docs/SHARE.md",
            )
            .changed()
        {
            state.share_lol_dsl = share;
            if let Err(e) = state.db.set_share_lol_dsl(&state.profile_id, share) {
                tracing::warn!(error = %e, "failed to persist share_lol_dsl toggle");
            }
        }
        ui.add_space(4.0);
        // Stage 5: tier-effective status Paid tiers force opt-out
        // regardless of the checkbox — surface that explicitly so users
        // don't wonder why their toggle isn't taking effect
        let effective_status = if state.share_effective_enabled() {
            "現在: 共有中 (LoRA 品質向上に貢献)"
        } else if state.share_lol_dsl {
            "現在: 有料 tier のため自動 opt-out (アップロードしません)"
        } else {
            "現在: opt-out (アップロードしません)"
        };
        ui.label(effective_status);
        ui.add_space(4.0);
        // Stage 5: surface both queue counts — dry-run kept as local
        // audit corpus, share_queue is the real upload backlog drained by
        // `retry_queued_uploads` on startup
        let dry_queued =
            text_to_print_network::share::count_dry_run_queued(&state.share_dry_run_dir());
        let queue_pending = text_to_print_network::share::count_pending(&state.share_queue_dir());
        ui.label(format!(
            "アップロード待ち: 実キュー {queue_pending} 件 / dry-run {dry_queued} 件"
        ));
        ui.add_space(4.0);
        ui.hyperlink_to(
            "詳細な送信内容と opt-out 手順 (docs/SHARE.md)",
            "https://github.com/ext-sakamoro/text-to-print/blob/main/docs/SHARE.md",
        );
    });

    ui.add_space(8.0);

    // Crash reports opt-in (#36)
    ui.collapsing("クラッシュレポート", |ui| {
        let mut enabled = text_to_print_core::crash_report::is_optin(&state.data_dir);
        if ui
            .checkbox(&mut enabled, "クラッシュ発生時にローカル report を保存する")
            .on_hover_text(
                "オンにするとクラッシュ発生時に crash_reports/{uuid}.json が data_dir に保存されます (現状 upload なし、backend #36 実装後に opt-in で送信予定) オフにするとログのみ",
            )
            .changed()
            && let Err(e) =
                text_to_print_core::crash_report::set_optin(&state.data_dir, enabled)
        {
            tracing::warn!(error = %e, "failed to persist crash report opt-in");
        }
        ui.add_space(4.0);
        let pending = text_to_print_core::crash_report::count_pending(&state.data_dir);
        ui.label(format!("保存済 report: {pending} 件"));
        if pending > 0
            && ui.button("フォルダを開く").clicked()
        {
            let _ = open::that(
                text_to_print_core::crash_report::crash_reports_dir(&state.data_dir),
            );
        }
    });

    ui.add_space(8.0);

    // ネットワーク情報
    ui.collapsing("Network", |ui| {
        ui.label(format!("Profile ID: {}", &state.profile_id[..8]));
        ui.label(format!("本日の使用量: {} 回", state.daily_usage()));
        let limit = state.tier.limits().daily_generations;
        if limit == u32::MAX {
            ui.label("生成上限: 無制限");
        } else {
            ui.label(format!("生成上限: {} 回/日", limit));
        }

        ui.separator();
        ui.label(
            egui::RichText::new("Advanced (次回起動時に反映)")
                .small()
                .weak(),
        );

        // 初回だけ DB から現在値を SettingsState に load
        if !settings.network_loaded {
            settings.presets_endpoint_input = state
                .db
                .get_presets_endpoint(&state.profile_id)
                .unwrap_or_default();
            settings.sidecar_port_input = state
                .db
                .get_sidecar_port(&state.profile_id)
                .unwrap_or(8000)
                .to_string();
            settings.network_loaded = true;
        }

        // Preset library endpoint (custom URL / 空 = default Cloudflare)
        ui.horizontal(|ui| {
            ui.label("Preset endpoint:");
            ui.add(
                egui::TextEdit::singleline(&mut settings.presets_endpoint_input)
                    .hint_text("空 = 既定 (Cloudflare)")
                    .desired_width(340.0),
            );
        });
        ui.label(
            egui::RichText::new(
                "空欄なら https://text-to-print.alicelaw.net/api/presets を使用 \
                 (self-hosted mirror / proxy 経由時のみ変更)",
            )
            .small()
            .weak(),
        );

        // Preset sync enable toggle
        let mut sync_enabled = state
            .db
            .get_presets_sync_enabled(&state.profile_id)
            .unwrap_or(true);
        if ui
            .checkbox(&mut sync_enabled, "起動時に preset library を同期する")
            .changed()
        {
            let _ = state
                .db
                .set_presets_sync_enabled(&state.profile_id, sync_enabled);
        }

        // Sidecar port (u16)
        ui.horizontal(|ui| {
            ui.label("Sidecar port:");
            ui.add(
                egui::TextEdit::singleline(&mut settings.sidecar_port_input).desired_width(80.0),
            );
            ui.label(
                egui::RichText::new("(既定 8000、使用中なら +1 で自動 fallback)")
                    .small()
                    .weak(),
            );
        });

        // Save button
        if ui.button("Save network settings").clicked() {
            let endpoint = settings.presets_endpoint_input.trim().to_string();
            let port_result = settings.sidecar_port_input.trim().parse::<u16>();
            let mut errors = Vec::new();
            match port_result {
                Ok(p) if (1024..=65535).contains(&p) => {
                    if let Err(e) = state.db.set_sidecar_port(&state.profile_id, p) {
                        errors.push(format!("port save 失敗: {e}"));
                    }
                }
                _ => errors.push("port は 1024-65535 の整数".to_string()),
            }
            if let Err(e) = state.db.set_presets_endpoint(&state.profile_id, &endpoint) {
                errors.push(format!("endpoint save 失敗: {e}"));
            }
            if errors.is_empty() {
                settings.network_message = Some(("保存完了 次回起動時に反映".to_string(), true));
            } else {
                settings.network_message = Some((errors.join(" / "), false));
            }
        }

        if let Some((msg, ok)) = &settings.network_message {
            let color = if *ok {
                egui::Color32::LIGHT_GREEN
            } else {
                egui::Color32::LIGHT_RED
            };
            ui.colored_label(color, msg);
        }
    });
}

/// BYO LLM (2026-08-23) UI section — provider preset picker, per-provider
/// config form (endpoint / model / max_tokens / temperature /
/// reasoning_effort / API key), Save / Test / Delete / Activate actions,
/// and per-generation cost estimate
fn show_byo_llm(ui: &mut Ui, state: &mut AppState, form: &mut ByoLlmSettings) {
    // ── 1. One-shot init from DB ──────────────────────────────
    if !form.loaded {
        form.configured_providers = state
            .db
            .list_llm_provider_configs(&state.profile_id)
            .map(|rows| rows.into_iter().map(|r| r.provider).collect())
            .unwrap_or_default();
        form.active_provider = state
            .db
            .get_openai_compat_active_provider(&state.profile_id)
            .unwrap_or_else(|_| "OpenAi".to_string());
        // Auto-load the active provider into the form so users see the
        // current settings immediately when opening the section
        let active = OpenAiCompatProvider::from_db_str(&form.active_provider);
        load_provider_form(state, form, active);
        form.loaded = true;
    }

    // ── 2. Info banner ────────────────────────────────────────
    ui.label(
        egui::RichText::new(
            "リモート LLM API 設定 (OpenAI / Anthropic / Google / Ollama 等) \
             保存された provider のうち 1 つを 'アクティブ' として生成に使用します",
        )
        .small()
        .weak(),
    );
    ui.add_space(4.0);
    if state.backend_kind != BackendKind::OpenAiCompat {
        ui.colored_label(
            ui.style().visuals.warn_fg_color,
            "現在の Inference backend は BYO LLM ではありません 上の picker で 'BYO LLM' を選択すると有効",
        );
        ui.add_space(4.0);
    }

    // ── 3. Provider preset buttons ────────────────────────────
    ui.label("Provider preset:");
    ui.horizontal_wrapped(|ui| {
        for p in [
            OpenAiCompatProvider::OpenAi,
            OpenAiCompatProvider::Anthropic,
            OpenAiCompatProvider::Google,
            OpenAiCompatProvider::Custom,
        ] {
            let is_configured = form.configured_providers.iter().any(|s| s == p.to_db_str());
            let is_active = form.active_provider == p.to_db_str();
            let mut label = p.label().to_string();
            if is_active {
                label = format!("★ {label}");
            } else if is_configured {
                label = format!("● {label}");
            }
            let selected = form.form_provider == Some(p);
            if ui.selectable_label(selected, label).clicked() {
                load_provider_form(state, form, p);
            }
        }
    });
    ui.label(
        egui::RichText::new("★ = アクティブ (生成に使用中) / ● = 保存済 (未アクティブ)")
            .small()
            .weak(),
    );

    let Some(provider) = form.form_provider else {
        ui.add_space(4.0);
        ui.label(egui::RichText::new("上のボタンで provider を選択").weak());
        return;
    };

    // ── 4. Form fields ────────────────────────────────────────
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label("Endpoint:");
        ui.add(egui::TextEdit::singleline(&mut form.form_endpoint).desired_width(420.0));
    });
    ui.horizontal(|ui| {
        ui.label("Model:");
        ui.add(egui::TextEdit::singleline(&mut form.form_model).desired_width(300.0))
            .on_hover_text(
                "例: OpenAI: gpt-5 / o1 / gpt-4o-mini \
                 Anthropic: claude-sonnet-4-5 / claude-opus-4-7 \
                 Google: gemini-2.5-pro / gemini-2.5-flash \
                 Ollama: qwen2.5-14b-instruct",
            );
    });
    ui.horizontal(|ui| {
        ui.label("Max tokens:");
        ui.add(egui::TextEdit::singleline(&mut form.form_max_tokens).desired_width(80.0));
        ui.label(
            egui::RichText::new("(既定 256、大きくすると 1 生成コスト増)")
                .small()
                .weak(),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Temperature:");
        ui.add(egui::TextEdit::singleline(&mut form.form_temperature).desired_width(60.0));
        ui.label(egui::RichText::new("(0.0-2.0、既定 0.7)").small().weak());
    });

    // Reasoning effort (cost guard)
    ui.horizontal(|ui| {
        ui.label("Reasoning effort:");
        egui::ComboBox::from_id_salt("byo_llm_reasoning_effort")
            .selected_text(if form.form_reasoning_effort.is_empty() {
                "(未指定)"
            } else {
                form.form_reasoning_effort.as_str()
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut form.form_reasoning_effort, String::new(), "(未指定)");
                ui.selectable_value(
                    &mut form.form_reasoning_effort,
                    "minimal".to_string(),
                    "minimal (OpenAI GPT-5 / o-series)",
                );
                ui.selectable_value(
                    &mut form.form_reasoning_effort,
                    "none".to_string(),
                    "none (Google Gemini 2.5)",
                );
                ui.selectable_value(
                    &mut form.form_reasoning_effort,
                    "low".to_string(),
                    "low",
                );
            });
    });
    ui.label(
        egui::RichText::new(
            "OpenAI GPT-5/o-series は 'minimal' 推奨 (silent thinking 課金抑制) \
             Google Gemini 2.5 は 'none' 推奨 (silent thinking 抑制) \
             Anthropic / Ollama は空欄で OK",
        )
        .small()
        .weak(),
    );

    // API key input
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label("API key:");
        ui.add(
            egui::TextEdit::singleline(&mut form.form_api_key)
                .password(true)
                .desired_width(320.0)
                .hint_text("Keychain 保存、平文 disk 化なし"),
        );
    });
    let key_status = match keychain::get_api_key(provider.keychain_account()) {
        Ok(Some(_)) => "Keychain に保存済",
        Ok(None) => "未保存",
        Err(_) => "Keychain 読出エラー",
    };
    ui.label(
        egui::RichText::new(format!("Keychain 状態: {key_status}"))
            .small()
            .weak(),
    );

    // ── 5. Action buttons ─────────────────────────────────────
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        if ui.button("保存").clicked() {
            save_byo_llm_form(state, form, provider);
        }
        if ui
            .button("テスト送信 (~16 tokens)")
            .on_hover_text("フォーム内容で 1 回だけ生成 (最大 30 秒 UI ブロック)")
            .clicked()
        {
            test_byo_llm(state, form, provider);
        }
        if ui.button("削除").clicked() {
            delete_byo_llm_form(state, form, provider);
        }
        let is_saved = form
            .configured_providers
            .iter()
            .any(|s| s == provider.to_db_str());
        let is_active = form.active_provider == provider.to_db_str();
        if is_saved
            && !is_active
            && ui
                .button("この provider をアクティブ化")
                .on_hover_text("生成時にこの provider を使うよう切替")
                .clicked()
        {
            activate_byo_llm(state, form, provider);
        }
    });

    // ── 6. Cost estimate ──────────────────────────────────────
    ui.add_space(6.0);
    match estimate_cost_per_generation(&form.form_model) {
        Some(usd) => {
            ui.label(
                egui::RichText::new(format!(
                    "予想コスト: 約 ${usd:.4} / 生成 (system_prompt ~4K in + ~500 out トークン想定、rate は 2026-08-23 時点、実際は provider の pricing page で確認)"
                ))
                .small(),
            );
        }
        None => {
            ui.label(
                egui::RichText::new(
                    "予想コスト: rate table に model なし (Custom / 独自 model 使用時) \
                     API 課金は provider の pricing page で確認",
                )
                .small()
                .weak(),
            );
        }
    }

    // ── 7. Message ────────────────────────────────────────────
    if let Some((msg, ok)) = &form.message {
        ui.add_space(4.0);
        let color = if *ok {
            egui::Color32::LIGHT_GREEN
        } else {
            egui::Color32::LIGHT_RED
        };
        ui.colored_label(color, msg);
    }
}

/// Populate the form buffer with the provider's saved config, or fall
/// back to the preset defaults when nothing is saved yet Clears the API
/// key input to force explicit re-entry (never surface the stored key)
fn load_provider_form(
    state: &AppState,
    form: &mut ByoLlmSettings,
    provider: OpenAiCompatProvider,
) {
    form.form_provider = Some(provider);
    match state
        .db
        .get_llm_provider_config(&state.profile_id, provider.to_db_str())
    {
        Ok(Some(row)) => {
            form.form_endpoint = row.endpoint;
            form.form_model = row.model;
            form.form_max_tokens = row.max_tokens.to_string();
            form.form_temperature = format!("{:.2}", row.temperature);
            form.form_reasoning_effort = row.reasoning_effort.unwrap_or_default();
        }
        _ => {
            form.form_endpoint = provider.default_endpoint().to_string();
            form.form_model = provider.default_model().to_string();
            form.form_max_tokens = "256".to_string();
            form.form_temperature = "0.7".to_string();
            form.form_reasoning_effort = provider
                .default_reasoning_effort()
                .unwrap_or("")
                .to_string();
        }
    }
    form.form_api_key.clear();
    form.message = None;
}

/// Persist the form buffer to DB (`llm_provider_configs`) and OS
/// Keychain (`crate::keychain`) If a value was typed into the API key
/// field, it overwrites the Keychain entry; otherwise the existing
/// stored key (if any) is reused Rebuilds the runtime backend if the
/// saved provider matches the current active provider
fn save_byo_llm_form(
    state: &mut AppState,
    form: &mut ByoLlmSettings,
    provider: OpenAiCompatProvider,
) {
    // Parse + validate
    let max_tokens = match form.form_max_tokens.trim().parse::<u32>() {
        Ok(v) if v > 0 => v,
        _ => {
            form.message = Some(("max_tokens は正の整数".to_string(), false));
            return;
        }
    };
    let temperature = match form.form_temperature.trim().parse::<f32>() {
        Ok(v) if (0.0..=2.0).contains(&v) => v,
        _ => {
            form.message = Some(("temperature は 0.0-2.0 の実数".to_string(), false));
            return;
        }
    };
    if form.form_endpoint.trim().is_empty() {
        form.message = Some(("endpoint が空".to_string(), false));
        return;
    }
    if form.form_model.trim().is_empty() {
        form.message = Some(("model が空".to_string(), false));
        return;
    }

    // Save API key to Keychain if the user typed something in the input
    let api_key_typed = !form.form_api_key.trim().is_empty();
    if api_key_typed
        && let Err(e) =
            keychain::set_api_key(provider.keychain_account(), form.form_api_key.trim())
    {
        form.message = Some((format!("Keychain 保存失敗: {e}"), false));
        return;
    }

    // Resolve API key from Keychain (either just-saved or previously-saved)
    let api_key = match keychain::get_api_key(provider.keychain_account()) {
        Ok(Some(k)) => k,
        Ok(None) => {
            form.message = Some((
                "API key が未入力 (Keychain にも保存なし)".to_string(),
                false,
            ));
            return;
        }
        Err(e) => {
            form.message = Some((format!("Keychain 読出失敗: {e}"), false));
            return;
        }
    };

    let reasoning_effort = if form.form_reasoning_effort.trim().is_empty() {
        None
    } else {
        Some(form.form_reasoning_effort.trim().to_string())
    };

    let row = LlmProviderConfigRow {
        provider: provider.to_db_str().to_string(),
        endpoint: form.form_endpoint.trim().to_string(),
        model: form.form_model.trim().to_string(),
        max_tokens,
        temperature,
        reasoning_effort: reasoning_effort.clone(),
    };
    if let Err(e) = state.db.set_llm_provider_config(&state.profile_id, &row) {
        form.message = Some((format!("DB 保存失敗: {e}"), false));
        return;
    }

    // Rebuild the in-memory backend when this provider is currently active
    // or when the user is switching to BYO LLM as their backend kind
    if form.active_provider == provider.to_db_str()
        || state.backend_kind == BackendKind::OpenAiCompat
    {
        let cfg = OpenAiCompatConfig {
            provider,
            endpoint: row.endpoint.clone(),
            api_key,
            model: row.model.clone(),
            reasoning_effort,
        };
        if let Ok(mut slot) = state.openai_compat.lock() {
            *slot = Some(OpenAiCompatBackend::new(cfg));
        }
    }

    if !form
        .configured_providers
        .iter()
        .any(|s| s == provider.to_db_str())
    {
        form.configured_providers
            .push(provider.to_db_str().to_string());
        form.configured_providers.sort();
    }
    form.form_api_key.clear();
    form.message = Some(("保存完了".to_string(), true));
}

/// Remove the saved config for a provider Deletes both the DB row and
/// the Keychain entry Idempotent — no error if either is already absent
fn delete_byo_llm_form(
    state: &mut AppState,
    form: &mut ByoLlmSettings,
    provider: OpenAiCompatProvider,
) {
    if let Err(e) = state
        .db
        .delete_llm_provider_config(&state.profile_id, provider.to_db_str())
    {
        form.message = Some((format!("DB 削除失敗: {e}"), false));
        return;
    }
    if let Err(e) = keychain::delete_api_key(provider.keychain_account()) {
        form.message = Some((format!("Keychain 削除失敗: {e}"), false));
        return;
    }
    if form.active_provider == provider.to_db_str()
        && let Ok(mut slot) = state.openai_compat.lock()
    {
        *slot = None;
    }
    form.configured_providers
        .retain(|s| s != provider.to_db_str());
    form.form_api_key.clear();
    form.message = Some(("削除完了".to_string(), true));
}

/// Mark the given provider as the active generation target Persists the
/// choice in DB (`profiles.openai_compat_active_provider`) and
/// populates the runtime backend slot from the stored DB row + Keychain
fn activate_byo_llm(
    state: &mut AppState,
    form: &mut ByoLlmSettings,
    provider: OpenAiCompatProvider,
) {
    if let Err(e) = state
        .db
        .set_openai_compat_active_provider(&state.profile_id, provider.to_db_str())
    {
        form.message = Some((format!("active provider 保存失敗: {e}"), false));
        return;
    }
    form.active_provider = provider.to_db_str().to_string();
    let row = match state
        .db
        .get_llm_provider_config(&state.profile_id, provider.to_db_str())
    {
        Ok(Some(r)) => r,
        _ => {
            form.message = Some((
                "この provider は未保存 まず '保存' して下さい".to_string(),
                false,
            ));
            return;
        }
    };
    let api_key = match keychain::get_api_key(provider.keychain_account()) {
        Ok(Some(k)) => k,
        Ok(None) => {
            form.message = Some((
                "Keychain に API key なし まず '保存' して下さい".to_string(),
                false,
            ));
            return;
        }
        Err(e) => {
            form.message = Some((format!("Keychain 読出失敗: {e}"), false));
            return;
        }
    };
    let cfg = OpenAiCompatConfig {
        provider,
        endpoint: row.endpoint,
        api_key,
        model: row.model,
        reasoning_effort: row.reasoning_effort,
    };
    if let Ok(mut slot) = state.openai_compat.lock() {
        *slot = Some(OpenAiCompatBackend::new(cfg));
    }
    form.message = Some((format!("{} をアクティブ化しました", provider.label()), true));
}

/// Fire a short synthetic generation request against the currently-
/// edited form config Blocks the UI thread for up to 30 s (acceptable
/// for a manual Test button) API key resolution: form input takes
/// priority; falls back to Keychain-stored value
fn test_byo_llm(
    state: &AppState,
    form: &mut ByoLlmSettings,
    provider: OpenAiCompatProvider,
) {
    let api_key = if !form.form_api_key.trim().is_empty() {
        form.form_api_key.trim().to_string()
    } else {
        match keychain::get_api_key(provider.keychain_account()) {
            Ok(Some(k)) => k,
            _ => {
                form.message = Some((
                    "API key が form / Keychain のどちらにもなし".to_string(),
                    false,
                ));
                return;
            }
        }
    };
    if form.form_endpoint.trim().is_empty() {
        form.message = Some(("endpoint が空".to_string(), false));
        return;
    }
    if form.form_model.trim().is_empty() {
        form.message = Some(("model が空".to_string(), false));
        return;
    }
    let cfg = OpenAiCompatConfig {
        provider,
        endpoint: form.form_endpoint.trim().to_string(),
        api_key,
        model: form.form_model.trim().to_string(),
        reasoning_effort: if form.form_reasoning_effort.trim().is_empty() {
            None
        } else {
            Some(form.form_reasoning_effort.trim().to_string())
        },
    };
    let backend = OpenAiCompatBackend::new(cfg);
    let params = text_to_print_llm::backend_kind::InferenceParams {
        max_tokens: 16,
        temperature: 0.0,
        top_k: 40,
        grammar: None,
    };
    let outcome = state.runtime.block_on(async move {
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            backend.generate(
                "You are a test responder. Reply with exactly: OK",
                "Reply with the exact word OK.",
                &params,
            ),
        )
        .await
    });
    form.message = Some(match outcome {
        Ok(Ok(reply)) => {
            let preview: String = reply.chars().take(80).collect();
            (format!("成功: {preview}"), true)
        }
        Ok(Err(e)) => (format!("失敗: {e}"), false),
        Err(_) => ("失敗: 30 秒でタイムアウト".to_string(), false),
    });
}

/// Rough per-generation cost estimate in USD Assumes 4 000 input
/// tokens (typical system_prompt) + 500 output tokens (typical mug /
/// bowl generation) Returns `None` when the model is not in the rate
/// table (Custom / Ollama / unknown release)
///
/// Rate reference: 2026-08-23 public pricing pages; prices drift, so
/// the UI labels the number as "estimate" and points users at the
/// provider's own pricing page for authoritative numbers
fn estimate_cost_per_generation(model: &str) -> Option<f64> {
    const INPUT_TOKENS: f64 = 4_000.0;
    const OUTPUT_TOKENS: f64 = 500.0;
    let m = model.trim().to_lowercase();
    // (input_usd_per_1M, output_usd_per_1M)
    let rate: Option<(f64, f64)> = if m.contains("gpt-5") {
        Some((2.00, 10.00))
    } else if m.contains("o1-mini") || m.contains("o3-mini") {
        Some((3.00, 12.00))
    } else if m.contains("o1") || m.contains("o3") {
        Some((15.00, 60.00))
    } else if m.contains("gpt-4o-mini") || m.contains("gpt-4.1-mini") {
        Some((0.15, 0.60))
    } else if m.contains("gpt-4o") || m.contains("gpt-4.1") {
        Some((5.00, 15.00))
    } else if m.contains("claude-opus") {
        Some((15.00, 75.00))
    } else if m.contains("claude-sonnet") {
        Some((3.00, 15.00))
    } else if m.contains("claude-haiku") {
        Some((0.80, 4.00))
    } else if m.contains("gemini-2.5-pro") {
        Some((1.25, 10.00))
    } else if m.contains("gemini-2.5-flash") {
        Some((0.30, 2.50))
    } else if m.contains("gemini-1.5-pro") {
        Some((1.25, 5.00))
    } else if m.contains("gemini-1.5-flash") {
        Some((0.075, 0.30))
    } else {
        None
    };
    rate.map(|(in_rate, out_rate)| {
        (INPUT_TOKENS / 1_000_000.0) * in_rate + (OUTPUT_TOKENS / 1_000_000.0) * out_rate
    })
}

fn apply_license(state: &mut AppState, settings: &mut SettingsState) {
    let input = settings.license_input.trim();

    // Base64 デコード
    let key = match LicenseKey::from_base64(input) {
        Ok(k) => k,
        Err(e) => {
            settings.license_message = Some((format!("無効なキー: {e}"), false));
            return;
        }
    };

    // 署名検証
    let verifier = match LicenseVerifier::new(&LICENSE_PUBLIC_KEY) {
        Ok(v) => v,
        Err(_) => {
            settings.license_message = Some(("ライセンスシステム未設定".to_string(), false));
            return;
        }
    };

    match verifier.verify(&key) {
        Ok(payload) => {
            let tier = payload.tier;
            let tier_str = format!("{tier:?}");
            let expires = payload.expires_at.format("%Y-%m-%d").to_string();

            // DB に保存
            let _ = state
                .db
                .update_profile_tier(&state.profile_id, &tier_str, input);
            state.tier = tier;

            settings.license_message = Some((
                format!("{tier:?} プランに更新しました (有効期限 {expires})"),
                true,
            ));
            tracing::info!(tier = ?tier, expires = %expires, "license applied");
        }
        Err(e) => {
            settings.license_message = Some((format!("検証失敗: {e}"), false));
        }
    }
}

/// Revert to Free tier — clears the stored license_key so subsequent
/// starts don't reinstate the paid tier from DB (Phase S2 UX for testing
/// / user-requested cancel)
fn clear_license(state: &mut AppState, settings: &mut SettingsState) {
    let free_str = format!("{:?}", Tier::Free);
    match state
        .db
        .update_profile_tier(&state.profile_id, &free_str, "")
    {
        Ok(()) => {
            state.tier = Tier::Free;
            settings.license_input.clear();
            settings.license_message = Some(("Free に戻しました".to_string(), true));
            tracing::info!("license cleared, tier reverted to Free");
        }
        Err(e) => {
            settings.license_message = Some((format!("DB 更新失敗: {e}"), false));
        }
    }
}

/// Kick off a checkout request to the CF Workers backend and open the
/// returned Stripe URL in the user's browser Runs the network call on
/// the shared tokio runtime so the UI thread is not blocked
fn spawn_checkout(state: &AppState, settings: &mut SettingsState, plan: &str) {
    let email = settings.checkout_email.trim().to_string();
    if !is_plausible_email(&email) {
        settings.checkout_message = Some(("email が未入力または不正です".to_string(), false));
        return;
    }
    settings.checkout_message = Some((format!("{plan} の checkout URL を取得中..."), true));

    let endpoint = checkout_endpoint();
    let body = CheckoutRequestBody {
        plan: plan.to_string(),
        user_email: email,
    };
    let tx = state.result_tx.clone();

    state.runtime.spawn(async move {
        let outcome = fetch_and_open_checkout(&endpoint, &body).await;
        // Piggyback on the existing generation-status channel to notify
        // the UI thread — a dedicated channel would be tidier but adds
        // wiring for a purely informational side effect
        let msg = match outcome {
            Ok(url) => format!("checkout ok: {url}"),
            Err(e) => format!("checkout error: {e}"),
        };
        tracing::info!(target: "checkout", %msg);
        // Note: we intentionally do NOT push to `tx` because
        // GenerationMessage does not have a variant for this; the
        // spawn is fire-and-forget with tracing feedback only
        let _ = &tx;
    });
}

async fn fetch_and_open_checkout(
    endpoint: &str,
    body: &CheckoutRequestBody,
) -> Result<String, String> {
    let client = reqwest::Client::new();
    let resp = client
        .post(endpoint)
        .json(body)
        .send()
        .await
        .map_err(|e| format!("HTTP エラー: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("backend {status}: {text}"));
    }
    let parsed: CheckoutResponseBody = resp
        .json()
        .await
        .map_err(|e| format!("レスポンス parse 失敗: {e}"))?;
    open::that(&parsed.url).map_err(|e| format!("browser 起動失敗: {e}"))?;
    Ok(parsed.url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkout_request_body_serializes_to_expected_wire_format() {
        let req = CheckoutRequestBody {
            plan: "pro_monthly".to_string(),
            user_email: "user@example.com".to_string(),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["plan"], "pro_monthly");
        assert_eq!(json["user_email"], "user@example.com");
        // Ensure no extra fields snuck in (contract with worker::CheckoutRequest)
        let obj = json.as_object().unwrap();
        assert_eq!(obj.len(), 2);
    }

    #[test]
    fn checkout_response_body_parses_worker_output() {
        let raw =
            r#"{"url":"https://checkout.stripe.com/c/pay/cs_test_abc","session_id":"cs_test_abc"}"#;
        let parsed: CheckoutResponseBody = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.url, "https://checkout.stripe.com/c/pay/cs_test_abc");
        assert_eq!(parsed.session_id, "cs_test_abc");
    }

    #[test]
    fn checkout_response_body_tolerates_missing_session_id() {
        // Defensive: worker may omit session_id in some paths
        let raw = r#"{"url":"https://checkout.stripe.com/c/pay/cs_test_xyz"}"#;
        let parsed: CheckoutResponseBody = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.url, "https://checkout.stripe.com/c/pay/cs_test_xyz");
        assert_eq!(parsed.session_id, "");
    }

    #[test]
    fn plausible_email_accepts_normal_addresses() {
        assert!(is_plausible_email("user@example.com"));
        assert!(is_plausible_email("user+tag@example.co.jp"));
        assert!(is_plausible_email("a@b.c"));
    }

    #[test]
    fn plausible_email_rejects_obvious_bad() {
        assert!(!is_plausible_email(""));
        assert!(!is_plausible_email("no-at-sign"));
        assert!(!is_plausible_email("has space@example.com"));
        assert!(!is_plausible_email(&"x".repeat(321)));
    }

    #[test]
    fn checkout_endpoint_defaults_to_production_when_env_unset() {
        // SAFETY: single-threaded test suite by default; we clear the
        // var to assert the default path Not perfect under parallel
        // execution but the default is deterministic when unset
        // SAFETY: Setting env vars is unsafe in Rust 2024
        unsafe {
            std::env::remove_var("TTP_CHECKOUT_ENDPOINT");
        }
        assert_eq!(checkout_endpoint(), DEFAULT_CHECKOUT_ENDPOINT);
    }

    #[test]
    fn enterprise_mailto_points_to_alicelaw_net() {
        assert!(ENTERPRISE_MAILTO.starts_with("mailto:"));
        assert!(ENTERPRISE_MAILTO.contains("enterprise@alicelaw.net"));
    }

    #[test]
    fn default_checkout_endpoint_targets_backend_path() {
        assert!(DEFAULT_CHECKOUT_ENDPOINT.ends_with("/stripe/checkout-session"));
        assert!(DEFAULT_CHECKOUT_ENDPOINT.starts_with("https://"));
    }
}
