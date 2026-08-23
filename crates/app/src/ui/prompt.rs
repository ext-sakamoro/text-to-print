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

    // 12 archetype customizer 展開時に画面外に溢れるため縦 scroll でラップ
    // (2026-08-20 追加、user 実機 report で発覚した scroll bar 不能 bug 修正)
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            show_inner(ui, state, ui_state, lang);
        });
}

fn show_inner(ui: &mut Ui, state: &mut AppState, ui_state: &mut PromptUiState, lang: Lang) {
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

    let is_generating = matches!(state.generation_status, GenerationStatus::Generating);
    let sidecar_running = state.sidecar_status.borrow().is_running();
    let can_gen = state.can_generate() && !state.prompt_input.trim().is_empty() && sidecar_running;

    // R3 tab reorder (2026-08-23): templates + customizer are the
    // primary recommended path Both bypass the LLM entirely and emit
    // LOL DSL directly, giving deterministic + fast + free generation
    // The LLM natural-language input is now demoted to an "Experimental"
    // collapsing panel below with an explicit capability disclaimer
    ui.add_space(6.0);
    ui.label(egui::RichText::new(crate::i18n::T::templates_section(lang)).strong());
    show_prompt_templates(ui, state, is_generating);

    ui.add_space(6.0);
    ui.label(egui::RichText::new(crate::i18n::T::customizer_section(lang)).strong());
    show_prompt_customizer(ui, state, is_generating);

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);

    // Experimental LLM path — collapsed by default so first-time users
    // land on the working templates/customizer path instead of the
    // 10-minute LLM wait that ends in a plain cube (see
    // [[feedback_llm_3b_complex_shape_hallucination]])
    egui::CollapsingHeader::new(
        egui::RichText::new(crate::i18n::T::experimental_llm_header(lang))
            .strong()
            .color(egui::Color32::from_rgb(200, 140, 60)),
    )
    .default_open(false)
    .id_salt("experimental_llm_section")
    .show(ui, |ui| {
        ui.label(
            egui::RichText::new(crate::i18n::T::experimental_llm_hint(lang))
                .small()
                .color(egui::Color32::from_rgb(200, 140, 60)),
        );
        ui.add_space(4.0);
        ui.label("3D モデルの説明を入力 (Enter で生成 / Shift+Enter で改行):");

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

        ui.add_space(6.0);
        if ui
            .add_enabled(!is_generating && can_gen, egui::Button::new("生成 (LLM)"))
            .clicked()
        {
            start_generation(state, lang);
        }

        if !state.can_generate() && !is_generating {
            let warn_color = ui.style().visuals.warn_fg_color;
            ui.colored_label(warn_color, "本日の生成上限に達しました");
        }
    });

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
            GenerationMessage::PresetsUpdated(new_snapshot) => {
                // Sprint X.1: background fetch 完了、UI 反映
                tracing::info!(
                    version = %new_snapshot.version,
                    category_count = new_snapshot.categories.len(),
                    "presets snapshot updated from cloud"
                );
                state.presets = *new_snapshot;
                ui.ctx().request_repaint();
            }
        }
    }
}

