use egui::Ui;
use std::time::{Duration, Instant};

use crate::i18n::Lang;
use crate::state::{AppState, GenerationMessage, GenerationPhase, GenerationStatus, PhaseProgress};
use text_to_print_core::db::GenerationRecord;
use text_to_print_core::manifest::tier_slug;
use text_to_print_core::pipeline::{self, ExportFormat, Quality};
use text_to_print_llm::sidecar::SidecarStatus;
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
            Self::Step => Some(ExportFormat::Step),
            Self::Gcode => Some(ExportFormat::Gcode),
            Self::Obj => None,
        }
    }
}

pub struct PromptUiState {
    pub export_format: UiExportFormat,
    pub last_export_error: Option<String>,
    /// GAP-B: 4-color export (Bambu AMS) config
    pub color4: Color4UiState,
}

impl Default for PromptUiState {
    fn default() -> Self {
        Self {
            export_format: UiExportFormat::ThreeMf,
            last_export_error: None,
            color4: Color4UiState::default(),
        }
    }
}

/// 4-color export UI state (GAP-B) Backing store for the collapsible
/// "4-color multi-filament" panel wiring `pipeline::export_mesh_color4`
/// to the Bambu AMS workflow
pub struct Color4UiState {
    pub n_colors: u8,
    pub projection: alice_bamboo::color4::ProjectionAxis,
    pub image_path: Option<std::path::PathBuf>,
    pub last_error: Option<String>,
    pub last_generated: Vec<std::path::PathBuf>,
}

impl Default for Color4UiState {
    fn default() -> Self {
        Self {
            n_colors: 4,
            projection: alice_bamboo::color4::ProjectionAxis::PositiveZ,
            image_path: None,
            last_error: None,
            last_generated: Vec::new(),
        }
    }
}

fn projection_label(p: alice_bamboo::color4::ProjectionAxis) -> &'static str {
    use alice_bamboo::color4::ProjectionAxis;
    match p {
        ProjectionAxis::PositiveZ => "+Z 正面 (front)",
        ProjectionAxis::NegativeZ => "-Z 背面 (back)",
        ProjectionAxis::PositiveY => "+Y 上面 (top)",
        ProjectionAxis::NegativeY => "-Y 底面 (bottom)",
        ProjectionAxis::PositiveX => "+X 右側面 (right)",
        ProjectionAxis::NegativeX => "-X 左側面 (left)",
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

    show_sidecar_status(ui, state);

    let limits = state.tier.limits();
    let usage = state.daily_usage();
    if limits.daily_generations == u32::MAX {
        // v0.1.0-beta.1: Free tier restriction 撤廃、'/ 4294967295' 表示
        // は醜いので usage 件数のみ表示
        ui.label(format!("本日の生成: {usage} 回 (β 制限なし)"));
    } else {
        ui.label(format!(
            "本日の生成: {} / {} 回",
            usage, limits.daily_generations
        ));
    }

    ui.add_space(4.0);
    ui.label("3D モデルの説明を入力 (Enter で生成 / Shift+Enter で改行):");

    let is_generating = matches!(state.generation_status, GenerationStatus::Generating);
    let sidecar_running = state.sidecar_status.borrow().is_running();
    let can_gen = state.can_generate() && !state.prompt_input.trim().is_empty() && sidecar_running;

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

    ui.add_space(4.0);
    show_prompt_templates(ui, state, is_generating);

    ui.add_space(4.0);
    show_prompt_customizer(ui, state, is_generating);

    ui.add_space(8.0);

    if ui
        .add_enabled(!is_generating && can_gen, egui::Button::new("生成"))
        .clicked()
    {
        start_generation(state, lang);
    }

    if !state.can_generate() && !is_generating {
        let warn_color = ui.style().visuals.warn_fg_color;
        ui.colored_label(warn_color, "本日の生成上限に達しました");
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
                ui.label(format!("保存先: {}", anonymize_home(&stats.path)));

                if let Some(overhang) = &stats.overhang_summary {
                    ui.label(format!(
                        "オーバーハング: {:.1}% ({} / {} 面) 最大壁角 {:.1}°",
                        overhang.overhang_ratio * 100.0,
                        overhang.overhang_face_count,
                        overhang.total_face_count,
                        overhang.max_wall_angle_deg,
                    ));
                }
                if let Some(slice) = &stats.slice_summary {
                    let mins = (slice.print_time_seconds / 60.0).round() as u32;
                    ui.label(format!(
                        "G-code: {} 層 / 推定 {} 分 / フィラメント {:.2} m",
                        slice.layer_count, mins, slice.filament_meters,
                    ));
                }
                if let Some(safety) = &stats.safety_summary {
                    let warn_color = ui.style().visuals.warn_fg_color;
                    let color = if safety.is_safe {
                        egui::Color32::GREEN
                    } else {
                        warn_color
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
                        ui.colored_label(warn_color, format!("  {msg}"));
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
                ui.add_space(4.0);
                show_color4_export(ui, state, ui_state, &lol);
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
            // theme-adaptive warn color (light/dark 両テーマで readable、
            // egui native YELLOW は light theme で contrast 不足)
            let warn_color = ui.style().visuals.warn_fg_color;
            ui.colored_label(warn_color, "LLM モデルをダウンロード中...");
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

/// `alice-llm-server` sidecar プロセスの状態を表示
///
/// Running 時は何も出さない (通常運用時の視覚ノイズを避ける)
/// Waiting/Starting は spinner、Error はメッセージ + 起動 hint を表示
fn show_sidecar_status(ui: &mut Ui, state: &AppState) {
    let status = state.sidecar_status.borrow().clone();
    match status {
        SidecarStatus::Waiting => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("LLM 起動待機中 (モデル準備完了後に自動起動)");
            });
            ui.ctx().request_repaint();
            ui.add_space(4.0);
        }
        SidecarStatus::Starting => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("LLM sidecar を起動中...");
            });
            ui.ctx().request_repaint();
            ui.add_space(4.0);
        }
        SidecarStatus::Running => {
            // 通常運用時は非表示
        }
        SidecarStatus::Error(msg) => {
            ui.colored_label(egui::Color32::RED, format!("LLM sidecar 起動失敗: {msg}"));
            ui.label(
                "対処: `cargo install --path ~/ALICE-LLM --features server` で \
                 alice-llm-server を PATH に配置、または Settings の Endpoint に \
                 既存の OpenAI 互換 endpoint (例: Ollama) を指定してください",
            );
            ui.add_space(4.0);
        }
    }
}

