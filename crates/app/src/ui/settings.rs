use egui::Ui;
use serde::{Deserialize, Serialize};

use crate::i18n::{Lang, T};
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

pub fn show(ui: &mut Ui, state: &mut AppState, settings: &mut SettingsState, lang: Lang) {
    ui.heading(T::settings_heading(lang));
    ui.separator();

    // Settings 全体は viewport より大きくなる可能性大 (BYO LLM UI + 各種 toggle + collapsing sections 多数)
    // ScrollArea で wrap して全 content 到達可能に (2026-09-02 fix、user 報告事案)
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            // P1-11 Phase 6 (2026-09-07): UI Lang switcher Persists to
            // profiles.lang_pref ("auto" / "ja" / "en") main.rs
            // re-resolves App.lang every frame so the change is live
            ui.collapsing(T::settings_language_section(lang), |ui| {
                let current = state.lang_pref.clone();
                let mut selected = current.clone();
                ui.horizontal(|ui| {
                    ui.label(T::settings_language_label(lang));
                    egui::ComboBox::from_id_salt("settings_lang_pref")
                        .selected_text(match selected.as_str() {
                            "ja" => "日本語",
                            "en" => "English",
                            _ => T::settings_language_auto(lang),
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut selected,
                                "auto".to_string(),
                                T::settings_language_auto(lang),
                            );
                            ui.selectable_value(&mut selected, "ja".to_string(), "日本語");
                            ui.selectable_value(&mut selected, "en".to_string(), "English");
                        })
                        .response
                        .on_hover_text(T::settings_language_hover(lang));
                });
                if selected != current {
                    state.lang_pref = selected.clone();
                    if let Err(e) = state.db.set_lang_pref(&state.profile_id, &selected) {
                        tracing::warn!(error = %e, "failed to persist lang_pref");
                    }
                }
                ui.add_space(4.0);
                ui.label(T::settings_language_env_note(lang));
            });

            ui.add_space(8.0);

            // ライセンス / サブスクリプション
            ui.collapsing(T::settings_section_license(lang), |ui| {
                // 現在の tier 表示 (Free / Pro / Enterprise / General)
                ui.horizontal(|ui| {
                    ui.label(T::settings_current_plan(lang));
                    let (label, color): (&'static str, egui::Color32) = match state.tier {
                        Tier::Free => {
                            (T::settings_tier_free_label(lang), egui::Color32::LIGHT_GRAY)
                        }
                        Tier::General => ("General", egui::Color32::LIGHT_BLUE),
                        Tier::Pro => ("Pro", egui::Color32::GREEN),
                        Tier::Enterprise => ("Enterprise", egui::Color32::GOLD),
                    };
                    ui.colored_label(color, label);
                });
                ui.colored_label(
                    ui.style().visuals.warn_fg_color,
                    T::settings_beta_plan_gated(lang),
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
                    ui.label(egui::RichText::new(T::settings_upgrade_pro(lang)).strong());
                    if PAID_UI_ENABLED {
                        ui.label(T::settings_pro_description(lang));
                    } else {
                        ui.colored_label(
                            ui.style().visuals.warn_fg_color,
                            T::settings_pro_coming_soon(lang),
                        );
                        ui.label(T::settings_pro_planned(lang));
                    }
                    ui.add_space(4.0);

                    ui.horizontal(|ui| {
                        ui.label(T::settings_email_label(lang));
                        ui.add_enabled(
                            PAID_UI_ENABLED,
                            egui::TextEdit::singleline(&mut settings.checkout_email),
                        );
                    });
                    let email_ok = PAID_UI_ENABLED && is_plausible_email(&settings.checkout_email);

                    ui.horizontal(|ui| {
                        let monthly = ui
                            .add_enabled(email_ok, egui::Button::new(T::settings_buy_monthly(lang)))
                            .on_disabled_hover_text(T::settings_paid_ui_disabled_hover(lang));
                        if monthly.clicked() {
                            spawn_checkout(state, settings, "pro_monthly", lang);
                        }
                        let yearly = ui
                            .add_enabled(email_ok, egui::Button::new(T::settings_buy_yearly(lang)))
                            .on_disabled_hover_text(T::settings_paid_ui_disabled_hover(lang));
                        if yearly.clicked() {
                            spawn_checkout(state, settings, "pro_yearly", lang);
                        }
                    });

                    if PAID_UI_ENABLED && !email_ok && !settings.checkout_email.is_empty() {
                        ui.colored_label(
                            ui.style().visuals.warn_fg_color,
                            T::settings_email_invalid(lang),
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
                    ui.label(egui::RichText::new(T::settings_enterprise_plan(lang)).strong());
                    ui.horizontal(|ui| {
                        ui.label(T::settings_enterprise_prompt(lang));
                        if ui.button(T::settings_contact_button(lang)).clicked()
                            && let Err(e) = open::that(ENTERPRISE_MAILTO)
                        {
                            settings.checkout_message = Some((
                                format!("{}: {e}", T::settings_mailer_launch_fail(lang)),
                                false,
                            ));
                        }
                    });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(6.0);
                }

                // ── License key 入力 (受信 email から貼付) ───────────────
                ui.label(egui::RichText::new(T::settings_license_input_heading(lang)).strong());
                ui.label(T::settings_license_input_hint(lang));
                ui.add_space(2.0);
                ui.text_edit_multiline(&mut settings.license_input);

                ui.horizontal(|ui| {
                    if ui.button(T::apply_license(lang)).clicked()
                        && !settings.license_input.trim().is_empty()
                    {
                        apply_license(state, settings, lang);
                    }
                    if !matches!(state.tier, Tier::Free)
                        && ui.button(T::settings_license_clear(lang)).clicked()
                    {
                        clear_license(state, settings, lang);
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
            ui.collapsing(T::settings_section_llm(lang), |ui| {
                // Stage 3-C.6: inference backend picker
                // BYO LLM (2026-08-23): added OpenAiCompat variant for remote
                // API providers (OpenAI / Anthropic / Google / Ollama)
                ui.label(T::settings_inference_backend_label(lang));
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
                    ui.label(format!(
                        "{} {}",
                        T::settings_embedded_status(lang),
                        status.label()
                    ))
                    .on_hover_text(T::settings_embedded_hover(lang));
                    // Stage 3-C.12: CPU / GPU picker (only relevant to Embedded)
                    ui.add_space(4.0);
                    ui.label(T::settings_execution_mode_label(lang));
                    let prev_mode = state.execution_mode;
                    let mut new_mode = prev_mode;
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut new_mode,
                            ExecutionMode::Cpu,
                            ExecutionMode::Cpu.label(),
                        );
                        ui.selectable_value(
                            &mut new_mode,
                            ExecutionMode::Gpu,
                            ExecutionMode::Gpu.label(),
                        );
                    });
                    if new_mode != prev_mode {
                        state.switch_execution_mode(new_mode);
                    }
                    ui.label(format!(
                        "{} {}",
                        T::settings_current(lang),
                        state.execution_mode.label()
                    ))
                    .on_hover_text(T::settings_execution_mode_hover(lang));

                    // BYO LLM (2026-08-23): custom GGUF path override
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(T::settings_custom_gguf_heading(lang)).strong());
                    let display_path = state
                        .custom_gguf_path
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| T::settings_custom_gguf_unset(lang).to_string());
                    ui.label(egui::RichText::new(display_path).small().color(
                        if state.custom_gguf_path.is_some() {
                            egui::Color32::LIGHT_GREEN
                        } else {
                            ui.style().visuals.weak_text_color()
                        },
                    ));
                    ui.horizontal(|ui| {
                        if ui
                            .button(T::settings_gguf_select_button(lang))
                            .on_hover_text(T::settings_gguf_select_hover_extra(lang))
                            .clicked()
                            && let Some(path) = rfd::FileDialog::new()
                                .add_filter("GGUF", &["gguf"])
                                .pick_file()
                        {
                            state.set_custom_gguf_path(Some(path));
                        }
                        if state.custom_gguf_path.is_some()
                            && ui
                                .button(T::settings_clear_button(lang))
                                .on_hover_text(T::settings_gguf_clear_hover(lang))
                                .clicked()
                        {
                            state.set_custom_gguf_path(None);
                        }
                    });
                }
                ui.add_space(6.0);

                ui.label(T::settings_endpoint_alice_llm(lang));
                ui.text_edit_singleline(&mut state.llm_config.endpoint);

                ui.add_space(4.0);
                ui.label(T::settings_model_label(lang));
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
                            "{} {} {}",
                            T::settings_manual_placement_prefix(lang),
                            state.llm_config.model_choice.default_filename(),
                            T::settings_manual_placement_suffix(lang),
                        ),
                    )
                    .on_hover_text(T::settings_bonsai_manual_hover(lang));
                }

                ui.add_space(4.0);
                ui.label(format!(
                    "{}: {:.1}",
                    T::settings_temperature_label(lang),
                    state.llm_config.temperature
                ));
                ui.add(egui::Slider::new(
                    &mut state.llm_config.temperature,
                    0.0..=2.0,
                ));

                // Stage 3-C.14: LOL GBNF grammar-constrained decoding toggle
                ui.add_space(6.0);
                let mut enforce = state.enforce_lol_grammar;
                if ui
                    .checkbox(&mut enforce, T::settings_grammar_checkbox(lang))
                    .on_hover_text(T::settings_grammar_hover(lang))
                    .changed()
                {
                    state.enforce_lol_grammar = enforce;
                    if let Err(e) = state.db.set_enforce_lol_grammar(&state.profile_id, enforce) {
                        tracing::warn!(error = %e, "failed to persist enforce_lol_grammar toggle");
                    }
                }
            });

            ui.add_space(8.0);

            // BYO LLM (2026-08-23): OpenAI-compat provider configuration
            ui.collapsing(T::settings_section_byo(lang), |ui| {
                show_byo_llm(ui, state, &mut settings.byo_llm, lang);
            });

            ui.add_space(8.0);

            // Gallery Phase 1 (2026-08-26): profile display name shown in the
            // Gallery in place of the raw DID hex Empty = fallback to DID
            // short-form 32 char cap enforced client-side
            ui.collapsing(T::settings_profile_section(lang), |ui| {
                let mut nickname = state.nickname.clone();
                let response = ui
                    .add(
                        egui::TextEdit::singleline(&mut nickname)
                            .hint_text(T::settings_nickname_hint(lang))
                            .char_limit(32)
                            .desired_width(240.0),
                    )
                    .on_hover_text(T::settings_nickname_hover(lang));
                if response.lost_focus() && nickname != state.nickname {
                    state.nickname = nickname.clone();
                    if let Err(e) = state.db.set_nickname(&state.profile_id, &nickname) {
                        tracing::warn!(error = %e, "failed to persist nickname");
                    }
                }
                ui.add_space(4.0);
                ui.label(format!(
                    "{}: {}",
                    T::settings_did_label(lang),
                    &state.profile_id[..16.min(state.profile_id.len())]
                ));
            });

            ui.add_space(8.0);

            // LoRA share opt-out (Stage 5 T5.2)
            ui.collapsing(T::settings_section_lora_share(lang), |ui| {
                let mut share = state.share_lol_dsl;
                if ui
                    .checkbox(&mut share, T::settings_lora_share_checkbox(lang))
                    .on_hover_text(T::settings_lora_share_hover(lang))
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
                let effective_status: &'static str = if state.share_effective_enabled() {
                    T::settings_share_status_active(lang)
                } else if state.share_lol_dsl {
                    T::settings_share_status_paid(lang)
                } else {
                    T::settings_share_status_off(lang)
                };
                ui.label(effective_status);
                ui.add_space(4.0);
                // Stage 5: surface both queue counts — dry-run kept as local
                // audit corpus, share_queue is the real upload backlog drained by
                // `retry_queued_uploads` on startup
                let dry_queued =
                    text_to_print_network::share::count_dry_run_queued(&state.share_dry_run_dir());
                let queue_pending =
                    text_to_print_network::share::count_pending(&state.share_queue_dir());
                ui.label(format!(
                    "{} {queue_pending} {} / dry-run {dry_queued} {}",
                    T::settings_upload_queue(lang),
                    T::settings_upload_queue_items(lang),
                    T::settings_upload_queue_items(lang),
                ));
                ui.add_space(4.0);
                // Gallery Phase 2 (2026-08-26): confirm-dialog opt-in Only
                // meaningful when the parent LoRA share is on; Paid tiers
                // never reach the dialog either way
                let mut auto = state.gallery_auto_share;
                let auto_response = ui
                    .add_enabled(
                        state.share_effective_enabled(),
                        egui::Checkbox::new(&mut auto, T::settings_auto_publish_checkbox(lang)),
                    )
                    .on_hover_text(T::settings_auto_publish_hover(lang));
                if auto_response.changed() {
                    state.gallery_auto_share = auto;
                    if let Err(e) = state.db.set_gallery_auto_share(&state.profile_id, auto) {
                        tracing::warn!(error = %e, "failed to persist gallery_auto_share toggle");
                    }
                }
                ui.add_space(4.0);
                ui.hyperlink_to(
                    T::settings_share_details_link(lang),
                    "https://github.com/ext-sakamoro/text-to-print/blob/main/docs/SHARE.md",
                );
            });

            ui.add_space(8.0);

            // Crash reports opt-in (#36)
            ui.collapsing(T::settings_section_crash(lang), |ui| {
                let mut enabled = text_to_print_core::crash_report::is_optin(&state.data_dir);
                if ui
                    .checkbox(&mut enabled, T::settings_crash_checkbox(lang))
                    .on_hover_text(T::settings_crash_hover(lang))
                    .changed()
                    && let Err(e) =
                        text_to_print_core::crash_report::set_optin(&state.data_dir, enabled)
                {
                    tracing::warn!(error = %e, "failed to persist crash report opt-in");
                }
                ui.add_space(4.0);
                let pending = text_to_print_core::crash_report::count_pending(&state.data_dir);
                ui.label(format!(
                    "{} {pending} {}",
                    T::settings_crash_saved(lang),
                    T::settings_upload_queue_items(lang),
                ));
                if pending > 0 && ui.button(T::settings_crash_open_folder(lang)).clicked() {
                    let _ = open::that(text_to_print_core::crash_report::crash_reports_dir(
                        &state.data_dir,
                    ));
                }
            });

            ui.add_space(8.0);

            // ネットワーク情報
            ui.collapsing(T::settings_section_network(lang), |ui| {
                ui.label(format!(
                    "{}: {}",
                    T::settings_profile_id_label(lang),
                    &state.profile_id[..8]
                ));
                ui.label(format!(
                    "{} {} {}",
                    T::settings_today_usage(lang),
                    state.daily_usage(),
                    T::settings_times_unit(lang),
                ));
                let limit = state.tier.limits().daily_generations;
                if limit == u32::MAX {
                    ui.label(T::settings_limit_unlimited(lang));
                } else {
                    ui.label(format!(
                        "{} {} {}",
                        T::settings_limit_daily_prefix(lang),
                        limit,
                        T::settings_limit_per_day(lang),
                    ));
                }

                ui.separator();
                ui.label(
                    egui::RichText::new(T::settings_advanced_section(lang))
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
                    ui.label(T::settings_preset_endpoint_label(lang));
                    ui.add(
                        egui::TextEdit::singleline(&mut settings.presets_endpoint_input)
                            .hint_text(T::settings_endpoint_hint(lang))
                            .desired_width(340.0),
                    );
                });
                ui.label(
                    egui::RichText::new(T::settings_endpoint_hover_extra(lang))
                        .small()
                        .weak(),
                );

                // Preset sync enable toggle
                let mut sync_enabled = state
                    .db
                    .get_presets_sync_enabled(&state.profile_id)
                    .unwrap_or(true);
                if ui
                    .checkbox(&mut sync_enabled, T::settings_preset_sync_checkbox(lang))
                    .changed()
                {
                    let _ = state
                        .db
                        .set_presets_sync_enabled(&state.profile_id, sync_enabled);
                }

                // Sidecar port (u16)
                ui.horizontal(|ui| {
                    ui.label(T::settings_sidecar_port_label(lang));
                    ui.add(
                        egui::TextEdit::singleline(&mut settings.sidecar_port_input)
                            .desired_width(80.0),
                    );
                    ui.label(
                        egui::RichText::new(T::settings_port_default_hint(lang))
                            .small()
                            .weak(),
                    );
                });

                // Save button
                if ui.button(T::settings_save_network_button(lang)).clicked() {
                    let endpoint = settings.presets_endpoint_input.trim().to_string();
                    let port_result = settings.sidecar_port_input.trim().parse::<u16>();
                    let mut errors = Vec::new();
                    match port_result {
                        Ok(p) if (1024..=65535).contains(&p) => {
                            if let Err(e) = state.db.set_sidecar_port(&state.profile_id, p) {
                                errors.push(format!("{}: {e}", T::settings_port_save_error(lang)));
                            }
                        }
                        _ => errors.push(T::settings_port_range_error(lang).to_string()),
                    }
                    if let Err(e) = state.db.set_presets_endpoint(&state.profile_id, &endpoint) {
                        errors.push(format!("{}: {e}", T::settings_endpoint_save_error(lang)));
                    }
                    if errors.is_empty() {
                        settings.network_message =
                            Some((T::settings_save_next_launch(lang).to_string(), true));
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
        }); // ScrollArea::show close (2026-09-02 fix)
}