/// テンプレート = 直接 LOL DSL 生成 (LLM bypass、~1 秒)
///
/// Sprint X.1 (2026-08-21) で TEMPLATE_CATEGORIES const を廃止、
/// `state.presets: PresetsSnapshot` (Cloudflare Worker 経由の Layer 1 sync) を
/// 直接読む dynamic 実装に refactor 起動時は bundled default (or local cache)、
/// background で Cloudflare fetch 完了時に UI 自動更新
///
/// 詳細: memory `project_text_to_print_archetype_library_architecture.md`
fn show_prompt_templates(ui: &mut egui::Ui, state: &mut AppState, is_generating: bool) {
    ui.collapsing(
        "テンプレート (クリックで即生成、LLM 経由しない)",
        |ui| {
            ui.add_enabled_ui(!is_generating, |ui| {
                // Snapshot を clone して borrow 期間を短縮 (start_generation_from_lol が
                // state を mutable borrow するため、iterator 中の借用と衝突しないよう分離)
                let snapshot = state.presets.clone();

                // preset source label (bundled / cache / cloud) を version と共に
                // 小さく表示、user が「今どの source を見ているか」認識できる
                let source_label = match snapshot.source {
                    crate::state::PresetsSource::Bundled => "内蔵",
                    crate::state::PresetsSource::Cache => "cache",
                    crate::state::PresetsSource::Cloud => "☁ Cloud",
                };
                ui.small(format!(
                    "presets: {source_label} / version {}",
                    snapshot.version
                ));

                for category in &snapshot.categories {
                    ui.label(egui::RichText::new(&category.name).strong());
                    ui.horizontal_wrapped(|ui| {
                        for preset in &category.presets {
                            if ui.button(&preset.label).clicked() {
                                state.prompt_input.clear();
                                state.prompt_focused_once = false;
                                start_generation_from_lol(
                                    state,
                                    preset.lol_dsl.clone(),
                                    &preset.label,
                                );
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
/// 現行対応 37 archetype: Gridfinity bin + organizer-gridfinity-desk PART 2 全部
/// (sticky_note_holder / business_card_holder / pen_cup / phone_stand /
///  headphone_holder / under_desk_mount / desk_shelf / monitor_riser) +
/// household 3 (coaster / tissue_box_cover / storage_box) +
/// hobby-diy 4 (cable_clip / led_channel / card_tray / token_well、Sprint 5) +
/// tools 3 (wrench_holder / socket_rail / hex_bit_holder、Sprint 6) +
/// electronics 3 (raspi_case / esp32_enclosure / battery_18650_holder、Sprint 7) +
/// bathroom-garage 3 (toothbrush_holder / drill_bit_holder / pliers_rack、Sprint 8) +
/// kitchen 3 (spice_rack / egg_tray / utensil_caddy、Sprint 9) +
/// printer 3 (filament_spool_holder / nozzle_holder / build_plate_rack、Sprint 10) +
/// drawer-wall 3 (cutlery_tray / pill_organizer / magnetic_strip、Sprint 11) +
/// mix 3 (hairdryer_holder / kcup_holder / hex_key_holder、Sprint 12)
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
                ui.separator();
                show_headphone_holder_customizer(ui, state);
                ui.separator();
                show_under_desk_mount_customizer(ui, state);
                ui.separator();
                show_desk_shelf_customizer(ui, state);
                ui.separator();
                show_monitor_riser_customizer(ui, state);
                ui.separator();
                show_coaster_customizer(ui, state);
                ui.separator();
                show_tissue_box_cover_customizer(ui, state);
                ui.separator();
                show_storage_box_customizer(ui, state);
                ui.separator();
                show_cable_clip_customizer(ui, state);
                ui.separator();
                show_led_channel_customizer(ui, state);
                ui.separator();
                show_card_tray_customizer(ui, state);
                ui.separator();
                show_token_well_customizer(ui, state);
                ui.separator();
                show_wrench_holder_customizer(ui, state);
                ui.separator();
                show_socket_rail_customizer(ui, state);
                ui.separator();
                show_hex_bit_holder_customizer(ui, state);
                ui.separator();
                show_raspi_case_customizer(ui, state);
                ui.separator();
                show_esp32_enclosure_customizer(ui, state);
                ui.separator();
                show_battery_18650_holder_customizer(ui, state);
                ui.separator();
                show_toothbrush_holder_customizer(ui, state);
                ui.separator();
                show_drill_bit_holder_customizer(ui, state);
                ui.separator();
                show_pliers_rack_customizer(ui, state);
                ui.separator();
                show_spice_rack_customizer(ui, state);
                ui.separator();
                show_egg_tray_customizer(ui, state);
                ui.separator();
                show_utensil_caddy_customizer(ui, state);
                ui.separator();
                show_filament_spool_holder_customizer(ui, state);
                ui.separator();
                show_nozzle_holder_customizer(ui, state);
                ui.separator();
                show_build_plate_rack_customizer(ui, state);
                ui.separator();
                show_cutlery_tray_customizer(ui, state);
                ui.separator();
                show_pill_organizer_customizer(ui, state);
                ui.separator();
                show_magnetic_strip_customizer(ui, state);
                ui.separator();
                show_hairdryer_holder_customizer(ui, state);
                ui.separator();
                show_kcup_holder_customizer(ui, state);
                ui.separator();
                show_hex_key_holder_customizer(ui, state);
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
        state.prompt_input.clear();
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
        state.prompt_input.clear();
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
        state.prompt_input.clear();
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
        state.prompt_input.clear();
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
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, ps_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// ヘッドホンホルダー customizer (`arm_length × headband_width × mount_width`)
///
/// wall_hook variant で headband 対応、M4 mount 穴付き
fn show_headphone_holder_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🎧 ヘッドホンホルダー (wall-mount + hook)").strong());

    let h = &mut state.customizer_state.headphone_holder;
    ui.horizontal(|ui| {
        ui.label("arm 長 (mm):");
        ui.add(egui::Slider::new(&mut h.arm_length, 60.0..=120.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label("headband 幅 (mm):");
        ui.add(egui::Slider::new(&mut h.headband_width, 30.0..=70.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label("mount 幅 (mm):");
        ui.add(egui::Slider::new(&mut h.mount_width, 60.0..=150.0).step_by(1.0));
    });

    let h_copy = *h;
    let label = format!(
        "ヘッドホンホルダー arm{}mm×hb{}mm×mount{}mm",
        h_copy.arm_length, h_copy.headband_width, h_copy.mount_width
    );
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, h_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// 机下 clamp mount customizer (`desk_thickness × clamp_width × screw_dia`)
///
/// C 字 clamp、screw=0 で穴なし (両面テープ想定)
fn show_under_desk_mount_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🔧 机下 clamp mount (C 字クランプ + 締付ネジ)").strong());

    let m = &mut state.customizer_state.under_desk_mount;
    ui.horizontal(|ui| {
        ui.label("desk 厚 (mm):");
        ui.add(egui::Slider::new(&mut m.desk_thickness, 15.0..=60.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label("clamp 幅 (mm):");
        ui.add(egui::Slider::new(&mut m.clamp_width, 20.0..=80.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label("screw 径 (mm):");
        ui.add(egui::Slider::new(&mut m.screw_hole_dia, 0.0..=8.0).step_by(0.5));
    });

    let m_copy = *m;
    let screw_note = if m_copy.screw_hole_dia > 0.0 {
        format!("M{:.0}", m_copy.screw_hole_dia)
    } else {
        "両面テープ".to_string()
    };
    let label = format!(
        "机下 mount desk{}mm × clamp{}mm ({screw_note})",
        m_copy.desk_thickness, m_copy.clamp_width
    );
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, m_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// 卓上シェルフ customizer (`shelf_width × shelf_depth × leg_height`)
///
/// 平板 + 左右 2 脚 shelf_divider 簡易版 (hex cutout なし)
fn show_desk_shelf_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🗄 卓上シェルフ (平板 + 左右 2 脚)").strong());

    let s = &mut state.customizer_state.desk_shelf;
    ui.horizontal(|ui| {
        ui.label("shelf 幅 (mm):");
        ui.add(egui::Slider::new(&mut s.shelf_width, 200.0..=500.0).step_by(10.0));
    });
    ui.horizontal(|ui| {
        ui.label("shelf 奥行 (mm):");
        ui.add(egui::Slider::new(&mut s.shelf_depth, 150.0..=300.0).step_by(10.0));
    });
    ui.horizontal(|ui| {
        ui.label("leg 高 (mm):");
        ui.add(egui::Slider::new(&mut s.leg_height, 60.0..=150.0).step_by(5.0));
    });

    let s_copy = *s;
    let label = format!(
        "シェルフ {}×{}mm × 脚{}mm",
        s_copy.shelf_width, s_copy.shelf_depth, s_copy.leg_height
    );
    ui.label("注: 幅 315mm 超えは Bambu H2D 単一プリント不可 (要分割)");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, s_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// モニターライザー customizer (`width × depth × height`)
///
/// 簡易版 = 単一プリント想定、Ø40mm cable hole 付き
fn show_monitor_riser_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🖥 モニターライザー (プラットフォーム + 2 脚 + cable)").strong());

    let r = &mut state.customizer_state.monitor_riser;
    ui.horizontal(|ui| {
        ui.label("幅 (mm):");
        ui.add(egui::Slider::new(&mut r.width, 200.0..=280.0).step_by(10.0));
    });
    ui.horizontal(|ui| {
        ui.label("奥行 (mm):");
        ui.add(egui::Slider::new(&mut r.depth, 150.0..=240.0).step_by(10.0));
    });
    ui.horizontal(|ui| {
        ui.label("高さ (mm):");
        ui.add(egui::Slider::new(&mut r.height, 60.0..=120.0).step_by(5.0));
    });

    let r_copy = *r;
    let label = format!(
        "モニターライザー {}×{}×{}mm",
        r_copy.width, r_copy.depth, r_copy.height
    );
    ui.label("cable 穴 Ø40mm 標準装備、単一プリント想定 (280mm 以下)");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, r_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// コースター customizer (`diameter × thickness`)
///
/// round bowl 状、rim 2.5mm 幅 × 1.5mm 高 で液滴 catch (household § 7)
fn show_coaster_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🥤 コースター (round + rim)").strong());

    let c = &mut state.customizer_state.coaster;
    ui.horizontal(|ui| {
        ui.label("直径 (mm):");
        ui.add(egui::Slider::new(&mut c.diameter, 80.0..=110.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label("全厚 (mm):");
        ui.add(egui::Slider::new(&mut c.thickness, 4.0..=8.0).step_by(0.5));
    });

    let c_copy = *c;
    let label = format!("コースター Ø{}×{}mm", c_copy.diameter, c_copy.thickness);
    ui.label("rim 2.5mm 幅 × 1.5mm 高 (液滴 catch)");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, c_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// ティッシュボックスカバー customizer (`internal_l × internal_w × internal_h`)
///
/// bottom open + top pull slot (80×30mm 標準)、内部寸法指定 (household § 1)
fn show_tissue_box_cover_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🧻 ティッシュボックスカバー (bottom open + top slot)").strong());

    let t = &mut state.customizer_state.tissue_box_cover;
    ui.horizontal(|ui| {
        ui.label("内部 長 (mm):");
        ui.add(egui::Slider::new(&mut t.internal_length, 100.0..=280.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label("内部 幅 (mm):");
        ui.add(egui::Slider::new(&mut t.internal_width, 100.0..=200.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label("内部 高 (mm):");
        ui.add(egui::Slider::new(&mut t.internal_height, 40.0..=140.0).step_by(1.0));
    });

    let t_copy = *t;
    let label = format!(
        "ティッシュカバー 内 {}×{}×{}mm",
        t_copy.internal_length, t_copy.internal_width, t_copy.internal_height
    );
    ui.label("プリセット目安: US rect (231×116×53) / Cube (114×114×127) / Square (114×114×100)");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, t_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// 収納 BOX customizer (`internal_l × internal_w × internal_h`)
///
/// top open 基本形、lid + hinge は future sprint (household § 3)
fn show_storage_box_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("📦 収納 BOX (top open、基本形)").strong());

    let s = &mut state.customizer_state.storage_box;
    ui.horizontal(|ui| {
        ui.label("内部 長 (mm):");
        ui.add(egui::Slider::new(&mut s.internal_length, 60.0..=250.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label("内部 幅 (mm):");
        ui.add(egui::Slider::new(&mut s.internal_width, 60.0..=200.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label("内部 高 (mm):");
        ui.add(egui::Slider::new(&mut s.internal_height, 30.0..=120.0).step_by(5.0));
    });

    let s_copy = *s;
    let label = format!(
        "収納 BOX 内 {}×{}×{}mm",
        s_copy.internal_length, s_copy.internal_width, s_copy.internal_height
    );
    ui.label("プリセット目安: Small (80×60×40) / Medium (150×100×60) / Large (200×150×80)");
    ui.label("注: lid + hinge は future sprint、現状は top open 基本形");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, s_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// ケーブルクリップ customizer (`cable_diameter × clip_length`、hobby-diy § 2)
///
/// Y-axis 沿い cable、+Z 開口 snap-fit (opening ratio 0.7 = 30% 狭い)
/// USB-A 3.5 / USB-C 4.5 / Ethernet 6 / HDMI 7 / Power 8-10 mm 想定
fn show_cable_clip_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🔌 ケーブルクリップ (snap-fit)").strong());

    let c = &mut state.customizer_state.cable_clip;
    ui.horizontal(|ui| {
        ui.label("ケーブル直径 (mm):");
        ui.add(egui::Slider::new(&mut c.cable_diameter, 3.0..=12.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label("クリップ長 (mm):");
        ui.add(egui::Slider::new(&mut c.clip_length, 15.0..=60.0).step_by(1.0));
    });

    let c_copy = *c;
    let label = format!(
        "ケーブルクリップ Ø{}×L{}mm",
        c_copy.cable_diameter, c_copy.clip_length
    );
    ui.label("プリセット目安: USB-C (Ø4.5/L22) / HDMI (Ø7/L28) / 電源 (Ø9/L36)");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, c_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// LED strip channel customizer (`strip_width × channel_length`、hobby-diy § 3)
///
/// Y-axis 沿い strip、+Z 開口 U 溝 (深さ 2.5mm 固定、壁厚 2.0mm)
/// SMD3528 8mm / WS2812B 10-12mm PCB 対応
fn show_led_channel_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("💡 LED strip channel (U 溝、上端開口)").strong());

    let l = &mut state.customizer_state.led_channel;
    ui.horizontal(|ui| {
        ui.label("strip PCB 幅 (mm):");
        ui.add(egui::Slider::new(&mut l.strip_width, 6.0..=20.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label("全長 (mm):");
        ui.add(egui::Slider::new(&mut l.channel_length, 50.0..=1000.0).step_by(10.0));
    });

    let l_copy = *l;
    let label = format!(
        "LED channel {}mm × {}mm",
        l_copy.strip_width, l_copy.channel_length
    );
    ui.label("プリセット目安: SMD3528 (8mm) / WS2812B 標準 (10mm) / 高密度 (12mm)");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, l_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// カードトレー customizer (`card_w × card_h × depth`、hobby-diy § 6)
///
/// top 開口 + front edge finger 半円 notch (r=9mm 固定)
/// Poker 63×88 / Mini Euro 44×68 / Standard Euro 59×92 / Tarot 70×120 対応
fn show_card_tray_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🎴 カードトレー (finger notch 付き)").strong());

    let t = &mut state.customizer_state.card_tray;
    ui.horizontal(|ui| {
        ui.label("カード幅 (mm):");
        ui.add(egui::Slider::new(&mut t.card_width, 30.0..=80.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label("カード高さ (mm):");
        ui.add(egui::Slider::new(&mut t.card_height, 50.0..=130.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label("tray 内深さ (mm):");
        ui.add(egui::Slider::new(&mut t.tray_depth, 10.0..=60.0).step_by(1.0));
    });

    let t_copy = *t;
    let label = format!(
        "カードトレー {}×{}×深{}mm",
        t_copy.card_width, t_copy.card_height, t_copy.tray_depth
    );
    ui.label(
        "プリセット目安: Poker (63×88) / Mini Euro (44×68) / Std Euro (59×92) / Tarot (70×120)",
    );
    ui.label("目安: 深 30mm ≈ 100-150 cards、finger notch r=9mm 固定");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, t_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// トークン井戸 customizer (`dia × depth × count`、hobby-diy § 6)
///
/// row 状に count 個の円筒 well、top 開口、印刷正立
/// shallow token 10-15 / dice/meeples 20-25 / miniatures 30-40 mm 深さ目安
fn show_token_well_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🎲 トークン井戸 (row 配置 count well)").strong());

    let w = &mut state.customizer_state.token_well;
    ui.horizontal(|ui| {
        ui.label("well 直径 (mm):");
        ui.add(egui::Slider::new(&mut w.well_diameter, 8.0..=40.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label("well 深さ (mm):");
        ui.add(egui::Slider::new(&mut w.well_depth, 5.0..=50.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label("well 個数:");
        ui.add(egui::Slider::new(&mut w.well_count, 1..=10).text("(1-10)"));
    });

    let w_copy = *w;
    let label = format!(
        "トークン井戸 Ø{}×深{}mm × {}",
        w_copy.well_diameter, w_copy.well_depth, w_copy.well_count
    );
    ui.label("プリセット目安: shallow token (10-15mm) / dice (20-25mm) / miniatures (30-40mm)");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, w_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// レンチホルダー customizer (`min_mm × max_mm × count`、tools § 1)
///
/// min-max mm を count 個 等間隔補間 (例: 8, 10, 12, 14, 16, 18)
/// Metric 標準 8-19 (6 slot) / 8-24 (8 slot) / SAE 1/4"-1" 相当は 6.35-25.4mm
fn show_wrench_holder_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🔧 レンチホルダー (row 状 slot、min-max 等間隔)").strong());

    let w = &mut state.customizer_state.wrench_holder;
    ui.horizontal(|ui| {
        ui.label("最小サイズ (mm):");
        ui.add(egui::Slider::new(&mut w.min_size_mm, 6.0..=22.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label("最大サイズ (mm):");
        ui.add(egui::Slider::new(&mut w.max_size_mm, 8.0..=32.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label("slot 個数:");
        ui.add(egui::Slider::new(&mut w.count, 3..=12).text("(3-12)"));
    });

    let w_copy = *w;
    let label = format!(
        "レンチホルダー {}-{}mm × {}",
        w_copy.min_size_mm, w_copy.max_size_mm, w_copy.count
    );
    ui.label("プリセット目安: Metric 8-19 (6 slot) / 8-24 (8 slot) / SAE 6-25 (1/4-1 inch)");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, w_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// ソケットレール customizer (`post_dia × post_height × count`、tools § 2)
///
/// base plate 上に post を row 配置 1/4"=6.0 / 3/8"=9.2 / 1/2"=12.4 / 3/4"=18.7
fn show_socket_rail_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🔩 ソケットレール (base + row post)").strong());

    let s = &mut state.customizer_state.socket_rail;
    ui.horizontal(|ui| {
        ui.label("post 直径 (mm):");
        ui.add(egui::Slider::new(&mut s.post_diameter, 5.0..=25.0).step_by(0.1));
    });
    ui.horizontal(|ui| {
        ui.label("post 高さ (mm):");
        ui.add(egui::Slider::new(&mut s.post_height, 12.0..=30.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label("post 個数:");
        ui.add(egui::Slider::new(&mut s.post_count, 3..=15).text("(3-15)"));
    });

    let s_copy = *s;
    let label = format!(
        "ソケットレール Ø{}×H{}mm × {}",
        s_copy.post_diameter, s_copy.post_height, s_copy.post_count
    );
    ui.label("Drive 目安: 1/4\"=6.0mm / 3/8\"=9.2mm / 1/2\"=12.4mm / 3/4\"=18.7mm");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, s_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// ヘックスビットホルダー customizer (`rows × cols × spacing`、tools § 3)
///
/// grid 状 hex hole、1/4" bit 想定 (across-flats 6.85mm × depth 14mm 固定)
fn show_hex_bit_holder_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🔷 ヘックスビットホルダー (1/4\" bit、grid)").strong());

    let h = &mut state.customizer_state.hex_bit_holder;
    ui.horizontal(|ui| {
        ui.label("行数:");
        ui.add(egui::Slider::new(&mut h.rows, 1..=10).text("(1-10)"));
    });
    ui.horizontal(|ui| {
        ui.label("列数:");
        ui.add(egui::Slider::new(&mut h.cols, 1..=10).text("(1-10)"));
    });
    ui.horizontal(|ui| {
        ui.label("hole 間 pitch (mm):");
        ui.add(egui::Slider::new(&mut h.spacing, 10.0..=20.0).step_by(0.5));
    });

    let h_copy = *h;
    let label = format!(
        "ビットホルダー {}×{} @ {}mm",
        h_copy.rows, h_copy.cols, h_copy.spacing
    );
    ui.label("固定: hex 6.85mm across-flats / depth 14mm (1/4\" bit 標準)");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, h_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// Raspberry Pi ケース customizer (`pcb_w × pcb_d × internal_h`、electronics § 1)
///
/// 4 corner standoff peg (M2.5 pilot) + 長辺 port opening (60mm 幅) + top open
/// Default: RPi 5 with Active Cooler (85×56×25mm) / bare Pi (h=15) / Zero 2W (65×30×15)
fn show_raspi_case_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🥧 Raspberry Pi ケース (standoff + port opening)").strong());

    let c = &mut state.customizer_state.raspi_case;
    ui.horizontal(|ui| {
        ui.label("PCB 幅 (mm):");
        ui.add(egui::Slider::new(&mut c.pcb_width, 40.0..=120.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label("PCB 奥行 (mm):");
        ui.add(egui::Slider::new(&mut c.pcb_depth, 20.0..=80.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label("内部高さ (mm):");
        ui.add(egui::Slider::new(&mut c.internal_height, 10.0..=40.0).step_by(1.0));
    });

    let c_copy = *c;
    let label = format!(
        "RPi ケース {}×{}×{}mm",
        c_copy.pcb_width, c_copy.pcb_depth, c_copy.internal_height
    );
    ui.label("プリセット目安: RPi 5/4 (85×56、cooler 25 / bare 15) / Zero 2W (65×30×15)");
    ui.label("固定: 4 corner standoff Ø6mm × H5mm + M2.5 pilot、port opening 60mm 幅");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, c_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// ESP32/Arduino エンクロージャ customizer (`pcb_w × pcb_d × internal_h`、electronics § 2)
///
/// standoff なし friction cradle + 短辺 USB opening (9×5mm) + top open
/// Default: ESP32 DevKit V1 (51.6×28.4×15) / Arduino Uno R3 (68.6×53.4×20) / Nano (45×18×12)
fn show_esp32_enclosure_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(
        egui::RichText::new("🔌 ESP32/Arduino エンクロージャ (friction、USB opening)").strong(),
    );

    let e = &mut state.customizer_state.esp32_enclosure;
    ui.horizontal(|ui| {
        ui.label("PCB 幅 (mm):");
        ui.add(egui::Slider::new(&mut e.pcb_width, 30.0..=100.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label("PCB 奥行 (mm):");
        ui.add(egui::Slider::new(&mut e.pcb_depth, 15.0..=80.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label("内部高さ (mm):");
        ui.add(egui::Slider::new(&mut e.internal_height, 8.0..=30.0).step_by(1.0));
    });

    let e_copy = *e;
    let label = format!(
        "MCU ケース {}×{}×{}mm",
        e_copy.pcb_width, e_copy.pcb_depth, e_copy.internal_height
    );
    ui.label("プリセット目安: ESP32 (51.6×28.4×15) / Arduino Uno (68.6×53.4×20) / Nano (45×18×12)");
    ui.label("固定: USB opening 短辺 9×5mm (USB-C 想定、Micro/Type-A は別途)");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, e_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// 18650 バッテリーホルダー customizer (`count × wall × floor`、electronics § 3)
///
/// row 状 cylindrical cavity (Ø18.6mm × L68mm 固定)
/// floor=0 なら両端貫通 (cell 挿入 open)、>0 なら片端閉塞 (spring 保持)
fn show_battery_18650_holder_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🔋 18650 バッテリーホルダー (row 状 cavity)").strong());

    let b = &mut state.customizer_state.battery_18650_holder;
    ui.horizontal(|ui| {
        ui.label("cell 個数:");
        ui.add(egui::Slider::new(&mut b.cell_count, 1..=10).text("(1-10)"));
    });
    ui.horizontal(|ui| {
        ui.label("inter-cell 壁厚 (mm):");
        ui.add(egui::Slider::new(&mut b.wall_thickness, 2.0..=4.0).step_by(0.1));
    });
    ui.horizontal(|ui| {
        ui.label("端部 floor 厚 (mm):");
        ui.add(egui::Slider::new(&mut b.floor_thickness, 0.0..=5.0).step_by(0.5));
    });

    let b_copy = *b;
    let label = format!(
        "18650 × {} (wall {}mm, floor {}mm)",
        b_copy.cell_count, b_copy.wall_thickness, b_copy.floor_thickness
    );
    ui.label("固定: cell Ø18.6mm × L68mm (18650 Li-ion 標準 + FDM clearance)");
    ui.label("floor=0 → 両端貫通 / floor>0 → 片端閉塞 (spring 保持)、素材は PETG/ABS 推奨");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, b_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// 歯ブラシホルダー customizer (`count × hole_diameter × height`、bathroom § 7.1)
///
/// row 状 cylindrical hole、top 開口 (Ø15 manual / Ø40 electric)
/// 素材は PETG 推奨 (moisture resistance)
fn show_toothbrush_holder_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🪥 歯ブラシホルダー (row 状 hole、top 開口)").strong());

    let t = &mut state.customizer_state.toothbrush_holder;
    ui.horizontal(|ui| {
        ui.label("hole 個数:");
        ui.add(egui::Slider::new(&mut t.count, 1..=8).text("(1-8)"));
    });
    ui.horizontal(|ui| {
        ui.label("hole 直径 (mm):");
        ui.add(egui::Slider::new(&mut t.hole_diameter, 10.0..=45.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label("hole 深さ (mm):");
        ui.add(egui::Slider::new(&mut t.hole_depth, 50.0..=120.0).step_by(1.0));
    });

    let t_copy = *t;
    let label = format!(
        "歯ブラシホルダー {} × Ø{}×H{}mm",
        t_copy.count, t_copy.hole_diameter, t_copy.hole_depth
    );
    ui.label("プリセット目安: manual (Ø15) / electric Sonicare (Ø32) / electric Oral-B (Ø40)");
    ui.label("素材: PETG 推奨 (moisture resistance)、drainage 穴は user 側で追加加工推奨");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, t_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// ドリルビットホルダー customizer (`min_mm × max_mm × count`、garage § 8.1)
///
/// row 状 hole、size linear interpolate (wrench_holder の hole 円形版)
fn show_drill_bit_holder_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🪛 ドリルビットホルダー (row 状 hole、min-max 補間)").strong());

    let d = &mut state.customizer_state.drill_bit_holder;
    ui.horizontal(|ui| {
        ui.label("最小径 (mm):");
        ui.add(egui::Slider::new(&mut d.min_size_mm, 1.0..=8.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label("最大径 (mm):");
        ui.add(egui::Slider::new(&mut d.max_size_mm, 5.0..=20.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label("hole 個数:");
        ui.add(egui::Slider::new(&mut d.count, 5..=25).text("(5-25)"));
    });

    let d_copy = *d;
    let label = format!(
        "ドリルビット {}-{}mm × {}",
        d_copy.min_size_mm, d_copy.max_size_mm, d_copy.count
    );
    ui.label("プリセット目安: Metric 3-13mm × 11 (1mm step) / 1-10mm × 19 (0.5mm step)");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, d_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// プライヤーラック customizer (`slot_count × slot_width × slot_depth`、garage § 8.4)
///
/// row 状 rect slot、top 開口、pliers 挿入
fn show_pliers_rack_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🔧 プライヤーラック (row 状 rect slot)").strong());

    let p = &mut state.customizer_state.pliers_rack;
    ui.horizontal(|ui| {
        ui.label("slot 個数:");
        ui.add(egui::Slider::new(&mut p.slot_count, 3..=12).text("(3-12)"));
    });
    ui.horizontal(|ui| {
        ui.label("slot 幅 (mm):");
        ui.add(egui::Slider::new(&mut p.slot_width, 8.0..=30.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label("slot 深さ (mm):");
        ui.add(egui::Slider::new(&mut p.slot_depth, 40.0..=90.0).step_by(1.0));
    });

    let p_copy = *p;
    let label = format!(
        "プライヤーラック {} × W{}×D{}mm",
        p_copy.slot_count, p_copy.slot_width, p_copy.slot_depth
    );
    ui.label("プリセット目安: needle-nose (W10) / combi (W15) / tongue-groove (W20-25)");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, p_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// スパイスラック customizer (`count × jar_diameter × jar_height`、kitchen § 6.1)
///
/// 薄 shelf + jar 用 shallow recess (5mm) + 前縁 lip (jar_height × 15%)
fn show_spice_rack_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🧂 スパイスラック (shelf + jar recess + lip)").strong());

    let s = &mut state.customizer_state.spice_rack;
    ui.horizontal(|ui| {
        ui.label("jar 個数:");
        ui.add(egui::Slider::new(&mut s.count, 3..=12).text("(3-12)"));
    });
    ui.horizontal(|ui| {
        ui.label("jar 直径 (mm):");
        ui.add(egui::Slider::new(&mut s.jar_diameter, 40.0..=55.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label("jar 高さ (mm):");
        ui.add(egui::Slider::new(&mut s.jar_height, 70.0..=130.0).step_by(1.0));
    });

    let s_copy = *s;
    let label = format!(
        "スパイスラック {} jar × Ø{}×H{}mm",
        s_copy.count, s_copy.jar_diameter, s_copy.jar_height
    );
    ui.label("プリセット目安: small (Ø42×H75) / std (Ø48×H100) / large (Ø52×H120)");
    ui.label("固定: recess 深 5mm、shelf 厚 5mm、front lip 高 = jar_height × 15%");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, s_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// 卵トレー customizer (`rows × cols × cup_depth`、kitchen § 6.5)
///
/// 2D grid 状 cup、egg cup Ø40mm 固定、pitch 50mm 固定
fn show_egg_tray_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🥚 卵トレー (2D grid、egg cup Ø40mm 固定)").strong());

    let e = &mut state.customizer_state.egg_tray;
    ui.horizontal(|ui| {
        ui.label("行数:");
        ui.add(egui::Slider::new(&mut e.rows, 1..=8).text("(1-8)"));
    });
    ui.horizontal(|ui| {
        ui.label("列数:");
        ui.add(egui::Slider::new(&mut e.cols, 1..=8).text("(1-8)"));
    });
    ui.horizontal(|ui| {
        ui.label("cup 深さ (mm):");
        ui.add(egui::Slider::new(&mut e.cup_depth, 12.0..=25.0).step_by(0.5));
    });

    let e_copy = *e;
    let label = format!(
        "卵トレー {}×{} × 深{}mm",
        e_copy.rows, e_copy.cols, e_copy.cup_depth
    );
    ui.label("プリセット目安: 12-egg tray (4×3) / 6-egg (3×2) / 4×4 (16-egg 大量)");
    ui.label("固定: egg cup Ø40mm (large egg spec)、pitch 50mm、素材 PETG 推奨 (冷蔵庫用)");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, e_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// キッチンツールキャディ customizer (`count × compartment_dia × height`、kitchen § 6.8)
///
/// row 状 large cylindrical compartment (spatula / ladle / whisk / tongs 分別)
fn show_utensil_caddy_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🍴 キッチンツールキャディ (row 状 large compartment)").strong());

    let u = &mut state.customizer_state.utensil_caddy;
    ui.horizontal(|ui| {
        ui.label("compartment 個数:");
        ui.add(egui::Slider::new(&mut u.count, 1..=6).text("(1-6)"));
    });
    ui.horizontal(|ui| {
        ui.label("compartment 内径 (mm):");
        ui.add(egui::Slider::new(&mut u.compartment_diameter, 45.0..=80.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label("compartment 高さ (mm):");
        ui.add(egui::Slider::new(&mut u.height, 100.0..=180.0).step_by(5.0));
    });

    let u_copy = *u;
    let label = format!(
        "ツールキャディ {} × Ø{}×H{}mm",
        u_copy.count, u_copy.compartment_diameter, u_copy.height
    );
    ui.label("プリセット目安: small (Ø45-50、whisk/peeler) / large (Ø60-70、spatula/ladle)");
    ui.label("素材: PETG 推奨 (水濺ね対応)、drainage 穴は user 側で追加加工");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, u_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// フィラメントスプールホルダー customizer
/// (`spool_od × spool_width × bore_dia`、printer § 9.1)
///
/// base plate + 垂直 peg (spool bore over peg、donut on pole style)
fn show_filament_spool_holder_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🎞 フィラメントスプールホルダー (base + 垂直 peg)").strong());

    let f = &mut state.customizer_state.filament_spool_holder;
    ui.horizontal(|ui| {
        ui.label("spool 外径 (mm):");
        ui.add(egui::Slider::new(&mut f.spool_outer_diameter, 100.0..=300.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label("spool 幅 (mm):");
        ui.add(egui::Slider::new(&mut f.spool_width, 30.0..=120.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label("bore 内径 (mm):");
        ui.add(egui::Slider::new(&mut f.bore_diameter, 30.0..=100.0).step_by(1.0));
    });

    let f_copy = *f;
    let label = format!(
        "スプールホルダー Ø{}×W{}×bore{}mm",
        f_copy.spool_outer_diameter, f_copy.spool_width, f_copy.bore_diameter
    );
    ui.label(
        "プリセット目安: 1kg (Ø200×W68×bore52) / 250g (Ø125×W45×bore30) / 2kg (Ø250×W80×bore70)",
    );
    ui.label("固定: base_thickness 5mm、peg clearance 1mm (slide fit)、peg 追加高 20mm");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, f_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// ノズルホルダー customizer (`count × hole_diameter × depth`、printer § 9.5)
///
/// row 状 small hole for M6 nozzles (E3D V6 / Bambu M6)
fn show_nozzle_holder_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🔩 ノズルホルダー (row 状 M6 nozzle hole)").strong());

    let n = &mut state.customizer_state.nozzle_holder;
    ui.horizontal(|ui| {
        ui.label("hole 個数:");
        ui.add(egui::Slider::new(&mut n.count, 3..=15).text("(3-15)"));
    });
    ui.horizontal(|ui| {
        ui.label("hole 直径 (mm):");
        ui.add(egui::Slider::new(&mut n.hole_diameter, 6.0..=15.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label("hole 深さ (mm):");
        ui.add(egui::Slider::new(&mut n.hole_depth, 4.0..=15.0).step_by(0.5));
    });

    let n_copy = *n;
    let label = format!(
        "ノズルホルダー {} hole × Ø{}×D{}mm",
        n_copy.count, n_copy.hole_diameter, n_copy.hole_depth
    );
    ui.label("プリセット目安: E3D V6/Bambu M6 (Ø8×D6) / large hotend (Ø10-12×D8)");
    ui.label("Label は user 側で別途印刷 or Sharpie 書込み推奨");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, n_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// ビルドプレートラック customizer
/// (`slot_count × slot_spacing × height`、printer § 9.6)
///
/// row 状 vertical slot for 5mm-thick build plates
fn show_build_plate_rack_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🏗 ビルドプレートラック (row 状 vertical slot)").strong());

    let r = &mut state.customizer_state.build_plate_rack;
    ui.horizontal(|ui| {
        ui.label("slot 個数:");
        ui.add(egui::Slider::new(&mut r.slot_count, 2..=10).text("(2-10)"));
    });
    ui.horizontal(|ui| {
        ui.label("slot spacing (mm):");
        ui.add(egui::Slider::new(&mut r.slot_spacing, 12.0..=25.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label("rack 全高 (mm):");
        ui.add(egui::Slider::new(&mut r.height, 150.0..=350.0).step_by(5.0));
    });

    let r_copy = *r;
    let label = format!(
        "プレートラック {} slot × spacing {}mm × H{}mm",
        r_copy.slot_count, r_copy.slot_spacing, r_copy.height
    );
    ui.label("プリセット目安: Ender/Bambu 235mm (H200) / Bambu 256mm (H225) / Voron 350mm (H300)");
    ui.label("固定: slot width 5.5mm (5mm plate + 0.5mm clearance)、depth 60mm");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, r_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// カトラリートレー customizer (`slot_count × slot_width × slot_length`、drawer § 3.2)
///
/// row 状 long rect slot (fork/knife/spoon 分別、drawer 引き出し向け)
fn show_cutlery_tray_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🍴 カトラリートレー (drawer 引き出し用、long slot)").strong());

    let c = &mut state.customizer_state.cutlery_tray;
    ui.horizontal(|ui| {
        ui.label("slot 個数:");
        ui.add(egui::Slider::new(&mut c.slot_count, 2..=8).text("(2-8)"));
    });
    ui.horizontal(|ui| {
        ui.label("slot 幅 (mm):");
        ui.add(egui::Slider::new(&mut c.slot_width, 20.0..=60.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label("slot 長 (mm):");
        ui.add(egui::Slider::new(&mut c.slot_length, 150.0..=350.0).step_by(5.0));
    });

    let c_copy = *c;
    let label = format!(
        "カトラリートレー {} slot × W{}×L{}mm",
        c_copy.slot_count, c_copy.slot_width, c_copy.slot_length
    );
    ui.label("プリセット目安: fork (W30-35) / knife (W25-30) / spoon (W50-55)、長さ 220mm 標準");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, c_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// 薬箱 customizer (`rows × cols × cell_size`、drawer § 3.6)
///
/// 2D grid rect cells (weekly pill box、egg_tray の rect 版)
fn show_pill_organizer_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("💊 薬箱 (2D grid rect cell、weekly pill box)").strong());

    let p = &mut state.customizer_state.pill_organizer;
    ui.horizontal(|ui| {
        ui.label("行数:");
        ui.add(egui::Slider::new(&mut p.rows, 1..=14).text("(1-14)"));
    });
    ui.horizontal(|ui| {
        ui.label("列数:");
        ui.add(egui::Slider::new(&mut p.cols, 1..=8).text("(1-8)"));
    });
    ui.horizontal(|ui| {
        ui.label("cell 内寸 (mm):");
        ui.add(egui::Slider::new(&mut p.cell_size, 15.0..=30.0).step_by(0.5));
    });

    let p_copy = *p;
    let label = format!(
        "薬箱 {}×{} × cell {}mm",
        p_copy.rows, p_copy.cols, p_copy.cell_size
    );
    ui.label("プリセット目安: weekly AM/PM (7×2×20) / small daily (3×1×15) / large (7×4×25)");
    ui.label("固定: cell 深 15mm、wall 1.5mm、floor 1.5mm");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, p_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// マグネットストリップ customizer
/// (`magnet_count × magnet_diameter × spacing`、wall § 4.6)
///
/// long thin bar + row of magnet holes (kitchen knife rail / tool retention)
fn show_magnetic_strip_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🧲 マグネットストリップ (long thin bar + magnet)").strong());

    let m = &mut state.customizer_state.magnetic_strip;
    ui.horizontal(|ui| {
        ui.label("magnet 個数:");
        ui.add(egui::Slider::new(&mut m.magnet_count, 3..=15).text("(3-15)"));
    });
    ui.horizontal(|ui| {
        ui.label("magnet 直径 (mm):");
        ui.add(egui::Slider::new(&mut m.magnet_diameter, 4.0..=15.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label("magnet spacing (mm):");
        ui.add(egui::Slider::new(&mut m.magnet_spacing, 20.0..=60.0).step_by(1.0));
    });

    let m_copy = *m;
    let label = format!(
        "マグネットバー {} × Ø{} spacing {}mm",
        m_copy.magnet_count, m_copy.magnet_diameter, m_copy.magnet_spacing
    );
    ui.label(
        "プリセット目安: kitchen knife rail (8×Ø6×30) / small tool (5×Ø8×25) / large (12×Ø10×40)",
    );
    ui.label("固定: bar 厚 5mm、bar 高 15mm、magnet 埋込 2mm (magnet は user 側で press-fit 挿入)");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, m_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// ヘアドライヤーホルダー customizer
/// (`barrel_diameter × holster_depth × wall_thickness`、bathroom § 7.7)
///
/// 大径 cylindrical holster (Dyson Supersonic / 汎用ドライヤー対応)
fn show_hairdryer_holder_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("💨 ヘアドライヤーホルダー (大径 holster)").strong());

    let h = &mut state.customizer_state.hairdryer_holder;
    ui.horizontal(|ui| {
        ui.label("barrel 内径 (mm):");
        ui.add(egui::Slider::new(&mut h.barrel_diameter, 40.0..=120.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label("holster 深さ (mm):");
        ui.add(egui::Slider::new(&mut h.holster_depth, 80.0..=150.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label("壁厚 (mm):");
        ui.add(egui::Slider::new(&mut h.wall_thickness, 2.0..=6.0).step_by(0.5));
    });

    let h_copy = *h;
    let label = format!(
        "ドライヤーホルダー Ø{}×D{}mm",
        h_copy.barrel_diameter, h_copy.holster_depth
    );
    ui.label("プリセット目安: Dyson Supersonic (Ø85) / 汎用 (Ø45-90) / 業務用 (Ø100+)");
    ui.label("固定: 内 clearance 2mm、floor 5mm (荷重 400-700g 想定)");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, h_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// K-Cup ホルダー customizer (`rows × cols × capsule_diameter`、kitchen § 6.7)
///
/// 2D grid K-Cup wells (K-Cup Ø53 / Nespresso Ø39 / Dolce Gusto Ø55 対応)
fn show_kcup_holder_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("☕ K-Cup ホルダー (2D grid capsule wells)").strong());

    let k = &mut state.customizer_state.kcup_holder;
    ui.horizontal(|ui| {
        ui.label("行数:");
        ui.add(egui::Slider::new(&mut k.rows, 1..=6).text("(1-6)"));
    });
    ui.horizontal(|ui| {
        ui.label("列数:");
        ui.add(egui::Slider::new(&mut k.cols, 1..=6).text("(1-6)"));
    });
    ui.horizontal(|ui| {
        ui.label("capsule 直径 (mm):");
        ui.add(egui::Slider::new(&mut k.capsule_diameter, 35.0..=60.0).step_by(1.0));
    });

    let k_copy = *k;
    let label = format!(
        "K-Cup ホルダー {}×{} × Ø{}mm",
        k_copy.rows, k_copy.cols, k_copy.capsule_diameter
    );
    ui.label("プリセット目安: K-Cup (Ø53) / Nespresso Original (Ø39) / Dolce Gusto (Ø55)");
    ui.label("固定: capsule 深 40mm、pitch = capsule + 3.5mm、floor 3mm");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, k_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}

/// ヘックスキーホルダー customizer
/// (`count × min_key_mm × max_key_mm`、garage § 8.2)
///
/// row 状 hole linear interpolate (Metric 9-piece / SAE 12-piece、drill_bit pattern)
fn show_hex_key_holder_customizer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("🔩 ヘックスキーホルダー (Allen key、block-style)").strong());

    let h = &mut state.customizer_state.hex_key_holder;
    ui.horizontal(|ui| {
        ui.label("key 個数:");
        ui.add(egui::Slider::new(&mut h.count, 5..=15).text("(5-15)"));
    });
    ui.horizontal(|ui| {
        ui.label("最小 key 幅 (mm):");
        ui.add(egui::Slider::new(&mut h.min_key_mm, 1.0..=4.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label("最大 key 幅 (mm):");
        ui.add(egui::Slider::new(&mut h.max_key_mm, 6.0..=15.0).step_by(0.5));
    });

    let h_copy = *h;
    let label = format!(
        "ヘックスキーホルダー {}-{}mm × {}",
        h_copy.min_key_mm, h_copy.max_key_mm, h_copy.count
    );
    ui.label("プリセット目安: Metric 9-piece (1.5-10mm) / SAE 12-piece (0.05-3/8 inch)");
    ui.label("固定: hole 深 18mm、clearance 0.3mm/side (key + 0.6mm total)");
    if ui.button(format!("作成: {label}")).clicked() {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, h_copy.to_lol(), &label);
    }

    ui.add_space(2.0);
}