/// Replace the user's `$HOME` prefix in a file path with `~` so the UI
/// doesn't leak their username in "保存先" / preview labels When `$HOME`
/// isn't set or doesn't match, returns the input unchanged
fn anonymize_home(path: &str) -> String {
    if let Some(home) = std::env::var_os("HOME").and_then(|h| h.into_string().ok())
        && let Some(rest) = path.strip_prefix(&home)
    {
        return format!("~{rest}");
    }
    path.to_string()
}

fn show_phase_progress(ui: &mut Ui, progress: &PhaseProgress) {
    let total = GenerationPhase::ALL.len();
    let done = progress.completed.len();
    let elapsed = progress.elapsed();
    let in_flight = progress.current.is_some();

    // Fake sub-progress during the in-flight phase so the bar visibly moves
    // instead of sitting at 0% for the entire LLM inference (~100-500 sec
    // on iGPU) The blend is 90% of the current phase share, capped just
    // shy of the next phase boundary so completion snaps forward
    #[allow(clippy::cast_precision_loss)]
    let phase_share = 1.0 / total as f32;
    #[allow(clippy::cast_precision_loss)]
    let base_ratio = done as f32 / total as f32;
    let ratio = if in_flight && let Some(elapsed) = elapsed {
        // 120s = typical LLM phase on iGPU; asymptotes to 0.9 * phase_share
        let phase_progress = (elapsed.as_secs_f32() / 120.0).min(0.9);
        (base_ratio + phase_share * phase_progress).min(1.0)
    } else {
        base_ratio
    };
    ui.add(egui::ProgressBar::new(ratio).show_percentage());

    if in_flight && let Some(elapsed) = elapsed {
        let sec = elapsed.as_secs();
        let phase_label = progress.current.map(|p| p.label()).unwrap_or("");
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new());
            ui.monospace(format!(
                "{phase_label} 実行中... 経過 {:02}:{:02}",
                sec / 60,
                sec % 60,
            ));
        });
        // Keep the timer ticking without user input
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(500));
    }

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

