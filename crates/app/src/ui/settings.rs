use egui::Ui;

use crate::state::AppState;
use tdvbgaran_core::license::{LicenseKey, LicenseVerifier};

// TODO: 本番用の公開鍵に差し替え（LicenseIssuer::public_key_bytes() の出力）
const LICENSE_PUBLIC_KEY: [u8; 32] = [0u8; 32];

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

        if ui.button("ライセンスを適用").clicked() && !settings.license_input.trim().is_empty() {
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
        ui.label("Endpoint:");
        ui.text_edit_singleline(&mut state.llm_config.endpoint);
        ui.label("Model:");
        ui.text_edit_singleline(&mut state.llm_config.model);
        ui.add_space(4.0);
        ui.label(format!("Temperature: {:.1}", state.llm_config.temperature));
        ui.add(egui::Slider::new(&mut state.llm_config.temperature, 0.0..=2.0));
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
            let _ = state.db.update_profile_tier(&state.profile_id, &tier_str, input);
            state.tier = tier;

            settings.license_message = Some((
                format!("{tier:?} プランに更新しました"),
                true,
            ));
            tracing::info!(tier = ?tier, "license applied");
        }
        Err(e) => {
            settings.license_message = Some((format!("検証失敗: {e}"), false));
        }
    }
}
