use egui::Ui;
use std::time::{Duration, Instant};

use crate::i18n::Lang;
use crate::state::{AppState, GenerationMessage, GenerationPhase, GenerationStatus, PhaseProgress};
use text_to_print_core::db::GenerationRecord;
use text_to_print_core::pipeline::{self, ExportFormat, Quality};
use text_to_print_llm::{backend, prompt};

/// Format offered by the export dropdown. Some entries map to a supported
/// [`ExportFormat`]; the rest are surfaced grey-out so users see the roadmap
/// without silently succeeding on formats we do not yet write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiExportFormat {
    ThreeMf,
    Stl,
    Obj,
    Fbx,
    Step,
    Gcode,
}

impl UiExportFormat {
    pub const ALL: [Self; 6] = [
        Self::ThreeMf,
        Self::Stl,
        Self::Obj,
        Self::Fbx,
        Self::Step,
        Self::Gcode,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::ThreeMf => "3MF",
            Self::Stl => "STL",
            Self::Obj => "OBJ",
            Self::Fbx => "FBX",
            Self::Step => "STEP",
            Self::Gcode => "G-code",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::ThreeMf => "3mf",
            Self::Stl => "stl",
            Self::Obj => "obj",
            Self::Fbx => "fbx",
            Self::Step => "step",
            Self::Gcode => "gcode",
        }
    }

    pub fn to_supported(self) -> Option<ExportFormat> {
        match self {
            Self::ThreeMf => Some(ExportFormat::ThreeMf),
            Self::Stl => Some(ExportFormat::Stl),
            Self::Fbx => Some(ExportFormat::Fbx),
            Self::Obj | Self::Step | Self::Gcode => None,
        }
    }
}

pub struct PromptUiState {
    pub export_format: UiExportFormat,
    pub last_export_error: Option<String>,
}

impl Default for PromptUiState {
    fn default() -> Self {
        Self {
            export_format: UiExportFormat::ThreeMf,
            last_export_error: None,
        }
    }
}

pub fn show(ui: &mut Ui, state: &mut AppState, ui_state: &mut PromptUiState, lang: Lang) {
    poll_results(ui, state);

    ui.heading("Text to 3D");
    ui.separator();

    if !state.model_ready {
        show_model_download(ui, state);
        ui.add_space(4.0);
    }

    let limits = state.tier.limits();
    let usage = state.daily_usage();
    ui.label(format!(
        "本日の生成: {} / {} 回",
        usage, limits.daily_generations
    ));

    ui.add_space(4.0);
    ui.label("3D モデルの説明を入力 (Enter で生成 / Shift+Enter で改行):");

    let is_generating = matches!(state.generation_status, GenerationStatus::Generating);
    let can_gen = state.can_generate() && !state.prompt_input.trim().is_empty();

    let prompt_id = egui::Id::new("prompt_input");
    let prompt_widget = egui::TextEdit::multiline(&mut state.prompt_input)
        .id(prompt_id)
        .desired_rows(3)
        .desired_width(f32::INFINITY)
        .hint_text("例: 20mm の立方体、上面に直径 5mm の穴");
    let prompt_response = ui.add_enabled(!is_generating, prompt_widget);

    let enter_pressed = prompt_response.has_focus()
        && ui.input(|i| {
            i.events.iter().any(|e| {
                matches!(
                    e,
                    egui::Event::Key {
                        key: egui::Key::Enter,
                        pressed: true,
                        modifiers,
                        ..
                    } if !modifiers.shift
                )
            })
        });

    if enter_pressed && can_gen && !is_generating {
        if state.prompt_input.ends_with('\n') {
            state.prompt_input.pop();
        }
        start_generation(state, lang);
    }

    if !state.prompt_focused_once {
        ui.memory_mut(|m| m.request_focus(prompt_id));
        state.prompt_focused_once = true;
    }

    ui.add_space(8.0);

    if ui
        .add_enabled(!is_generating && can_gen, egui::Button::new("生成"))
        .clicked()
    {
        start_generation(state, lang);
    }

    if !state.can_generate() && !is_generating {
        ui.colored_label(egui::Color32::YELLOW, "本日の生成上限に達しました");
    }

    ui.add_space(8.0);

    if is_generating || !state.phase_progress.completed.is_empty() {
        show_phase_progress(ui, &state.phase_progress);
        ui.add_space(8.0);
    }

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

                if let Some(overhang) = &stats.overhang_summary {
                    ui.label(format!(
                        "オーバーハング: {:.1}% ({} / {} 面) 最大壁角 {:.1}°",
                        overhang.overhang_ratio * 100.0,
                        overhang.overhang_face_count,
                        overhang.total_face_count,
                        overhang.max_wall_angle_deg,
                    ));
                }
                if let Some(safety) = &stats.safety_summary {
                    let color = if safety.is_safe {
                        egui::Color32::GREEN
                    } else {
                        egui::Color32::YELLOW
                    };
                    ui.colored_label(
                        color,
                        format!(
                            "安全性 ({}): {} / 反り {}",
                            safety.material_name,
                            if safety.is_safe { "OK" } else { "要注意" },
                            safety.warp_category,
                        ),
                    );
                    for msg in &safety.messages {
                        ui.colored_label(egui::Color32::YELLOW, format!("  {msg}"));
                    }
                }
            }

            ui.collapsing("LOL ソース", |ui| {
                ui.monospace(lol_source);
            });

            if state.tier.limits().can_download {
                ui.add_space(4.0);
                let lol = lol_source.clone();
                show_export_dropdown(ui, state, ui_state, &lol, lang);
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

    if let Some(err) = &ui_state.last_export_error {
        ui.add_space(4.0);
        ui.colored_label(egui::Color32::RED, format!("エクスポート失敗: {err}"));
    }
}