/// 4-color multi-filament export panel (GAP-B, Stage 4 T4.5 UI 完結)
///
/// Bambu Lab AMS / Prusa MMU 対応の 4 色分割エクスポート
/// LOL DSL + 正面 image → SDF → mesh → k-means 色量子化 → palette 色ごと
/// 3MF ファイル (`{uuid}_color{N}.3mf`) を生成する
///
/// UI:
/// - n_colors slider (2-4)
/// - projection axis dropdown (6 方向)
/// - 正面画像ピック button (rfd file dialog、PNG/JPG)
/// - Export button
/// - 生成された 3MF path 一覧 + Finder open
fn show_color4_export(
    ui: &mut Ui,
    state: &AppState,
    ui_state: &mut PromptUiState,
    lol_source: &str,
) {
    ui.collapsing("4色 export (Bambu AMS 対応)", |ui| {
        ui.label("正面画像を palette 化して色ごとに 3MF 分割 (Bambu Lab AMS / Prusa MMU)");

        // n_colors slider (2-4)
        ui.horizontal(|ui| {
            ui.label("色数:");
            ui.add(egui::Slider::new(&mut ui_state.color4.n_colors, 2..=4));
        });

        // Projection axis dropdown
        ui.horizontal(|ui| {
            ui.label("投影軸:");
            egui::ComboBox::from_id_salt("color4_projection")
                .selected_text(projection_label(ui_state.color4.projection))
                .show_ui(ui, |ui| {
                    use alice_bamboo::color4::ProjectionAxis;
                    for axis in [
                        ProjectionAxis::PositiveZ,
                        ProjectionAxis::NegativeZ,
                        ProjectionAxis::PositiveY,
                        ProjectionAxis::NegativeY,
                        ProjectionAxis::PositiveX,
                        ProjectionAxis::NegativeX,
                    ] {
                        ui.selectable_value(
                            &mut ui_state.color4.projection,
                            axis,
                            projection_label(axis),
                        );
                    }
                });
        });

        // Image picker
        ui.horizontal(|ui| {
            if ui.button("正面画像を選択...").clicked() {
                let picked = rfd::FileDialog::new()
                    .add_filter("PNG / JPEG", &["png", "jpg", "jpeg"])
                    .pick_file();
                if let Some(p) = picked {
                    ui_state.color4.image_path = Some(p);
                    ui_state.color4.last_error = None;
                }
            }
            if let Some(p) = &ui_state.color4.image_path {
                ui.monospace(p.file_name().and_then(|n| n.to_str()).unwrap_or("(image)"));
            } else {
                ui.colored_label(egui::Color32::GRAY, "(未選択)");
            }
        });

        // Export button
        let can_export = ui_state.color4.image_path.is_some();
        if ui
            .add_enabled(can_export, egui::Button::new("4色 3MF を生成"))
            .clicked()
            && let Some(image_path) = ui_state.color4.image_path.clone()
        {
            run_color4_export(state, ui_state, lol_source, &image_path);
        }

        if let Some(err) = &ui_state.color4.last_error {
            ui.add_space(4.0);
            ui.colored_label(egui::Color32::RED, format!("4色エクスポート失敗: {err}"));
        }
        if !ui_state.color4.last_generated.is_empty() {
            ui.add_space(4.0);
            ui.label(format!(
                "生成完了: {} 色 / {} ファイル",
                ui_state.color4.last_generated.len(),
                ui_state.color4.last_generated.len(),
            ));
            for p in &ui_state.color4.last_generated {
                ui.monospace(p.display().to_string());
            }
            if let Some(first) = ui_state.color4.last_generated.first()
                && let Some(parent) = first.parent()
                && ui.button("フォルダを開く").clicked()
            {
                let _ = open::that(parent);
            }
        }
    });
}