/// BYO LLM (2026-08-23) UI section — provider preset picker, per-provider
/// config form (endpoint / model / max_tokens / temperature /
/// reasoning_effort / API key), Save / Test / Delete / Activate actions,
/// and per-generation cost estimate
fn show_byo_llm(ui: &mut Ui, state: &mut AppState, form: &mut ByoLlmSettings, lang: Lang) {
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
        egui::RichText::new(T::settings_byo_intro_extra(lang))
            .small()
            .weak(),
    );
    ui.add_space(4.0);
    if state.backend_kind != BackendKind::OpenAiCompat {
        ui.colored_label(
            ui.style().visuals.warn_fg_color,
            T::settings_byo_not_selected(lang),
        );
        ui.add_space(4.0);
    }

    // ── 3. Provider preset buttons ────────────────────────────
    ui.label(T::settings_provider_preset_label(lang));
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
        egui::RichText::new(T::settings_byo_legend(lang))
            .small()
            .weak(),
    );
    ui.label(
        egui::RichText::new(T::settings_byo_free_hint_extra(lang))
            .small()
            .color(egui::Color32::from_rgb(120, 180, 220)),
    );

    let Some(provider) = form.form_provider else {
        ui.add_space(4.0);
        ui.label(egui::RichText::new(T::settings_byo_select_provider(lang)).weak());
        return;
    };

    // ── 4. Form fields ────────────────────────────────────────
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(T::settings_byo_endpoint_label(lang));
        ui.add(egui::TextEdit::singleline(&mut form.form_endpoint).desired_width(420.0));
    });
    ui.horizontal(|ui| {
        ui.label(T::settings_byo_model_label(lang));
        ui.add(egui::TextEdit::singleline(&mut form.form_model).desired_width(300.0))
            .on_hover_text(T::settings_byo_model_hint_extra(lang));
    });
    ui.horizontal(|ui| {
        ui.label(T::settings_byo_max_tokens_label(lang));
        ui.add(egui::TextEdit::singleline(&mut form.form_max_tokens).desired_width(80.0));
        ui.label(
            egui::RichText::new(T::settings_byo_max_tokens_hint(lang))
                .small()
                .weak(),
        );
    });
    ui.horizontal(|ui| {
        ui.label(T::settings_byo_temperature_label(lang));
        ui.add(egui::TextEdit::singleline(&mut form.form_temperature).desired_width(60.0));
        ui.label(
            egui::RichText::new(T::settings_byo_temperature_hint(lang))
                .small()
                .weak(),
        );
    });

    // Reasoning effort (cost guard)
    ui.horizontal(|ui| {
        ui.label(T::settings_byo_reasoning_label(lang));
        egui::ComboBox::from_id_salt("byo_llm_reasoning_effort")
            .selected_text(if form.form_reasoning_effort.is_empty() {
                T::settings_byo_reasoning_unset(lang)
            } else {
                form.form_reasoning_effort.as_str()
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut form.form_reasoning_effort,
                    String::new(),
                    T::settings_byo_reasoning_unset(lang),
                );
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
                ui.selectable_value(&mut form.form_reasoning_effort, "low".to_string(), "low");
            });
    });
    ui.label(
        egui::RichText::new(T::settings_byo_reasoning_hover_extra(lang))
            .small()
            .weak(),
    );

    // API key input
    // Note: このフィールドは write-only buffer 保存済み key は
    // security 上再表示しない (OS Keychain に暗号化保存、
    // memory 平文化を最小化) 起動毎に empty で始まるが正常動作
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(T::settings_byo_api_key_label(lang));
        ui.add(
            egui::TextEdit::singleline(&mut form.form_api_key)
                .password(true)
                .desired_width(320.0)
                .hint_text(T::settings_byo_api_key_hint(lang)),
        );
    });
    // Keychain 状態を色 + icon で prominent 化 (2026-09-04 UX 改善、
    // 「空欄 = 未保存」と誤読される report 対応 実際は Keychain 保存済)
    match keychain::get_api_key(provider.keychain_account()) {
        Ok(Some(_)) => {
            ui.label(
                egui::RichText::new(T::settings_byo_keychain_saved(lang))
                    .color(egui::Color32::from_rgb(0x2e, 0xa0, 0x43))
                    .strong(),
            );
        }
        Ok(None) => {
            ui.label(
                egui::RichText::new(T::settings_byo_keychain_unset(lang))
                    .color(ui.style().visuals.warn_fg_color)
                    .strong(),
            );
        }
        Err(_) => {
            ui.label(
                egui::RichText::new(T::settings_byo_keychain_error(lang))
                    .color(ui.style().visuals.error_fg_color)
                    .strong(),
            );
        }
    }

    // ── 5. Action buttons ─────────────────────────────────────
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        if ui.button(T::settings_save_button(lang)).clicked() {
            save_byo_llm_form(state, form, provider, lang);
        }
        if ui
            .button(T::settings_byo_test_button(lang))
            .on_hover_text(T::settings_byo_test_hover(lang))
            .clicked()
        {
            test_byo_llm(state, form, provider, lang);
        }
        if ui.button(T::settings_delete_button(lang)).clicked() {
            delete_byo_llm_form(state, form, provider, lang);
        }
        let is_saved = form
            .configured_providers
            .iter()
            .any(|s| s == provider.to_db_str());
        let is_active = form.active_provider == provider.to_db_str();
        if is_saved
            && !is_active
            && ui
                .button(T::settings_byo_activate_button(lang))
                .on_hover_text(T::settings_byo_activate_hover(lang))
                .clicked()
        {
            activate_byo_llm(state, form, provider, lang);
        }
    });

    // ── 6. Cost estimate ──────────────────────────────────────
    ui.add_space(6.0);
    match estimate_cost_per_generation(&form.form_model) {
        Some(usd) => {
            ui.label(
                egui::RichText::new(format!(
                    "{} ${usd:.4} {}",
                    T::settings_byo_cost_estimate_prefix(lang),
                    T::settings_byo_cost_estimate_suffix(lang),
                ))
                .small(),
            );
        }
        None => {
            ui.label(
                egui::RichText::new(T::settings_byo_cost_unknown_extra(lang))
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
fn load_provider_form(state: &AppState, form: &mut ByoLlmSettings, provider: OpenAiCompatProvider) {
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
    lang: Lang,
) {
    // Parse + validate
    let max_tokens = match form.form_max_tokens.trim().parse::<u32>() {
        Ok(v) if v > 0 => v,
        _ => {
            form.message = Some((T::settings_max_tokens_positive_int(lang).to_string(), false));
            return;
        }
    };
    let temperature = match form.form_temperature.trim().parse::<f32>() {
        Ok(v) if (0.0..=2.0).contains(&v) => v,
        _ => {
            form.message = Some((T::settings_temperature_range(lang).to_string(), false));
            return;
        }
    };
    if form.form_endpoint.trim().is_empty() {
        form.message = Some((T::settings_endpoint_empty(lang).to_string(), false));
        return;
    }
    if form.form_model.trim().is_empty() {
        form.message = Some((T::settings_model_empty(lang).to_string(), false));
        return;
    }

    // Save API key to Keychain if the user typed something in the input
    let api_key_typed = !form.form_api_key.trim().is_empty();
    if api_key_typed
        && let Err(e) = keychain::set_api_key(provider.keychain_account(), form.form_api_key.trim())
    {
        form.message = Some((
            format!("{}: {e}", T::settings_keychain_save_fail(lang)),
            false,
        ));
        return;
    }

    // Resolve API key from Keychain (either just-saved or previously-saved)
    let api_key = match keychain::get_api_key(provider.keychain_account()) {
        Ok(Some(k)) => k,
        Ok(None) => {
            form.message = Some((T::settings_api_key_missing(lang).to_string(), false));
            return;
        }
        Err(e) => {
            form.message = Some((
                format!("{}: {e}", T::settings_keychain_read_fail(lang)),
                false,
            ));
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
        form.message = Some((format!("{}: {e}", T::settings_db_save_fail(lang)), false));
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
    form.message = Some((T::settings_saved_ok(lang).to_string(), true));
}

/// Remove the saved config for a provider Deletes both the DB row and
/// the Keychain entry Idempotent — no error if either is already absent
fn delete_byo_llm_form(
    state: &mut AppState,
    form: &mut ByoLlmSettings,
    provider: OpenAiCompatProvider,
    lang: Lang,
) {
    if let Err(e) = state
        .db
        .delete_llm_provider_config(&state.profile_id, provider.to_db_str())
    {
        form.message = Some((format!("{}: {e}", T::settings_db_delete_fail(lang)), false));
        return;
    }
    if let Err(e) = keychain::delete_api_key(provider.keychain_account()) {
        form.message = Some((
            format!("{}: {e}", T::settings_keychain_delete_fail(lang)),
            false,
        ));
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
    form.message = Some((T::settings_deleted_ok(lang).to_string(), true));
}

/// Mark the given provider as the active generation target Persists the
/// choice in DB (`profiles.openai_compat_active_provider`) and
/// populates the runtime backend slot from the stored DB row + Keychain
fn activate_byo_llm(
    state: &mut AppState,
    form: &mut ByoLlmSettings,
    provider: OpenAiCompatProvider,
    lang: Lang,
) {
    if let Err(e) = state
        .db
        .set_openai_compat_active_provider(&state.profile_id, provider.to_db_str())
    {
        form.message = Some((
            format!("{}: {e}", T::settings_active_save_fail(lang)),
            false,
        ));
        return;
    }
    form.active_provider = provider.to_db_str().to_string();
    let row = match state
        .db
        .get_llm_provider_config(&state.profile_id, provider.to_db_str())
    {
        Ok(Some(r)) => r,
        _ => {
            form.message = Some((T::settings_provider_not_saved(lang).to_string(), false));
            return;
        }
    };
    let api_key = match keychain::get_api_key(provider.keychain_account()) {
        Ok(Some(k)) => k,
        Ok(None) => {
            form.message = Some((T::settings_no_api_key_saved(lang).to_string(), false));
            return;
        }
        Err(e) => {
            form.message = Some((
                format!("{}: {e}", T::settings_keychain_read_fail(lang)),
                false,
            ));
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
    form.message = Some((
        format!(
            "{} {}",
            provider.label(),
            T::settings_activated_ok_suffix(lang)
        ),
        true,
    ));
}

/// Fire a short synthetic generation request against the currently-
/// edited form config Blocks the UI thread for up to 30 s (acceptable
/// for a manual Test button) API key resolution: form input takes
/// priority; falls back to Keychain-stored value
fn test_byo_llm(
    state: &AppState,
    form: &mut ByoLlmSettings,
    provider: OpenAiCompatProvider,
    lang: Lang,
) {
    let api_key = if !form.form_api_key.trim().is_empty() {
        form.form_api_key.trim().to_string()
    } else {
        match keychain::get_api_key(provider.keychain_account()) {
            Ok(Some(k)) => k,
            _ => {
                form.message = Some((T::settings_api_key_both_missing(lang).to_string(), false));
                return;
            }
        }
    };
    if form.form_endpoint.trim().is_empty() {
        form.message = Some((T::settings_endpoint_empty(lang).to_string(), false));
        return;
    }
    if form.form_model.trim().is_empty() {
        form.message = Some((T::settings_model_empty(lang).to_string(), false));
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
            (
                format!("{} {preview}", T::settings_test_success_prefix(lang)),
                true,
            )
        }
        Ok(Err(e)) => (format!("{} {e}", T::settings_test_fail_prefix(lang)), false),
        Err(_) => (T::settings_test_timeout(lang).to_string(), false),
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

fn apply_license(state: &mut AppState, settings: &mut SettingsState, lang: Lang) {
    let input = settings.license_input.trim();

    // Base64 デコード
    let key = match LicenseKey::from_base64(input) {
        Ok(k) => k,
        Err(e) => {
            settings.license_message =
                Some((format!("{}: {e}", T::settings_license_invalid(lang)), false));
            return;
        }
    };

    // 署名検証
    let verifier = match LicenseVerifier::new(&LICENSE_PUBLIC_KEY) {
        Ok(v) => v,
        Err(_) => {
            settings.license_message =
                Some((T::settings_license_system_missing(lang).to_string(), false));
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
                format!(
                    "{tier:?} {} ({} {expires})",
                    T::settings_plan_updated_updated(lang),
                    T::settings_plan_updated_expires(lang),
                ),
                true,
            ));
            tracing::info!(tier = ?tier, expires = %expires, "license applied");
        }
        Err(e) => {
            settings.license_message = Some((
                format!("{}: {e}", T::settings_license_verify_fail(lang)),
                false,
            ));
        }
    }
}

/// Revert to Free tier — clears the stored license_key so subsequent
/// starts don't reinstate the paid tier from DB (Phase S2 UX for testing
/// / user-requested cancel)
fn clear_license(state: &mut AppState, settings: &mut SettingsState, lang: Lang) {
    let free_str = format!("{:?}", Tier::Free);
    match state
        .db
        .update_profile_tier(&state.profile_id, &free_str, "")
    {
        Ok(()) => {
            state.tier = Tier::Free;
            settings.license_input.clear();
            settings.license_message =
                Some((T::settings_license_reverted_free(lang).to_string(), true));
            tracing::info!("license cleared, tier reverted to Free");
        }
        Err(e) => {
            settings.license_message =
                Some((format!("{}: {e}", T::settings_db_update_fail(lang)), false));
        }
    }
}

/// Kick off a checkout request to the CF Workers backend and open the
/// returned Stripe URL in the user's browser Runs the network call on
/// the shared tokio runtime so the UI thread is not blocked
fn spawn_checkout(state: &AppState, settings: &mut SettingsState, plan: &str, lang: Lang) {
    let email = settings.checkout_email.trim().to_string();
    if !is_plausible_email(&email) {
        settings.checkout_message =
            Some((T::settings_email_invalid_short(lang).to_string(), false));
        return;
    }
    settings.checkout_message = Some((
        format!("{plan} {}", T::settings_checkout_fetching(lang)),
        true,
    ));

    let endpoint = checkout_endpoint();
    let body = CheckoutRequestBody {
        plan: plan.to_string(),
        user_email: email,
    };
    let tx = state.result_tx.clone();

    state.runtime.spawn(async move {
        let outcome = fetch_and_open_checkout(&endpoint, &body, lang).await;
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
    lang: Lang,
) -> Result<String, String> {
    let client = reqwest::Client::new();
    let resp = client
        .post(endpoint)
        .json(body)
        .send()
        .await
        .map_err(|e| format!("{}: {e}", T::settings_http_error(lang)))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("backend {status}: {text}"));
    }
    let parsed: CheckoutResponseBody = resp
        .json()
        .await
        .map_err(|e| format!("{}: {e}", T::settings_response_parse_fail(lang)))?;
    open::that(&parsed.url)
        .map_err(|e| format!("{}: {e}", T::settings_browser_launch_fail(lang)))?;
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