fn show_model_download(ui: &mut Ui, state: &AppState) {
    let progress = state.model_progress.borrow().clone();
    match progress.status {
        text_to_print_llm::downloader::DownloadStatus::Downloading => {
            ui.colored_label(egui::Color32::YELLOW, "LLM モデルをダウンロード中...");
            if let Some(total) = progress.total_bytes {
                #[allow(clippy::cast_precision_loss)]
                let pct = progress.downloaded_bytes as f32 / total as f32;
                #[allow(clippy::cast_precision_loss)]
                let done_mb = progress.downloaded_bytes as f64 / 1_048_576.0;
                #[allow(clippy::cast_precision_loss)]
                let total_mb = total as f64 / 1_048_576.0;
                ui.add(
                    egui::ProgressBar::new(pct).text(format!("{done_mb:.0} / {total_mb:.0} MB")),
                );
            } else {
                ui.spinner();
            }
            ui.ctx().request_repaint();
        }
        text_to_print_llm::downloader::DownloadStatus::Complete => {}
        text_to_print_llm::downloader::DownloadStatus::Error(ref e) => {
            ui.colored_label(egui::Color32::RED, format!("モデルDLエラー: {e}"));
        }
        text_to_print_llm::downloader::DownloadStatus::Pending => {
            ui.label("モデル準備中...");
            ui.ctx().request_repaint();
        }
    }
}

fn show_phase_progress(ui: &mut Ui, progress: &PhaseProgress) {
    let total = GenerationPhase::ALL.len();
    let done = progress.completed.len();
    #[allow(clippy::cast_precision_loss)]
    let ratio = done as f32 / total as f32;
    ui.add(egui::ProgressBar::new(ratio).show_percentage());

    egui::Grid::new("phase_grid")
        .num_columns(3)
        .spacing([8.0, 2.0])
        .show(ui, |ui| {
            for phase in GenerationPhase::ALL {
                let (icon, color) = if progress.is_done(phase) {
                    ("✓", egui::Color32::LIGHT_GREEN)
                } else if progress.current == Some(phase) {
                    ("…", egui::Color32::YELLOW)
                } else {
                    ("·", egui::Color32::GRAY)
                };
                ui.colored_label(color, icon);
                ui.monospace(phase.label());
                let latency = progress
                    .latency_of(phase)
                    .map(|d| format!("{} ms", d.as_millis()))
                    .unwrap_or_default();
                ui.label(latency);
                ui.end_row();
            }
        });

    if progress.retry_count > 0 {
        ui.colored_label(
            egui::Color32::YELLOW,
            format!("LLM retry: {}", progress.retry_count),
        );
    }
}

fn show_export_dropdown(
    ui: &mut Ui,
    state: &mut AppState,
    ui_state: &mut PromptUiState,
    lol_source: &str,
    lang: Lang,
) {
    ui.horizontal(|ui| {
        ui.label("エクスポート形式:");
        egui::ComboBox::from_id_salt("export_format")
            .selected_text(display_label(ui_state.export_format))
            .show_ui(ui, |ui| {
                for fmt in UiExportFormat::ALL {
                    let text = display_label(fmt);
                    if fmt.to_supported().is_some() {
                        ui.selectable_value(&mut ui_state.export_format, fmt, text);
                    } else {
                        ui.add_enabled(false, egui::Label::new(text));
                    }
                }
            });

        let supported = ui_state.export_format.to_supported();
        let can_export = supported.is_some();
        if ui
            .add_enabled(can_export, egui::Button::new("保存..."))
            .clicked()
            && let Some(fmt) = supported
        {
            run_export(
                state,
                ui_state,
                lol_source,
                ui_state.export_format,
                fmt,
                lang,
            );
        }
    });
}

fn display_label(fmt: UiExportFormat) -> String {
    if fmt.to_supported().is_some() {
        fmt.label().to_string()
    } else {
        format!("{} (未対応)", fmt.label())
    }
}