/// 4-color export executor Loads the picked image, builds a
/// `Color4Config`, then delegates to `pipeline::export_mesh_color4` The
/// resulting palette-split 3MF paths (`{uuid}_color{N}.3mf`) are stored
/// in `ui_state.color4.last_generated` so the UI can show them
fn run_color4_export(
    state: &AppState,
    ui_state: &mut PromptUiState,
    lol_source: &str,
    image_path: &std::path::Path,
) {
    ui_state.color4.last_error = None;
    ui_state.color4.last_generated.clear();

    let image = match image::open(image_path) {
        Ok(img) => img.to_rgb8(),
        Err(e) => {
            ui_state.color4.last_error = Some(format!("画像読み込み失敗: {e}"));
            return;
        }
    };

    let output_dir = state.data_dir.join("exports").join("color4");
    let _ = std::fs::create_dir_all(&output_dir);

    let cfg = alice_bamboo::color4::Color4Config {
        n_colors: ui_state.color4.n_colors,
        projection: ui_state.color4.projection,
        ..Default::default()
    };

    match pipeline::export_mesh_color4(lol_source, &output_dir, &image, Quality::High, cfg) {
        Ok(paths) => {
            tracing::info!(
                count = paths.len(),
                dir = %output_dir.display(),
                "4-color export succeeded"
            );
            ui_state.color4.last_generated = paths;
        }
        Err(e) => {
            tracing::error!(error = %e, "4-color export failed");
            ui_state.color4.last_error = Some(e.to_string());
        }
    }
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
        // Stage 5: effective tier from DB profile (LicenseIssuer flow will
        // upgrade `state.tier` on activation) drives both the alice:tier
        // metadata in the exported 3MF and the LoRA share flag downstream
        tier: state.tier,
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

/// Skip the LLM entirely and drive the mesh pipeline directly from a
/// hand-crafted LOL DSL string This is the fast path (~1 sec) used by
/// template buttons in the prompt panel LLM phase is emitted as
/// zero-duration so the UI phase grid still walks through all 5 stages
fn start_generation_from_lol(state: &mut AppState, lol_source: String, template_name: &str) {
    let gen_id = uuid::Uuid::now_v7().to_string();
    let is_public = state.tier.limits().force_public;
    let prompt_placeholder = format!("[template] {template_name}");

    let _ = state.db.insert_generation(&GenerationRecord {
        id: &gen_id,
        profile_id: &state.profile_id,
        prompt: &prompt_placeholder,
        lol_source: Some(&lol_source),
        sdf_data: None,
        quality: "preview",
        status: "pending",
        is_public,
        manifest_json: None,
    });
    let _ = state
        .db
        .increment_daily_usage(&state.profile_id, &state.today());

    state.generation_status = GenerationStatus::Generating;
    state.phase_progress.reset();
    state.phase_progress.generation_start = Some(Instant::now());

    let tx = state.result_tx.clone();
    let id = gen_id;
    let output_dir = state.data_dir.join("exports");
    let can_download = state.tier.limits().can_download;
    let lol_clone = lol_source.clone();

    state.runtime.spawn(async move {
        // LLM phase = instant so the phase grid still ticks through in the
        // same order as a real generation (Llm → Parse → Mesh → Safety →
        // Export) template mesh is entirely local so no network / model
        // load is involved
        let _ = tx.send(GenerationMessage::PhaseStart(GenerationPhase::Llm));
        let _ = tx.send(GenerationMessage::PhaseDone(
            GenerationPhase::Llm,
            Duration::ZERO,
        ));
        let _ = tx.send(GenerationMessage::PhaseStart(GenerationPhase::Parse));
        let _ = tx.send(GenerationMessage::PhaseDone(
            GenerationPhase::Parse,
            Duration::ZERO,
        ));

        let mut pipeline_error: Option<String> = None;
        let mesh_stats = if can_download {
            let _ = std::fs::create_dir_all(&output_dir);
            let _ = tx.send(GenerationMessage::PhaseStart(GenerationPhase::Mesh));
            let mesh_start = Instant::now();
            let stats_result = pipeline::export_mesh(
                &lol_clone,
                &output_dir,
                ExportFormat::ThreeMf,
                Quality::Preview,
            );
            let _ = tx.send(GenerationMessage::PhaseDone(
                GenerationPhase::Mesh,
                mesh_start.elapsed(),
            ));

            let _ = tx.send(GenerationMessage::PhaseStart(GenerationPhase::Safety));
            let _ = tx.send(GenerationMessage::PhaseDone(
                GenerationPhase::Safety,
                Duration::ZERO,
            ));

            let _ = tx.send(GenerationMessage::PhaseStart(GenerationPhase::Export));
            let out = match stats_result {
                Ok(stats) => Some(stats),
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        lol_preview = %lol_clone.chars().take(120).collect::<String>(),
                        "template mesh export failed"
                    );
                    pipeline_error = Some(format!("テンプレート生成失敗: {e}"));
                    None
                }
            };
            let _ = tx.send(GenerationMessage::PhaseDone(
                GenerationPhase::Export,
                Duration::ZERO,
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

        if let Some(err) = pipeline_error {
            let _ = tx.send(GenerationMessage::Failure { id, error: err });
        } else {
            let _ = tx.send(GenerationMessage::Success {
                id,
                lol_source: lol_clone,
                mesh_stats: mesh_stats.map(Box::new),
                retry_count: 0,
                share_dry_run: None,
            });
        }
    });
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
        manifest_json: None,
    });
    let _ = state
        .db
        .increment_daily_usage(&state.profile_id, &state.today());

    state.generation_status = GenerationStatus::Generating;
    state.phase_progress.reset();
    // Start the wall-clock timer so the UI can show progress during the
    // LLM phase (which is 99% of total wall time on iGPU)
    state.phase_progress.generation_start = Some(Instant::now());

    let config = state.llm_config.clone();
    // Stage 3-C.6: dispatch through the user-selected backend When the
    // Settings UI toggles to Embedded, `active_backend` returns the
    // loaded `EmbeddedBackend` (Ready) or falls back to Sidecar (during
    // load / on failure) so first-generation requests never dead-lock
    let backend = state.active_backend();
    // Stage 3-C.14: attach the LOL DSL GBNF grammar when the user has
    // opted in (default) so both Sidecar and Embedded paths return
    // syntactically valid LOL DSL
    let mut inference_params = text_to_print_llm::backend_kind::InferenceParams::from(&config);
    if state.enforce_lol_grammar {
        inference_params.grammar = Some(text_to_print_llm::grammar_lol::LOL_GBNF.to_string());
    }
    let tx = state.result_tx.clone();
    let id = gen_id;
    let output_dir = state.data_dir.join("exports");
    let can_download = state.tier.limits().can_download;
    // GAP-12 / Stage 5: capture the tier-effective opt-in flag + both share
    // directories so the async task can (a) dry-run dump for local audit and
    // (b) enqueue a real upload for the Cloudflare Worker sweep Paid tiers
    // suppress both regardless of the raw checkbox state
    let share_enabled = state.share_effective_enabled();
    let share_dry_run_dir = state.share_dry_run_dir();
    let share_queue_dir = state.share_queue_dir();
    let tier_slug = tier_slug(state.tier);
    let model_id = state.llm_config.model_choice.model_id().to_string();
    let prompt_lang = _lang.as_bcp47().to_string();

    state.runtime.spawn(async move {
        let _ = tx.send(GenerationMessage::PhaseStart(GenerationPhase::Llm));
        let llm_start = Instant::now();
        // GAP-11: replace `backend::generate` with `generate_with_retry` so the
        // Stage 8 fix-prompt loop actually engages when the LOL DSL raises
        // safety violations. Each retry re-sends `PhaseStart(Llm)` so the UI
        // progress indicator visualises the retries instead of pretending the
        // very first attempt succeeded.
        let tx_retry = tx.clone();
        // v0.1.0-beta.1 (2026-08-07): max_retries 3 → 1 に削減
        // 1 回の LLM inference が iGPU で 2-3 分かかるため、3 retry では
        // worst case 8-12 分待たされる 1 retry (2 attempt) までなら
        // 実用範囲内 (~5 分) empty response でも即 abort する新 gate も入れた
        // (crates/llm/src/backend.rs::generate_with_retry コメント参照)
        //
        // 2026-08-07 追加: 1 → 2 に拡張 (3 attempt 総計 ~5-7 分)
        // LOL 単一 expression 縛りを LLM が破る multi-statement 誤りが
        // 1 retry では吸収できないため試行機会を +1
        let result = backend::generate_with_retry(
            &backend,
            &inference_params,
            prompt::SYSTEM_PROMPT,
            &prompt_text,
            2,
            |response| {
                let _ = tx_retry.send(GenerationMessage::PhaseStart(GenerationPhase::Llm));
                let lol = pipeline::extract_lol(response)
                    .unwrap_or_else(|| pipeline::balance_parens(response));
                pipeline::safety_check_lol(&lol)
            },
        )
        .await
        .map(|r| (r.content, r.retry_count));
        let llm_elapsed = llm_start.elapsed();
        let _ = tx.send(GenerationMessage::PhaseDone(
            GenerationPhase::Llm,
            llm_elapsed,
        ));

        match result {
            Ok((response, retry_count)) => {
                // v0.1.0-beta.1: LLM 出力を diag log 化 (extract_lol が
                // 失敗した時に何を出力していたか特定するため、head 500 char
                // だけ tracing に流す 個人情報は含まないが、prompt から
                // 逆算可能なので shared LoRA training 対象からは除外)
                let preview: String = response.chars().take(500).collect();
                tracing::info!(
                    total_len = response.len(),
                    preview = %preview,
                    "LLM raw response (first 500 chars)"
                );
                let _ = tx.send(GenerationMessage::PhaseStart(GenerationPhase::Parse));
                let parse_start = Instant::now();
                let lol = pipeline::extract_lol(&response)
                    .unwrap_or_else(|| pipeline::balance_parens(&response));
                tracing::info!(
                    extracted_len = lol.len(),
                    extracted_preview = %lol.chars().take(200).collect::<String>(),
                    "LOL extracted from LLM response"
                );
                let _ = tx.send(GenerationMessage::PhaseDone(
                    GenerationPhase::Parse,
                    parse_start.elapsed(),
                ));

                // v0.1.0-beta.1 fix: `stats_result.ok()` は export Err を silent
                // 破棄して UI に「生成完了」を偽装していた 実際は LOL parse
                // fail 等で 3MF 未生成なのに Success が飛ぶ 修正: pipeline_error
                // に真の error を捕捉して後段で Failure 分岐する
                let mut pipeline_error: Option<String> = None;
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
                    let out = match stats_result {
                        Ok(stats) => Some(stats),
                        Err(e) => {
                            tracing::error!(error = %e, "mesh export failed");
                            pipeline_error = Some(format!("mesh export failed: {e}"));
                            None
                        }
                    };
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

                // GAP-12 / Stage 5: gate the share payload on the
                // tier-effective opt-in flag Free-tier + opt-in → dry-run
                // dump (local audit) + enqueue for the Cloudflare Worker
                // sweep Paid tiers short-circuit the branch entirely
                let share_dry_run = if share_enabled
                    && let Some(stats) = mesh_stats.as_ref()
                    && let Ok(mesh_bytes) = std::fs::read(&stats.path)
                {
                    let payload = text_to_print_network::share::SharePayload::from_inputs(
                        text_to_print_network::share::ShareInputs {
                            uuid: &id,
                            schema_version: "1",
                            prompt: &prompt_text,
                            prompt_lang: &prompt_lang,
                            llm_model: &model_id,
                            lol_source: &lol,
                            lol_sha256: &text_to_print_core::manifest::sha256_hex(lol.as_bytes()),
                            mesh_sha256: &text_to_print_core::manifest::sha256_hex(&mesh_bytes),
                            tier: tier_slug,
                            success: true,
                            retry_count,
                            time_to_file_ms: 0,
                            safety_violations: stats
                                .safety_summary
                                .as_ref()
                                .map(|s| s.messages.clone())
                                .unwrap_or_default(),
                            export_format: "3mf",
                            user_kept: true,
                            user_edited: false,
                        },
                    );
                    // Enqueue for the real upload sweep The dry-run dump
                    // path is kept for local inspection but the queued file
                    // is what actually gets delivered on the next
                    // `retry_queued_uploads` cycle
                    if let Err(e) =
                        text_to_print_network::share::enqueue(&payload, &share_queue_dir)
                    {
                        tracing::warn!(error = %e, "share enqueue failed");
                    }
                    match text_to_print_network::share::dump_dry_run(&payload, &share_dry_run_dir) {
                        Ok(p) => Some(p),
                        Err(e) => {
                            tracing::warn!(error = %e, "share dry-run dump failed");
                            None
                        }
                    }
                } else {
                    None
                };

                if let Some(err) = pipeline_error {
                    let _ = tx.send(GenerationMessage::Failure { id, error: err });
                } else {
                    let _ = tx.send(GenerationMessage::Success {
                        id,
                        lol_source: lol,
                        mesh_stats: mesh_stats.map(Box::new),
                        retry_count,
                        share_dry_run,
                    });
                }
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
                retry_count,
                share_dry_run,
            } => {
                let _ = state
                    .db
                    .update_generation_status(&id, "complete", Some(&lol_source), None);
                state.current_lol = Some(lol_source.clone());
                state.phase_progress.retry_count = retry_count;
                state.pending_share_dry_run = share_dry_run;

                if state.tier.limits().force_public {
                    state.pending_publish =
                        Some((id.clone(), lol_source.clone(), state.prompt_input.clone()));
                }

                // Publish the just-generated mesh to the preview viewer The
                // mesh already lives inside `mesh_stats.preview_mesh` (an
                // `Arc<Mesh>`), so this is a cheap ref-count bump
                if let Some(stats) = mesh_stats.as_ref()
                    && let Some(mesh) = stats.preview_mesh.as_ref()
                {
                    state.viewer_mesh = Some(mesh.clone());
                    state.mesh_version = state.mesh_version.wrapping_add(1);
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

/// テンプレート = ALICE-Bamboo/models 実プリント合格 baseline
///
/// Phase T1.1 (2026-08-08) で ALICE-* 由来なしの自作 LOL DSL を全削除、
/// `alice_lol::stdlib::pattern::registry::ALL` (canonical 13 pattern) のうち
/// runtime_parser Phase 5.1 高階 primitive で表現可能な 9 items を採用
///
/// 全 template は `~/ALICE-Bamboo/models/` に対応する `bamboo_canonical` を持ち、
/// `printability_score` (Bamboo simulation 実測) + `certified_by` (Both / UserFieldTest)
/// で認証済 未対応 4 items (shelf_divider / wall_hook / gridfinity_bin / drawer_organizer)
/// は ALICE-LOL runtime_parser に高階 primitive 追加後に取り込む
///
/// 詳細: memory `project_text_to_print_templates_alice_source.md` 参照
///
/// タプル形式: (button label, LOL DSL string)
const TEMPLATE_CATEGORIES: &[(&str, &[(&str, &str)])] = &[
    (
        "実績品 Both 認証 (Sim 88 + UserFieldTest、ALICE-Bamboo/models 由来)",
        &[
            // shopping_cart_coin_100yen: Φ22.8 × 1.7mm、models/accessories/shopping-cart-coin
            ("コイン (100円)", "shopping_cart_coin(22.8, 1.7)"),
            // skadis_panel_300x300: 300×300×5mm + peg 穴 98 個、models/wall-organizer/skadis-300x300
            ("SKADIS パネル 300×300", "skadis_panel(300, 5, 6)"),
            // skadis_hook_s: S 字曲げ、models/wall-organizer/skadis-hook-s
            ("SKADIS フック S", "skadis_hook_s()"),
            // skadis_clip: 単 peg 細物ホルダー、models/wall-organizer/skadis-clip
            ("SKADIS クリップ", "skadis_clip()"),
            // skadis_elastic_cord: 伸縮バンド固定、models/wall-organizer/skadis-elastic-cord
            ("SKADIS ゴムバンド", "skadis_elastic_cord()"),
        ],
    ),
    (
        "実績品 UserFieldTest 認証 (実荷重テスト合格、ALICE-Bamboo/models 由来)",
        &[
            // skadis_hook_j: J 字曲げ、models/wall-organizer/skadis-hook-j
            ("SKADIS フック J", "skadis_hook_j()"),
            // skadis_hook_l: 直角曲げ、models/wall-organizer/skadis-hook-l
            ("SKADIS フック L", "skadis_hook_l()"),
            // skadis_container: 2 peg gusset ribs 補強、models/wall-organizer/skadis-container
            ("SKADIS コンテナ", "skadis_container()"),
            // skadis_shelf: 2 peg rib 補強棚板、PETG 30lbs 実荷重合格、models/wall-organizer/skadis-shelf
            ("SKADIS シェルフ", "skadis_shelf()"),
            // shelf_divider: 560×250×120mm U 字仕切り、hex cutout 底板 + 2 側板
            // models/shelf/divider-560x250x120 実プリント合格 spec
            ("棚仕切り 560×250×120", "shelf_divider()"),
        ],
    ),
];

/// テンプレート = 直接 LOL DSL 生成 (LLM bypass、~1 秒)
///
/// 従来の Japanese prompt + LLM 経路 (2-8 min + 非決定) から刷新
/// ボタンクリック → alice-bamboo pipeline に LOL を直接流し込み、mesh + 3MF
/// を即座に生成 → viewer 表示 生成中は disabled
fn show_prompt_templates(ui: &mut egui::Ui, state: &mut AppState, is_generating: bool) {
    ui.collapsing(
        "テンプレート (クリックで即生成、LLM 経由しない)",
        |ui| {
            ui.add_enabled_ui(!is_generating, |ui| {
                for (cat_name, items) in TEMPLATE_CATEGORIES {
                    ui.label(egui::RichText::new(*cat_name).strong());
                    ui.horizontal_wrapped(|ui| {
                        for (label, lol_dsl) in *items {
                            if ui.button(*label).clicked() {
                                state.prompt_input = format!("[template] {label}");
                                state.prompt_focused_once = false;
                                start_generation_from_lol(state, (*lol_dsl).to_string(), label);
                            }
                        }
                    });
                    ui.add_space(2.0);
                }
            });
        },
    );
}

/// カスタマイザー = パラメータ入力可能な template (LLM bypass、~1 秒)
///
/// 経路 A (固定 preset button) と経路 B (LLM 自然言語) の中間 slider で
/// param を指定 → 「作成」ボタンで LOL DSL 動的組立て → 生成
/// 現行対応: Gridfinity bin + organizer-gridfinity-desk PART 2 の 4 archetype
/// (sticky_note_holder / business_card_holder / pen_cup / phone_stand)
/// 追加 archetype は同 collapsing 内に別 section で並べる
fn show_prompt_customizer(ui: &mut egui::Ui, state: &mut AppState, is_generating: bool) {
    ui.collapsing(
        "カスタマイザー (サイズ指定して生成、LLM 経由しない)",
        |ui| {
            ui.add_enabled_ui(!is_generating, |ui| {
                show_gridfinity_customizer(ui, state);
                ui.separator();
                show_sticky_note_customizer(ui, state);
                ui.separator();
                show_business_card_customizer(ui, state);
                ui.separator();
                show_pen_cup_customizer(ui, state);
                ui.separator();
                show_phone_stand_customizer(ui, state);
            });
        },
    );
}

/// Gridfinity bin customizer (basic 3 param + advanced 5 param collapsible)
///
/// 42mm grid × 7mm height unit で任意サイズを生成
/// 例: 2×2 × 6U = 84×84×46mm (最典型 default)
/// 詳細設定で dividers (内部仕切り) + 壁厚 + 底厚 も指定可
fn show_gridfinity_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("📦 Gridfinity bin (42mm grid × 7mm 高さ)").strong());

    let g = &mut state.customizer_state.gridfinity;
    ui.horizontal(|ui| {
        ui.label("Units X:");
        ui.add(egui::Slider::new(&mut g.units_x, 1..=6).text("(1-6)"));
    });
    ui.horizontal(|ui| {
        ui.label("Units Y:");
        ui.add(egui::Slider::new(&mut g.units_y, 1..=6).text("(1-6)"));
    });
    ui.horizontal(|ui| {
        ui.label("Height U:");
        ui.add(egui::Slider::new(&mut g.height_u, 2..=10).text("(2U=18mm ~ 10U=74mm)"));
    });

    // Advanced (dividers + wall/floor thickness) は default で閉じている
    ui.collapsing("詳細設定 (dividers + 壁厚)", |ui| {
        ui.checkbox(&mut g.use_dividers, "内部仕切り (dividers) を有効化");
        ui.add_enabled_ui(g.use_dividers, |ui| {
            ui.horizontal(|ui| {
                ui.label("Dividers X:");
                ui.add(egui::Slider::new(&mut g.dividers_x, 2..=6).text("(cells)"));
            });
            ui.horizontal(|ui| {
                ui.label("Dividers Y:");
                ui.add(egui::Slider::new(&mut g.dividers_y, 2..=6).text("(cells)"));
            });
        });
        ui.horizontal(|ui| {
            ui.label("壁厚 (mm):");
            ui.add(egui::Slider::new(&mut g.wall_thickness, 0.8..=3.0).step_by(0.1));
        });
        ui.horizontal(|ui| {
            ui.label("底厚 (mm):");
            ui.add(egui::Slider::new(&mut g.floor_thickness, 1.0..=4.0).step_by(0.1));
        });
    });

    #[allow(clippy::cast_precision_loss)]
    let ext_x_mm = g.units_x as f32 * 42.0;
    #[allow(clippy::cast_precision_loss)]
    let ext_y_mm = g.units_y as f32 * 42.0;
    #[allow(clippy::cast_precision_loss)]
    let ext_h_mm = g.height_u as f32 * 7.0 + 4.75;
    ui.label(format!(
        "外形寸法: {ext_x_mm:.1} × {ext_y_mm:.1} × {ext_h_mm:.1}mm"
    ));

    let g_copy = *g;
    let label = if g_copy.use_dividers {
        format!(
            "Gridfinity {}×{} × {}U ({}×{} dividers)",
            g_copy.units_x, g_copy.units_y, g_copy.height_u, g_copy.dividers_x, g_copy.dividers_y
        )
    } else {
        format!(
            "Gridfinity {}×{} × {}U",
            g_copy.units_x, g_copy.units_y, g_copy.height_u
        )
    };
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input = format!("[customizer] {label}");
        state.prompt_focused_once = false;
        start_generation_from_lol(state, g_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// 付箋ホルダー customizer (`pad_w × pad_d × height`)
///
/// Post-it 3×3 inch = 76×76mm、大型 3×5 inch = 76×127mm 等
fn show_sticky_note_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🗒 付箋ホルダー (Post-it 3×3 / 3×5 inch 対応)").strong());

    let s = &mut state.customizer_state.sticky_note;
    ui.horizontal(|ui| {
        ui.label("pad 幅 (mm):");
        ui.add(egui::Slider::new(&mut s.pad_width, 50.0..=150.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label("pad 深さ (mm):");
        ui.add(egui::Slider::new(&mut s.pad_depth, 50.0..=150.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label("高さ (mm):");
        ui.add(egui::Slider::new(&mut s.height, 15.0..=60.0).step_by(1.0));
    });

    let s_copy = *s;
    let label = format!(
        "付箋ホルダー {}×{}×{}mm",
        s_copy.pad_width, s_copy.pad_depth, s_copy.height
    );
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input = format!("[customizer] {label}");
        state.prompt_focused_once = false;
        start_generation_from_lol(state, s_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// 名刺ホルダー customizer (`card_w × card_h × slot_thickness`)
///
/// JP meishi 91×55 / US 89×51 / EU 85.6×54、収納枚数 = slot_thickness / 0.5mm 目安
fn show_business_card_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("💳 名刺ホルダー (JP 91×55 / US 89×51 / EU 85.6×54)").strong());

    let b = &mut state.customizer_state.business_card;
    ui.horizontal(|ui| {
        ui.label("card 幅 (mm):");
        ui.add(egui::Slider::new(&mut b.card_width, 80.0..=100.0).step_by(0.1));
    });
    ui.horizontal(|ui| {
        ui.label("card 高さ (mm):");
        ui.add(egui::Slider::new(&mut b.card_height, 45.0..=65.0).step_by(0.1));
    });
    ui.horizontal(|ui| {
        ui.label("slot 厚 (mm):");
        ui.add(egui::Slider::new(&mut b.slot_thickness, 10.0..=40.0).step_by(1.0));
    });

    // 収納枚数目安 (card 1 枚 ~0.5mm、20% margin)
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let capacity = (b.slot_thickness / 0.5 * 0.8) as u32;
    ui.label(format!("収納枚数目安: 約 {capacity} 枚"));

    let b_copy = *b;
    let label = format!(
        "名刺ホルダー {}×{}mm ({}枚)",
        b_copy.card_width, b_copy.card_height, capacity
    );
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input = format!("[customizer] {label}");
        state.prompt_focused_once = false;
        start_generation_from_lol(state, b_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// ペン立て customizer (`inner_dia × height`)
///
/// standard 70-85mm 内径 × 90-120mm 高、pen 12mm / pencil 8mm / marker 16mm / highlighter 24mm 想定
fn show_pen_cup_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("✏ ペン立て (single-compartment 円筒)").strong());

    let p = &mut state.customizer_state.pen_cup;
    ui.horizontal(|ui| {
        ui.label("内径 (mm):");
        ui.add(egui::Slider::new(&mut p.inner_diameter, 40.0..=120.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label("高さ (mm):");
        ui.add(egui::Slider::new(&mut p.height, 50.0..=150.0).step_by(1.0));
    });

    let p_copy = *p;
    let outer_dia = p_copy.inner_diameter + 4.0;
    let label = format!("ペン立て Ø{}×{}mm", p_copy.inner_diameter, p_copy.height);
    ui.label(format!("外形 Ø{outer_dia:.1}mm (壁厚 2mm)"));
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input = format!("[customizer] {label}");
        state.prompt_focused_once = false;
        start_generation_from_lol(state, p_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// スマホ / タブレット スタンド customizer (`slot_w × back_h × cable_dia`)
///
/// phone: slot 10-15mm / back 80-120mm、tablet: slot 12-18mm / back 150-190mm
/// cable_dia = 0 で cable 穴なし
fn show_phone_stand_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("📱 スマホ / タブレット スタンド (L 字 + 上部 slot)").strong());

    let ps = &mut state.customizer_state.phone_stand;
    ui.horizontal(|ui| {
        ui.label("slot 幅 (mm):");
        ui.add(egui::Slider::new(&mut ps.slot_width, 8.0..=20.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label("back 高さ (mm):");
        ui.add(egui::Slider::new(&mut ps.back_height, 60.0..=200.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label("cable 穴径 (mm):");
        ui.add(egui::Slider::new(&mut ps.cable_hole_dia, 0.0..=30.0).step_by(1.0));
    });

    let ps_copy = *ps;
    let hole_note = if ps_copy.cable_hole_dia > 0.0 {
        format!("cable Ø{}mm", ps_copy.cable_hole_dia)
    } else {
        "穴なし".to_string()
    };
    let label = format!(
        "スタンド slot {}mm × back {}mm ({hole_note})",
        ps_copy.slot_width, ps_copy.back_height
    );
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input = format!("[customizer] {label}");
        state.prompt_focused_once = false;
        start_generation_from_lol(state, ps_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}
