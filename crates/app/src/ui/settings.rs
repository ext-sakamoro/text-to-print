use egui::Ui;

use crate::state::AppState;
use text_to_print_core::license::{LicenseKey, LicenseVerifier};
use text_to_print_llm::model::ModelChoice;

const LICENSE_PUBLIC_KEY: [u8; 32] = [
    0x05, 0x7f, 0x9c, 0x5f, 0xdb, 0x3f, 0x6f, 0x93, 0x69, 0x11, 0xd9, 0x16, 0x85, 0xb1, 0x82, 0x5b,
    0xcb, 0x3a, 0x75, 0xff, 0x18, 0x39, 0x12, 0xd5, 0x26, 0x6e, 0xf4, 0x75, 0x34, 0xec, 0xae, 0xc2,
];

#[derive(Default)]
pub struct SettingsState {
    pub license_input: String,
    pub license_message: Option<(String, bool)>,
}

pub fn show(ui: &mut Ui, state: &mut AppState, settings: &mut SettingsState) {
    ui.heading("Settings");
    ui.separator();

    // ライセンス
    ui.collapsing("License", |ui| {
        ui.label(format!("現在のティア: {:?}", state.tier));
        ui.add_space(4.0);

        ui.label("ライセンスキー:");
        ui.text_edit_multiline(&mut settings.license_input);

        if ui.button("ライセンスを適用").clicked() && !settings.license_input.trim().is_empty()
        {
            apply_license(state, settings);
        }

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
        ui.label("Endpoint (alice-llm-server):");
        ui.text_edit_singleline(&mut state.llm_config.endpoint);

        ui.add_space(4.0);
        ui.label("Model:");
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

        ui.add_space(4.0);
        ui.label(format!("Temperature: {:.1}", state.llm_config.temperature));
        ui.add(egui::Slider::new(
            &mut state.llm_config.temperature,
            0.0..=2.0,
        ));
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
    });
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

            // DB に保存
            let _ = state
                .db
                .update_profile_tier(&state.profile_id, &tier_str, input);
            state.tier = tier;

            settings.license_message = Some((format!("{tier:?} プランに更新しました"), true));
            tracing::info!(tier = ?tier, "license applied");
        }
        Err(e) => {
            settings.license_message = Some((format!("検証失敗: {e}"), false));
        }
    }
}