fn run_export(
    state: &AppState,
    ui_state: &mut PromptUiState,
    lol_source: &str,
    ui_fmt: UiExportFormat,
    fmt: ExportFormat,
    lang: Lang,
) {
    let default_name = format!("text-to-print.{}", ui_fmt.extension());
    let target = rfd::FileDialog::new()
        .set_file_name(&default_name)
        .add_filter(ui_fmt.label(), &[ui_fmt.extension()])
        .save_file();

    let Some(target) = target else {
        return;
    };

    let output_dir = target
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| state.data_dir.join("exports"));
    let _ = std::fs::create_dir_all(&output_dir);

    let meta = pipeline::MetadataInputs {
        prompt: state.prompt_input.as_str(),
        prompt_lang: lang.as_bcp47(),
        llm_model: state.llm_config.model_choice.model_id(),
        llm_seed: None,
        retry_count: state.phase_progress.retry_count,
        safety_violations: vec![],
    };

    let export_result =
        pipeline::export_mesh_with_metadata(lol_source, &output_dir, fmt, Quality::High, meta);

    match export_result {
        Ok((stats, _manifest)) => {
            let src = std::path::PathBuf::from(&stats.path);
            if src != target
                && let Err(e) = std::fs::rename(&src, &target)
            {
                if std::fs::copy(&src, &target).is_ok() {
                    let _ = std::fs::remove_file(&src);
                } else {
                    ui_state.last_export_error =
                        Some(format!("rename to {}: {e}", target.display()));
                    return;
                }
            }
            ui_state.last_export_error = None;
            tracing::info!(
                path = %target.display(),
                triangles = stats.triangle_count,
                "mesh exported"
            );
            if let Some(parent) = target.parent() {
                let _ = open::that(parent);
            }
        }
        Err(e) => {
            ui_state.last_export_error = Some(e.to_string());
            tracing::error!(error = %e, "mesh export failed");
        }
    }
}

fn start_generation(state: &mut AppState, _lang: Lang) {
    let gen_id = uuid::Uuid::now_v7().to_string();
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
    state.phase_progress.reset();

    let config = state.llm_config.clone();
    let tx = state.result_tx.clone();
    let id = gen_id;
    let output_dir = state.data_dir.join("exports");
    let can_download = state.tier.limits().can_download;

    state.runtime.spawn(async move {
        let _ = tx.send(GenerationMessage::PhaseStart(GenerationPhase::Llm));
        let llm_start = Instant::now();
        let result = backend::generate(&config, prompt::SYSTEM_PROMPT, &prompt_text).await;
        let llm_elapsed = llm_start.elapsed();
        let _ = tx.send(GenerationMessage::PhaseDone(
            GenerationPhase::Llm,
            llm_elapsed,
        ));

        match result {
            Ok(response) => {
                let _ = tx.send(GenerationMessage::PhaseStart(GenerationPhase::Parse));
                let parse_start = Instant::now();
                let lol = pipeline::extract_lol(&response).unwrap_or_else(|| response.clone());
                let _ = tx.send(GenerationMessage::PhaseDone(
                    GenerationPhase::Parse,
                    parse_start.elapsed(),
                ));

                let mesh_stats = if can_download {
                    let _ = std::fs::create_dir_all(&output_dir);
                    let _ = tx.send(GenerationMessage::PhaseStart(GenerationPhase::Mesh));
                    let mesh_start = Instant::now();
                    let stats_result = pipeline::export_mesh(
                        &lol,
                        &output_dir,
                        ExportFormat::ThreeMf,
                        Quality::Preview,
                    );
                    let mesh_elapsed = mesh_start.elapsed();
                    let _ = tx.send(GenerationMessage::PhaseDone(
                        GenerationPhase::Mesh,
                        mesh_elapsed,
                    ));

                    // Safety runs inside `export_mesh_via_bamboo`; we surface a
                    // zero-cost tick so the bar advances. When the pipeline is
                    // split into a Safety-only stage we can measure it here.
                    let _ = tx.send(GenerationMessage::PhaseStart(GenerationPhase::Safety));
                    let _ = tx.send(GenerationMessage::PhaseDone(
                        GenerationPhase::Safety,
                        Duration::ZERO,
                    ));

                    let _ = tx.send(GenerationMessage::PhaseStart(GenerationPhase::Export));
                    let export_start = Instant::now();
                    let out = stats_result.ok();
                    let _ = tx.send(GenerationMessage::PhaseDone(
                        GenerationPhase::Export,
                        export_start.elapsed(),
                    ));
                    out
                } else {
                    for p in [
                        GenerationPhase::Mesh,
                        GenerationPhase::Safety,
                        GenerationPhase::Export,
                    ] {
                        let _ = tx.send(GenerationMessage::PhaseStart(p));
                        let _ = tx.send(GenerationMessage::PhaseDone(p, Duration::ZERO));
                    }
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
            GenerationMessage::PhaseStart(phase) => {
                state.phase_progress.current = Some(phase);
                ui.ctx().request_repaint();
            }
            GenerationMessage::PhaseDone(phase, dur) => {
                state.phase_progress.completed.push((phase, dur));
                if state.phase_progress.current == Some(phase) {
                    state.phase_progress.current = None;
                }
                ui.ctx().request_repaint();
            }
            GenerationMessage::Success {
                id,
                lol_source,
                mesh_stats,
            } => {
                let _ = state
                    .db
                    .update_generation_status(&id, "complete", Some(&lol_source), None);
                state.current_lol = Some(lol_source.clone());

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
