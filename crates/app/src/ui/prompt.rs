use egui::Ui;

use crate::state::{AppState, GenerationMessage, GenerationStatus};
use text_to_print_core::db::GenerationRecord;
use text_to_print_core::pipeline::{self, ExportFormat, Quality};
use text_to_print_llm::{backend, prompt};

pub fn show(ui: &mut Ui, state: &mut AppState) {
    poll_results(ui, state);

    ui.heading("Text to 3D");
    ui.separator();

    // モデルダウンロード進捗
    if !state.model_ready {
        let progress = state.model_progress.borrow().clone();
        match progress.status {
            text_to_print_llm::downloader::DownloadStatus::Downloading => {
                ui.colored_label(egui::Color32::YELLOW, "LLM モデルをダウンロード中...");
                if let Some(total) = progress.total_bytes {
                    let pct = progress.downloaded_bytes as f32 / total as f32;
                    ui.add(egui::ProgressBar::new(pct).text(format!(
                        "{:.0} / {:.0} MB",
                        progress.downloaded_bytes as f64 / 1_048_576.0,
                        total as f64 / 1_048_576.0,
                    )));
                } else {
                    ui.spinner();
                }
                ui.ctx().request_repaint();
            }
            text_to_print_llm::downloader::DownloadStatus::Complete => {
                state.model_ready = true;
            }
            text_to_print_llm::downloader::DownloadStatus::Error(ref e) => {
                ui.colored_label(egui::Color32::RED, format!("モデルDLエラー: {e}"));
            }
            text_to_print_llm::downloader::DownloadStatus::Pending => {
                ui.label("モデル準備中...");
                ui.ctx().request_repaint();
            }
        }
        ui.add_space(4.0);
    }

    let limits = state.tier.limits();
    let usage = state.daily_usage();
    ui.label(format!(
        "本日の生成: {} / {} 回",
        usage, limits.daily_generations
    ));

    ui.add_space(4.0);
    ui.label("3D モデルの説明を入力:");
    ui.text_edit_multiline(&mut state.prompt_input);

    ui.add_space(8.0);

    let is_generating = matches!(state.generation_status, GenerationStatus::Generating);
    let can_gen = state.can_generate() && !state.prompt_input.trim().is_empty();

    if ui
        .add_enabled(!is_generating && can_gen, egui::Button::new("生成"))
        .clicked()
    {
        start_generation(state);
    }

    if !state.can_generate() && !is_generating {
        ui.colored_label(egui::Color32::YELLOW, "本日の生成上限に達しました");
    }

    ui.add_space(8.0);

    match &state.generation_status {
        GenerationStatus::Idle => {}
        GenerationStatus::Generating => {
            ui.spinner();
            ui.label("生成中...");
        }
        GenerationStatus::Done {
            lol_source,
            mesh_stats,
        } => {
            ui.colored_label(egui::Color32::GREEN, "生成完了");

            if let Some(stats) = mesh_stats {
                ui.label(format!(
                    "頂点数: {} / 三角形数: {}",
                    stats.vertex_count, stats.triangle_count
                ));
                ui.label(format!("保存先: {}", stats.path));
            }

            ui.collapsing("LOL ソース", |ui| {
                ui.monospace(lol_source);
            });

            // エクスポートボタン
            if state.tier.limits().can_download {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let lol = lol_source.clone();
                    if ui.button("3MF").clicked() {
                        export(state, &lol, ExportFormat::ThreeMf);
                    }
                    if ui.button("FBX").clicked() {
                        export(state, &lol, ExportFormat::Fbx);
                    }
                    if ui.button("STL").clicked() {
                        export(state, &lol, ExportFormat::Stl);
                    }
                });
            } else {
                ui.colored_label(
                    egui::Color32::GRAY,
                    "ダウンロードには General 以上のプランが必要です",
                );
            }
        }
        GenerationStatus::Error(msg) => {
            ui.colored_label(egui::Color32::RED, format!("エラー: {msg}"));
        }
    }
}

fn export(state: &AppState, lol_source: &str, format: ExportFormat) {
    let output_dir = state.data_dir.join("exports");
    let _ = std::fs::create_dir_all(&output_dir);

    match pipeline::export_mesh(lol_source, &output_dir, format, Quality::High) {
        Ok(stats) => {
            tracing::info!(
                path = %stats.path,
                triangles = stats.triangle_count,
                "mesh exported"
            );
            // ファイルダイアログで保存先を開く
            if let Some(parent) = std::path::Path::new(&stats.path).parent() {
                let _ = open::that(parent);
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "mesh export failed");
        }
    }
}

fn start_generation(state: &mut AppState) {
    let gen_id = uuid::Uuid::new_v4().to_string();
    let prompt_text = state.prompt_input.trim().to_string();
    let is_public = state.tier.limits().force_public;

    let _ = state.db.insert_generation(&GenerationRecord {
        id: &gen_id,
        profile_id: &state.profile_id,
        prompt: &prompt_text,
        lol_source: None,
        sdf_data: None,
        quality: "preview",
        status: "pending",
        is_public,
    });
    let _ = state
        .db
        .increment_daily_usage(&state.profile_id, &state.today());

    state.generation_status = GenerationStatus::Generating;

    let config = state.llm_config.clone();
    let tx = state.result_tx.clone();
    let id = gen_id;
    let output_dir = state.data_dir.join("exports");
    let can_download = state.tier.limits().can_download;

    state.runtime.spawn(async move {
        let result = backend::generate(&config, prompt::SYSTEM_PROMPT, &prompt_text).await;

        match result {
            Ok(response) => {
                let lol = pipeline::extract_lol(&response).unwrap_or_else(|| response.clone());

                // メッシュ生成（DL可能な場合）
                let mesh_stats = if can_download {
                    let _ = std::fs::create_dir_all(&output_dir);
                    pipeline::export_mesh(
                        &lol,
                        &output_dir,
                        ExportFormat::ThreeMf,
                        Quality::Preview,
                    )
                    .ok()
                } else {
                    None
                };

                let _ = tx.send(GenerationMessage::Success {
                    id,
                    lol_source: lol,
                    mesh_stats,
                });
            }
            Err(e) => {
                let _ = tx.send(GenerationMessage::Failure {
                    id,
                    error: e.to_string(),
                });
            }
        }
    });
}

fn poll_results(ui: &egui::Ui, state: &mut AppState) {
    while let Ok(msg) = state.result_rx.try_recv() {
        match msg {
            GenerationMessage::Success {
                id,
                lol_source,
                mesh_stats,
            } => {
                let _ = state
                    .db
                    .update_generation_status(&id, "complete", Some(&lol_source), None);
                state.current_lol = Some(lol_source.clone());

                // General tier: SDF 公開義務 → P2P に自動公開
                if state.tier.limits().force_public {
                    state.pending_publish =
                        Some((id.clone(), lol_source.clone(), state.prompt_input.clone()));
                }

                state.generation_status = GenerationStatus::Done {
                    lol_source,
                    mesh_stats,
                };
                state.refresh_history();
                ui.ctx().request_repaint();
            }
            GenerationMessage::Failure { id, error } => {
                let _ = state
                    .db
                    .update_generation_status(&id, "error", None, Some(&error));
                state.generation_status = GenerationStatus::Error(error);
                state.refresh_history();
                ui.ctx().request_repaint();
            }
        }
    }
}
