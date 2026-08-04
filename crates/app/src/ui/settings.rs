use egui::Ui;

use crate::state::AppState;
use text_to_print_core::license::{LicenseKey, LicenseVerifier};
use text_to_print_llm::backend_kind::{BackendKind, ExecutionMode};
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
        // Stage 3-C.6: inference backend picker
        ui.label("Inference backend:");
        let current_kind = state.backend_kind;
        let mut new_kind = current_kind;
        ui.horizontal(|ui| {
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
                egui::Color32::YELLOW,
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
