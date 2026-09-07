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
    /// 2026-09-04 案 A: Gallery「編集して再生成」button click 直後の 1 frame
    /// だけ true にして CollapsingHeader を強制展開 その frame の中で
    /// prompt_input が既に流し込まれているので user は即 edit 可
    pub force_open_experimental: bool,
}

impl Default for PromptUiState {
    fn default() -> Self {
        Self {
            export_format: UiExportFormat::ThreeMf,
            last_export_error: None,
            color4: Color4UiState::default(),
            force_open_experimental: false,
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

fn projection_label(p: alice_bamboo::color4::ProjectionAxis, lang: Lang) -> &'static str {
    use alice_bamboo::color4::ProjectionAxis;
    match p {
        ProjectionAxis::PositiveZ => crate::i18n::T::prompt_p000(lang),
        ProjectionAxis::NegativeZ => crate::i18n::T::prompt_p001(lang),
        ProjectionAxis::PositiveY => crate::i18n::T::prompt_p002(lang),
        ProjectionAxis::NegativeY => crate::i18n::T::prompt_p003(lang),
        ProjectionAxis::PositiveX => crate::i18n::T::prompt_p004(lang),
        ProjectionAxis::NegativeX => crate::i18n::T::prompt_p005(lang),
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
        show_model_download(ui, state, lang);
        ui.add_space(4.0);
    }

    show_sidecar_status(ui, state, lang);

    let limits = state.tier.limits();
    let usage = state.daily_usage();
    if limits.daily_generations == u32::MAX {
        // v0.1.0-beta.1: Free tier restriction 撤廃、'/ 4294967295' 表示
        // は醜いので usage 件数のみ表示
        ui.label(crate::i18n::T::prompt_fmt_usage_beta(usage, lang));
    } else {
        ui.label(crate::i18n::T::prompt_fmt_usage_limit(
            usage,
            limits.daily_generations,
            lang,
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
    show_prompt_templates(ui, state, is_generating, lang);

    ui.add_space(6.0);
    ui.label(egui::RichText::new(crate::i18n::T::customizer_section(lang)).strong());
    show_prompt_customizer(ui, state, is_generating, lang);

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);

    // Experimental LLM path — collapsed by default so first-time users
    // land on the working templates/customizer path instead of the
    // 10-minute LLM wait that ends in a plain cube (local 3B model 実力壁)
    // 案 A: force_open_experimental が true なら 1 frame だけ強制展開
    // (Gallery「編集して再生成」click 経路から流入した LOL を user が
    // 即 edit できるように section を開いた状態で見せる) flag は消費して false に
    let force_open = ui_state.force_open_experimental;
    if force_open {
        ui_state.force_open_experimental = false;
    }
    let mut header = egui::CollapsingHeader::new(
        egui::RichText::new(crate::i18n::T::experimental_llm_header(lang))
            .strong()
            .color(egui::Color32::from_rgb(200, 140, 60)),
    )
    .id_salt("experimental_llm_section");
    if force_open {
        header = header.open(Some(true));
    } else {
        header = header.default_open(false);
    }
    header.show(ui, |ui| {
        ui.label(
            egui::RichText::new(crate::i18n::T::experimental_llm_hint(lang))
                .small()
                .color(egui::Color32::from_rgb(200, 140, 60)),
        );
        ui.add_space(4.0);
        ui.label(crate::i18n::T::prompt_p008(lang));

        let prompt_id = egui::Id::new("prompt_input");
        let prompt_widget = egui::TextEdit::multiline(&mut state.prompt_input)
            .id(prompt_id)
            .desired_rows(3)
            .desired_width(f32::INFINITY)
            .hint_text(crate::i18n::T::prompt_p009(lang));
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
            .add_enabled(
                !is_generating && can_gen,
                egui::Button::new(crate::i18n::T::prompt_p010(lang)),
            )
            .clicked()
        {
            start_generation(state, lang);
        }

        if !state.can_generate() && !is_generating {
            let warn_color = ui.style().visuals.warn_fg_color;
            ui.colored_label(warn_color, crate::i18n::T::prompt_p011(lang));
        }
    });

    ui.add_space(8.0);

    if is_generating || !state.phase_progress.completed.is_empty() {
        show_phase_progress(ui, &state.phase_progress, lang);
        ui.add_space(8.0);
    }

    match &state.generation_status {
        GenerationStatus::Idle => {}
        GenerationStatus::Generating => {
            ui.spinner();
            ui.label(crate::i18n::T::prompt_p012(lang));
        }
        GenerationStatus::Done {
            lol_source,
            mesh_stats,
        } => {
            ui.colored_label(egui::Color32::GREEN, crate::i18n::T::prompt_p013(lang));

            if let Some(stats) = mesh_stats {
                ui.label(crate::i18n::T::prompt_fmt_verts_tris(
                    stats.vertex_count,
                    stats.triangle_count,
                    lang,
                ));
                ui.label(crate::i18n::T::prompt_fmt_saved_to(
                    anonymize_home(&stats.path),
                    lang,
                ));

                if let Some(overhang) = &stats.overhang_summary {
                    ui.label(crate::i18n::T::prompt_fmt_overhang(
                        overhang.overhang_ratio * 100.0,
                        overhang.overhang_face_count,
                        overhang.total_face_count,
                        overhang.max_wall_angle_deg,
                        lang,
                    ));
                }
                if let Some(slice) = &stats.slice_summary {
                    let mins = (slice.print_time_seconds / 60.0).round() as u32;
                    ui.label(crate::i18n::T::prompt_fmt_gcode(
                        slice.layer_count,
                        mins,
                        slice.filament_meters,
                        lang,
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
                        crate::i18n::T::prompt_fmt_safety(
                            &safety.material_name,
                            if safety.is_safe {
                                "OK"
                            } else {
                                crate::i18n::T::prompt_p019(lang)
                            },
                            &safety.warp_category,
                            lang,
                        ),
                    );
                    for msg in &safety.messages {
                        ui.colored_label(warn_color, format!("  {msg}"));
                    }
                }
            }

            ui.collapsing(crate::i18n::T::prompt_p020(lang), |ui| {
                ui.monospace(lol_source);
            });

            if state.tier.limits().can_download {
                ui.add_space(4.0);
                let lol = lol_source.clone();
                show_export_dropdown(ui, state, ui_state, &lol, lang);
                ui.add_space(4.0);
                show_color4_export(ui, state, ui_state, &lol, lang);
            } else {
                ui.colored_label(egui::Color32::GRAY, crate::i18n::T::prompt_p021(lang));
            }
        }
        GenerationStatus::Error(msg) => {
            ui.colored_label(
                egui::Color32::RED,
                crate::i18n::T::prompt_fmt_error_msg(msg, lang),
            );
        }
    }

    if let Some(err) = &ui_state.last_export_error {
        ui.add_space(4.0);
        ui.colored_label(
            egui::Color32::RED,
            crate::i18n::T::prompt_fmt_export_failed(err, lang),
        );
    }
}

fn show_model_download(ui: &mut Ui, state: &AppState, lang: Lang) {
    let progress = state.model_progress.borrow().clone();
    match progress.status {
        text_to_print_llm::downloader::DownloadStatus::Downloading => {
            // theme-adaptive warn color (light/dark 両テーマで readable、
            // egui native YELLOW は light theme で contrast 不足)
            let warn_color = ui.style().visuals.warn_fg_color;
            ui.colored_label(warn_color, crate::i18n::T::prompt_p024(lang));
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
            ui.colored_label(
                egui::Color32::RED,
                crate::i18n::T::prompt_fmt_model_dl_error(e, lang),
            );
        }
        text_to_print_llm::downloader::DownloadStatus::Pending => {
            ui.label(crate::i18n::T::prompt_p026(lang));
            ui.ctx().request_repaint();
        }
    }
}

/// `alice-llm-server` sidecar プロセスの状態を表示
///
/// Running 時は何も出さない (通常運用時の視覚ノイズを避ける)
/// Waiting/Starting は spinner、Error はメッセージ + 起動 hint を表示
fn show_sidecar_status(ui: &mut Ui, state: &AppState, lang: Lang) {
    let status = state.sidecar_status.borrow().clone();
    match status {
        SidecarStatus::Waiting => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(crate::i18n::T::prompt_p027(lang));
            });
            ui.ctx().request_repaint();
            ui.add_space(4.0);
        }
        SidecarStatus::Starting => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(crate::i18n::T::prompt_p028(lang));
            });
            ui.ctx().request_repaint();
            ui.add_space(4.0);
        }
        SidecarStatus::Running => {
            // 通常運用時は非表示
        }
        SidecarStatus::Error(msg) => {
            ui.colored_label(
                egui::Color32::RED,
                crate::i18n::T::prompt_fmt_sidecar_start_fail(msg, lang),
            );
            ui.label(crate::i18n::T::prompt_lit_sidecar_help_msg(lang));
            ui.add_space(4.0);
        }
    }
}

/// Replace the user's `$HOME` prefix in a file path with `~` so the UI
/// doesn't leak their username in crate::i18n::T::prompt_p031(lang) / preview labels When `$HOME`
/// isn't set or doesn't match, returns the input unchanged
fn anonymize_home(path: &str) -> String {
    if let Some(home) = std::env::var_os("HOME").and_then(|h| h.into_string().ok())
        && let Some(rest) = path.strip_prefix(&home)
    {
        return format!("~{rest}");
    }
    path.to_string()
}

fn show_phase_progress(ui: &mut Ui, progress: &PhaseProgress, lang: Lang) {
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
            ui.monospace(crate::i18n::T::prompt_fmt_phase_running(
                phase_label,
                sec / 60,
                sec % 60,
                lang,
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
        ui.label(crate::i18n::T::prompt_p033(lang));
        egui::ComboBox::from_id_salt("export_format")
            .selected_text(display_label(ui_state.export_format, lang))
            .show_ui(ui, |ui| {
                for fmt in UiExportFormat::ALL {
                    let text = display_label(fmt, lang);
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
            .add_enabled(
                can_export,
                egui::Button::new(crate::i18n::T::prompt_p034(lang)),
            )
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
    lang: Lang,
) {
    ui.collapsing(crate::i18n::T::prompt_p035(lang), |ui| {
        ui.label(crate::i18n::T::prompt_p036(lang));

        // n_colors slider (2-4)
        ui.horizontal(|ui| {
            ui.label(crate::i18n::T::prompt_p037(lang));
            ui.add(egui::Slider::new(&mut ui_state.color4.n_colors, 2..=4));
        });

        // Projection axis dropdown
        ui.horizontal(|ui| {
            ui.label(crate::i18n::T::prompt_p038(lang));
            egui::ComboBox::from_id_salt("color4_projection")
                .selected_text(projection_label(ui_state.color4.projection, lang))
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
                            projection_label(axis, lang),
                        );
                    }
                });
        });

        // Image picker
        ui.horizontal(|ui| {
            if ui.button(crate::i18n::T::prompt_p039(lang)).clicked() {
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
                ui.colored_label(egui::Color32::GRAY, crate::i18n::T::prompt_p040(lang));
            }
        });

        // Export button
        let can_export = ui_state.color4.image_path.is_some();
        if ui
            .add_enabled(
                can_export,
                egui::Button::new(crate::i18n::T::prompt_p041(lang)),
            )
            .clicked()
            && let Some(image_path) = ui_state.color4.image_path.clone()
        {
            run_color4_export(state, ui_state, lol_source, &image_path, lang);
        }

        if let Some(err) = &ui_state.color4.last_error {
            ui.add_space(4.0);
            ui.colored_label(
                egui::Color32::RED,
                crate::i18n::T::prompt_fmt_color4_fail(err, lang),
            );
        }
        if !ui_state.color4.last_generated.is_empty() {
            ui.add_space(4.0);
            ui.label(crate::i18n::T::prompt_fmt_color4_done(
                ui_state.color4.last_generated.len(),
                ui_state.color4.last_generated.len(),
                lang,
            ));
            for p in &ui_state.color4.last_generated {
                ui.monospace(p.display().to_string());
            }
            if let Some(first) = ui_state.color4.last_generated.first()
                && let Some(parent) = first.parent()
                && ui.button(crate::i18n::T::prompt_p044(lang)).clicked()
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
    lang: Lang,
) {
    ui_state.color4.last_error = None;
    ui_state.color4.last_generated.clear();

    let image = match image::open(image_path) {
        Ok(img) => img.to_rgb8(),
        Err(e) => {
            ui_state.color4.last_error = Some(crate::i18n::T::prompt_fmt_image_load_fail(e, lang));
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

fn display_label(fmt: UiExportFormat, lang: Lang) -> String {
    if fmt.to_supported().is_some() {
        fmt.label().to_string()
    } else {
        crate::i18n::T::prompt_fmt_unsupported_suffix(fmt.label(), lang)
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
fn start_generation_from_lol(
    state: &mut AppState,
    lol_source: String,
    template_name: &str,
    lang: Lang,
) {
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
                    pipeline_error = Some(crate::i18n::T::prompt_fmt_template_gen_fail(e, lang));
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
                share_confirm_pending: None,
            });
        }
    });
}

fn start_generation(state: &mut AppState, lang: Lang) {
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
    // Gallery Phase 2 (2026-08-26): auto vs confirm-dialog gate `false`
    // (default) means the async task dry-runs for audit but defers the
    // real `enqueue()` until the UI dialog resolves it `true` restores
    // the pre-Phase-2 auto-publish flow (backward compatible for users
    // who opt in)
    let gallery_auto_share = state.gallery_auto_share;
    let share_dry_run_dir = state.share_dry_run_dir();
    let share_queue_dir = state.share_queue_dir();
    let tier_slug = tier_slug(state.tier);
    let model_id = state.llm_config.model_choice.model_id().to_string();
    let prompt_lang = lang.as_bcp47().to_string();

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

                // 2026-09-04 Fix B: pre-export parse gate LLM retry loop の
                // safety_check_lol が parse error を検出しないケース
                // (unclassified violations で break、backend bug 等) の
                // fail-safe として、export 呼ぶ前に explicit parse verify
                // 失敗時は user-friendly error を surface して early return
                if let Err(e) = pipeline::validate_lol(&lol) {
                    let _ = tx.send(GenerationMessage::PhaseDone(
                        GenerationPhase::Parse,
                        parse_start.elapsed(),
                    ));
                    let clean_err = format!("{e}");
                    tracing::error!(
                        error = %clean_err,
                        lol_preview = %lol.chars().take(200).collect::<String>(),
                        retry_count,
                        "pre-export parse gate: LLM output failed to parse after retries"
                    );
                    let _ = tx.send(GenerationMessage::Failure {
                        id,
                        error: crate::i18n::T::prompt_fmt_llm_invalid_dsl(&clean_err, lang),
                    });
                    return;
                }

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

                // GAP-12 / Stage 5 / Gallery Phase 2: gate the share
                // payload on the tier-effective opt-in flag Free-tier +
                // opt-in → dry-run dump (local audit) is always done;
                // real `enqueue()` is auto when `gallery_auto_share` is
                // on and deferred to a dialog otherwise Paid tiers
                // short-circuit the branch entirely
                let (share_dry_run, share_confirm_pending) = if share_enabled
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
                    let dry_path = match text_to_print_network::share::dump_dry_run(
                        &payload,
                        &share_dry_run_dir,
                    ) {
                        Ok(p) => Some(p),
                        Err(e) => {
                            tracing::warn!(error = %e, "share dry-run dump failed");
                            None
                        }
                    };
                    if gallery_auto_share {
                        // Auto path: enqueue immediately for the next
                        // `retry_queued_uploads` sweep, no dialog
                        if let Err(e) =
                            text_to_print_network::share::enqueue(&payload, &share_queue_dir)
                        {
                            tracing::warn!(error = %e, "share enqueue failed");
                        }
                        (dry_path, None)
                    } else {
                        // Dialog path: dry-run kept for audit but the
                        // real enqueue is deferred until the user
                        // resolves `pending_share_confirm`
                        (dry_path.clone(), dry_path)
                    }
                } else {
                    (None, None)
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
                        share_confirm_pending,
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
                share_confirm_pending,
            } => {
                let _ = state
                    .db
                    .update_generation_status(&id, "complete", Some(&lol_source), None);
                state.current_lol = Some(lol_source.clone());
                state.phase_progress.retry_count = retry_count;
                state.pending_share_dry_run = share_dry_run;
                state.pending_share_confirm = share_confirm_pending;

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
            GenerationMessage::GalleryLoaded(load_state) => {
                // Gallery Phase 3: background fetch 完了、UI 反映 成功 /
                // 失敗どちらも同 slot に反映 (Error variant で fetch fail
                // を UI に露呈、silent skip はしない)
                match &load_state {
                    crate::state::GalleryLoadState::Loaded(items) => {
                        tracing::info!(count = items.len(), "gallery snapshot loaded");
                    }
                    crate::state::GalleryLoadState::Error(msg) => {
                        tracing::warn!(error = %msg, "gallery snapshot fetch failed");
                    }
                }
                state.gallery = Some(load_state);
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
fn show_prompt_templates(ui: &mut egui::Ui, state: &mut AppState, is_generating: bool, lang: Lang) {
    ui.collapsing(crate::i18n::T::prompt_p049(lang), |ui| {
        ui.add_enabled_ui(!is_generating, |ui| {
            // Snapshot を clone して borrow 期間を短縮 (start_generation_from_lol が
            // state を mutable borrow するため、iterator 中の借用と衝突しないよう分離)
            let snapshot = state.presets.clone();

            // preset source label (bundled / cache / cloud) を version と共に
            // 小さく表示、user が「今どの source を見ているか」認識できる
            let source_label = match snapshot.source {
                crate::state::PresetsSource::Bundled => crate::i18n::T::prompt_p050(lang),
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
                                lang,
                            );
                        }
                    }
                });
                ui.add_space(2.0);
            }
        });
    });
}

/// カスタマイザー = パラメータ入力可能な template (LLM bypass、~1 秒)
///
/// 経路 A (固定 preset button) と経路 B (LLM 自然言語) の中間 slider で
/// param を指定 → 「作成」ボタンで LOL DSL 動的組立て → 生成
/// 現行対応 61 archetype: Gridfinity bin + organizer-gridfinity-desk PART 2 全部
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
/// mix 3 (hairdryer_holder / kcup_holder / hex_key_holder、Sprint 12) +
/// mix2 3 (wrap_holder / sock_divider / soap_tray、Sprint 13) +
/// mix3 3 (razor_holder / chopstick_holder / swatch_holder、Sprint 14) +
/// mix4 3 (tp_holder / sd_card_holder / driver_rack、Sprint 15) +
/// mix5 3 (cotton_dispenser / sink_caddy / clamp_rack、Sprint 16) +
/// mix6 3 (dry_box / outdoor_enclosure / jewelry_stand、Sprint 17) +
/// mix7 3 (phone_dock / cutting_board_rack / tape_dispenser、Sprint 18、multi-component) +
/// mix8 3 (shower_caddy / caliper_holder / bag_clip_org、Sprint 19、multi-component) +
/// mix9 3 (can_rack / led_hub_box / makeup_organizer、Sprint 20、kitchen+electronics 100% 完走)
fn show_prompt_customizer(
    ui: &mut egui::Ui,
    state: &mut AppState,
    is_generating: bool,
    lang: Lang,
) {
    ui.collapsing(crate::i18n::T::prompt_p051(lang), |ui| {
        ui.add_enabled_ui(!is_generating, |ui| {
            show_gridfinity_customizer(ui, state, lang);
            ui.separator();
            show_sticky_note_customizer(ui, state, lang);
            ui.separator();
            show_business_card_customizer(ui, state, lang);
            ui.separator();
            show_pen_cup_customizer(ui, state, lang);
            ui.separator();
            show_phone_stand_customizer(ui, state, lang);
            ui.separator();
            show_headphone_holder_customizer(ui, state, lang);
            ui.separator();
            show_under_desk_mount_customizer(ui, state, lang);
            ui.separator();
            show_desk_shelf_customizer(ui, state, lang);
            ui.separator();
            show_monitor_riser_customizer(ui, state, lang);
            ui.separator();
            show_coaster_customizer(ui, state, lang);
            ui.separator();
            show_tissue_box_cover_customizer(ui, state, lang);
            ui.separator();
            show_storage_box_customizer(ui, state, lang);
            ui.separator();
            show_cable_clip_customizer(ui, state, lang);
            ui.separator();
            show_led_channel_customizer(ui, state, lang);
            ui.separator();
            show_card_tray_customizer(ui, state, lang);
            ui.separator();
            show_token_well_customizer(ui, state, lang);
            ui.separator();
            show_wrench_holder_customizer(ui, state, lang);
            ui.separator();
            show_socket_rail_customizer(ui, state, lang);
            ui.separator();
            show_hex_bit_holder_customizer(ui, state, lang);
            ui.separator();
            show_raspi_case_customizer(ui, state, lang);
            ui.separator();
            show_esp32_enclosure_customizer(ui, state, lang);
            ui.separator();
            show_battery_18650_holder_customizer(ui, state, lang);
            ui.separator();
            show_toothbrush_holder_customizer(ui, state, lang);
            ui.separator();
            show_drill_bit_holder_customizer(ui, state, lang);
            ui.separator();
            show_pliers_rack_customizer(ui, state, lang);
            ui.separator();
            show_spice_rack_customizer(ui, state, lang);
            ui.separator();
            show_egg_tray_customizer(ui, state, lang);
            ui.separator();
            show_utensil_caddy_customizer(ui, state, lang);
            ui.separator();
            show_filament_spool_holder_customizer(ui, state, lang);
            ui.separator();
            show_nozzle_holder_customizer(ui, state, lang);
            ui.separator();
            show_build_plate_rack_customizer(ui, state, lang);
            ui.separator();
            show_cutlery_tray_customizer(ui, state, lang);
            ui.separator();
            show_pill_organizer_customizer(ui, state, lang);
            ui.separator();
            show_magnetic_strip_customizer(ui, state, lang);
            ui.separator();
            show_hairdryer_holder_customizer(ui, state, lang);
            ui.separator();
            show_kcup_holder_customizer(ui, state, lang);
            ui.separator();
            show_hex_key_holder_customizer(ui, state, lang);
            ui.separator();
            show_wrap_holder_customizer(ui, state, lang);
            ui.separator();
            show_sock_divider_customizer(ui, state, lang);
            ui.separator();
            show_soap_tray_customizer(ui, state, lang);
            ui.separator();
            show_razor_holder_customizer(ui, state, lang);
            ui.separator();
            show_chopstick_holder_customizer(ui, state, lang);
            ui.separator();
            show_swatch_holder_customizer(ui, state, lang);
            ui.separator();
            show_tp_holder_customizer(ui, state, lang);
            ui.separator();
            show_sd_card_holder_customizer(ui, state, lang);
            ui.separator();
            show_driver_rack_customizer(ui, state, lang);
            ui.separator();
            show_cotton_dispenser_customizer(ui, state, lang);
            ui.separator();
            show_sink_caddy_customizer(ui, state, lang);
            ui.separator();
            show_clamp_rack_customizer(ui, state, lang);
            ui.separator();
            show_dry_box_customizer(ui, state, lang);
            ui.separator();
            show_outdoor_enclosure_customizer(ui, state, lang);
            ui.separator();
            show_jewelry_stand_customizer(ui, state, lang);
            ui.separator();
            show_phone_dock_customizer(ui, state, lang);
            ui.separator();
            show_cutting_board_rack_customizer(ui, state, lang);
            ui.separator();
            show_tape_dispenser_customizer(ui, state, lang);
            ui.separator();
            show_shower_caddy_customizer(ui, state, lang);
            ui.separator();
            show_caliper_holder_customizer(ui, state, lang);
            ui.separator();
            show_bag_clip_org_customizer(ui, state, lang);
            ui.separator();
            show_can_rack_customizer(ui, state, lang);
            ui.separator();
            show_led_hub_box_customizer(ui, state, lang);
            ui.separator();
            show_makeup_organizer_customizer(ui, state, lang);
            // ── Sprint 21-22 + Multi-domain 16 archetype (2026-08-31、Task A) ──
            ui.separator();
            show_vesa_mount_customizer(ui, state, lang);
            ui.separator();
            show_l_bracket_customizer(ui, state, lang);
            ui.separator();
            show_t_slot_bracket_2020_customizer(ui, state, lang);
            ui.separator();
            show_raspi_mount_plate_customizer(ui, state, lang);
            ui.separator();
            show_heat_set_array_customizer(ui, state, lang);
            ui.separator();
            show_flange_mount_customizer(ui, state, lang);
            ui.separator();
            show_dovetail_pair_customizer(ui, state, lang);
            ui.separator();
            show_profile_extrusion_customizer(ui, state, lang);
            ui.separator();
            show_snap_fit_pair_customizer(ui, state, lang);
            ui.separator();
            show_boss_array_customizer(ui, state, lang);
            ui.separator();
            show_bearing_seat_customizer(ui, state, lang);
            ui.separator();
            show_cable_grommet_customizer(ui, state, lang);
            ui.separator();
            show_curtain_rod_bracket_customizer(ui, state, lang);
            ui.separator();
            show_arduino_mount_plate_customizer(ui, state, lang);
            ui.separator();
            show_pixhawk_mount_customizer(ui, state, lang);
            ui.separator();
            show_servo_mount_customizer(ui, state, lang);
        });
    });
}

/// Gridfinity bin customizer (basic 3 param + advanced 5 param collapsible)
///
/// 42mm grid × 7mm height unit で任意サイズを生成
/// 例: 2×2 × 6U = 84×84×46mm (最典型 default)
/// 詳細設定で dividers (内部仕切り) + 壁厚 + 底厚 も指定可
fn show_gridfinity_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p052(lang)).strong());

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
    ui.collapsing(crate::i18n::T::prompt_p053(lang), |ui| {
        ui.checkbox(&mut g.use_dividers, crate::i18n::T::prompt_p054(lang));
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
            ui.label(crate::i18n::T::prompt_p055(lang));
            ui.add(egui::Slider::new(&mut g.wall_thickness, 0.8..=3.0).step_by(0.1));
        });
        ui.horizontal(|ui| {
            ui.label(crate::i18n::T::prompt_p056(lang));
            ui.add(egui::Slider::new(&mut g.floor_thickness, 1.0..=4.0).step_by(0.1));
        });
    });

    #[allow(clippy::cast_precision_loss)]
    let ext_x_mm = g.units_x as f32 * 42.0;
    #[allow(clippy::cast_precision_loss)]
    let ext_y_mm = g.units_y as f32 * 42.0;
    #[allow(clippy::cast_precision_loss)]
    let ext_h_mm = g.height_u as f32 * 7.0 + 4.75;
    ui.label(crate::i18n::T::prompt_fmt_outer_dim_3f(
        ext_x_mm, ext_y_mm, ext_h_mm, lang,
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
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, g_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// 付箋ホルダー customizer (`pad_w × pad_d × height`)
///
/// Post-it 3×3 inch = 76×76mm、大型 3×5 inch = 76×127mm 等
fn show_sticky_note_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p059(lang)).strong());

    let s = &mut state.customizer_state.sticky_note;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p060(lang));
        ui.add(egui::Slider::new(&mut s.pad_width, 50.0..=150.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p061(lang));
        ui.add(egui::Slider::new(&mut s.pad_depth, 50.0..=150.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p062(lang));
        ui.add(egui::Slider::new(&mut s.height, 15.0..=60.0).step_by(1.0));
    });

    let s_copy = *s;
    let label = crate::i18n::T::prompt_fmt_sticky_note_label(
        s_copy.pad_width,
        s_copy.pad_depth,
        s_copy.height,
        lang,
    );
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, s_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// 名刺ホルダー customizer (`card_w × card_h × slot_thickness`)
///
/// JP meishi 91×55 / US 89×51 / EU 85.6×54、収納枚数 = slot_thickness / 0.5mm 目安
fn show_business_card_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p064(lang)).strong());

    let b = &mut state.customizer_state.business_card;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p065(lang));
        ui.add(egui::Slider::new(&mut b.card_width, 80.0..=100.0).step_by(0.1));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p066(lang));
        ui.add(egui::Slider::new(&mut b.card_height, 45.0..=65.0).step_by(0.1));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p067(lang));
        ui.add(egui::Slider::new(&mut b.slot_thickness, 10.0..=40.0).step_by(1.0));
    });

    // 収納枚数目安 (card 1 枚 ~0.5mm、20% margin)
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let capacity = (b.slot_thickness / 0.5 * 0.8) as u32;
    ui.label(crate::i18n::T::prompt_fmt_capacity_cards(capacity, lang));

    let b_copy = *b;
    let label = crate::i18n::T::prompt_fmt_business_card_label(
        b_copy.card_width,
        b_copy.card_height,
        capacity,
        lang,
    );
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, b_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// ペン立て customizer (`inner_dia × height`)
///
/// standard 70-85mm 内径 × 90-120mm 高、pen 12mm / pencil 8mm / marker 16mm / highlighter 24mm 想定
fn show_pen_cup_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p070(lang)).strong());

    let p = &mut state.customizer_state.pen_cup;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p071(lang));
        ui.add(egui::Slider::new(&mut p.inner_diameter, 40.0..=120.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p062(lang));
        ui.add(egui::Slider::new(&mut p.height, 50.0..=150.0).step_by(1.0));
    });

    let p_copy = *p;
    let outer_dia = p_copy.inner_diameter + 4.0;
    let label =
        crate::i18n::T::prompt_fmt_pen_cup_label(p_copy.inner_diameter, p_copy.height, lang);
    ui.label(crate::i18n::T::prompt_fmt_pen_cup_outer(outer_dia, lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, p_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// スマホ / タブレット スタンド customizer (`slot_w × back_h × cable_dia`)
///
/// phone: slot 10-15mm / back 80-120mm、tablet: slot 12-18mm / back 150-190mm
/// cable_dia = 0 で cable 穴なし
fn show_phone_stand_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p074(lang)).strong());

    let ps = &mut state.customizer_state.phone_stand;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p075(lang));
        ui.add(egui::Slider::new(&mut ps.slot_width, 8.0..=20.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p076(lang));
        ui.add(egui::Slider::new(&mut ps.back_height, 60.0..=200.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p077(lang));
        ui.add(egui::Slider::new(&mut ps.cable_hole_dia, 0.0..=30.0).step_by(1.0));
    });

    let ps_copy = *ps;
    let hole_note = if ps_copy.cable_hole_dia > 0.0 {
        format!("cable Ø{}mm", ps_copy.cable_hole_dia)
    } else {
        crate::i18n::T::prompt_p078(lang).to_string()
    };
    let label = format!(
        "スタンド slot {}mm × back {}mm ({hole_note})",
        ps_copy.slot_width, ps_copy.back_height
    );
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, ps_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// ヘッドホンホルダー customizer (`arm_length × headband_width × mount_width`)
///
/// wall_hook variant で headband 対応、M4 mount 穴付き
fn show_headphone_holder_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p079(lang)).strong());

    let h = &mut state.customizer_state.headphone_holder;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p080(lang));
        ui.add(egui::Slider::new(&mut h.arm_length, 60.0..=120.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p081(lang));
        ui.add(egui::Slider::new(&mut h.headband_width, 30.0..=70.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p082(lang));
        ui.add(egui::Slider::new(&mut h.mount_width, 60.0..=150.0).step_by(1.0));
    });

    let h_copy = *h;
    let label = crate::i18n::T::prompt_fmt_headphone_label(
        h_copy.arm_length,
        h_copy.headband_width,
        h_copy.mount_width,
        lang,
    );
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, h_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// 机下 clamp mount customizer (`desk_thickness × clamp_width × screw_dia`)
///
/// C 字 clamp、screw=0 で穴なし (両面テープ想定)
fn show_under_desk_mount_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p084(lang)).strong());

    let m = &mut state.customizer_state.under_desk_mount;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p085(lang));
        ui.add(egui::Slider::new(&mut m.desk_thickness, 15.0..=60.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p086(lang));
        ui.add(egui::Slider::new(&mut m.clamp_width, 20.0..=80.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p087(lang));
        ui.add(egui::Slider::new(&mut m.screw_hole_dia, 0.0..=8.0).step_by(0.5));
    });

    let m_copy = *m;
    let screw_note = if m_copy.screw_hole_dia > 0.0 {
        format!("M{:.0}", m_copy.screw_hole_dia)
    } else {
        crate::i18n::T::prompt_p088(lang).to_string()
    };
    let label = crate::i18n::T::prompt_fmt_under_desk_label(
        m_copy.desk_thickness,
        m_copy.clamp_width,
        &screw_note,
        lang,
    );
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, m_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// 卓上シェルフ customizer (`shelf_width × shelf_depth × leg_height`)
///
/// 平板 + 左右 2 脚 shelf_divider 簡易版 (hex cutout なし)
fn show_desk_shelf_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p090(lang)).strong());

    let s = &mut state.customizer_state.desk_shelf;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p091(lang));
        ui.add(egui::Slider::new(&mut s.shelf_width, 200.0..=500.0).step_by(10.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p092(lang));
        ui.add(egui::Slider::new(&mut s.shelf_depth, 150.0..=300.0).step_by(10.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p093(lang));
        ui.add(egui::Slider::new(&mut s.leg_height, 60.0..=150.0).step_by(5.0));
    });

    let s_copy = *s;
    let label = crate::i18n::T::prompt_fmt_shelf_label(
        s_copy.shelf_width,
        s_copy.shelf_depth,
        s_copy.leg_height,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p095(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, s_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// モニターライザー customizer (`width × depth × height`)
///
/// 簡易版 = 単一プリント想定、Ø40mm cable hole 付き
fn show_monitor_riser_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p096(lang)).strong());

    let r = &mut state.customizer_state.monitor_riser;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p097(lang));
        ui.add(egui::Slider::new(&mut r.width, 200.0..=280.0).step_by(10.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p098(lang));
        ui.add(egui::Slider::new(&mut r.depth, 150.0..=240.0).step_by(10.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p062(lang));
        ui.add(egui::Slider::new(&mut r.height, 60.0..=120.0).step_by(5.0));
    });

    let r_copy = *r;
    let label = crate::i18n::T::prompt_fmt_monitor_riser_label(
        r_copy.width,
        r_copy.depth,
        r_copy.height,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p100(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, r_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// コースター customizer (`diameter × thickness`)
///
/// round bowl 状、rim 2.5mm 幅 × 1.5mm 高 で液滴 catch (household § 7)
fn show_coaster_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p101(lang)).strong());

    let c = &mut state.customizer_state.coaster;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p102(lang));
        ui.add(egui::Slider::new(&mut c.diameter, 80.0..=110.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p103(lang));
        ui.add(egui::Slider::new(&mut c.thickness, 4.0..=8.0).step_by(0.5));
    });

    let c_copy = *c;
    let label = crate::i18n::T::prompt_fmt_coaster_label(c_copy.diameter, c_copy.thickness, lang);
    ui.label(crate::i18n::T::prompt_p105(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, c_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// ティッシュボックスカバー customizer (`internal_l × internal_w × internal_h`)
///
/// bottom open + top pull slot (80×30mm 標準)、内部寸法指定 (household § 1)
fn show_tissue_box_cover_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p106(lang)).strong());

    let t = &mut state.customizer_state.tissue_box_cover;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p107(lang));
        ui.add(egui::Slider::new(&mut t.internal_length, 100.0..=280.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p108(lang));
        ui.add(egui::Slider::new(&mut t.internal_width, 100.0..=200.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p109(lang));
        ui.add(egui::Slider::new(&mut t.internal_height, 40.0..=140.0).step_by(1.0));
    });

    let t_copy = *t;
    let label = crate::i18n::T::prompt_fmt_tissue_label(
        t_copy.internal_length,
        t_copy.internal_width,
        t_copy.internal_height,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p111(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, t_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// 収納 BOX customizer (`internal_l × internal_w × internal_h`)
///
/// top open 基本形、lid + hinge は future sprint (household § 3)
fn show_storage_box_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p112(lang)).strong());

    let s = &mut state.customizer_state.storage_box;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p107(lang));
        ui.add(egui::Slider::new(&mut s.internal_length, 60.0..=250.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p108(lang));
        ui.add(egui::Slider::new(&mut s.internal_width, 60.0..=200.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p109(lang));
        ui.add(egui::Slider::new(&mut s.internal_height, 30.0..=120.0).step_by(5.0));
    });

    let s_copy = *s;
    let label = crate::i18n::T::prompt_fmt_storage_box_label(
        s_copy.internal_length,
        s_copy.internal_width,
        s_copy.internal_height,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p114(lang));
    ui.label(crate::i18n::T::prompt_p115(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, s_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// ケーブルクリップ customizer (`cable_diameter × clip_length`、hobby-diy § 2)
///
/// Y-axis 沿い cable、+Z 開口 snap-fit (opening ratio 0.7 = 30% 狭い)
/// USB-A 3.5 / USB-C 4.5 / Ethernet 6 / HDMI 7 / Power 8-10 mm 想定
fn show_cable_clip_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p116(lang)).strong());

    let c = &mut state.customizer_state.cable_clip;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p117(lang));
        ui.add(egui::Slider::new(&mut c.cable_diameter, 3.0..=12.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p118(lang));
        ui.add(egui::Slider::new(&mut c.clip_length, 15.0..=60.0).step_by(1.0));
    });

    let c_copy = *c;
    let label = crate::i18n::T::prompt_fmt_cable_clip_label(
        c_copy.cable_diameter,
        c_copy.clip_length,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p120(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, c_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// LED strip channel customizer (`strip_width × channel_length`、hobby-diy § 3)
///
/// Y-axis 沿い strip、+Z 開口 U 溝 (深さ 2.5mm 固定、壁厚 2.0mm)
/// SMD3528 8mm / WS2812B 10-12mm PCB 対応
fn show_led_channel_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p121(lang)).strong());

    let l = &mut state.customizer_state.led_channel;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p122(lang));
        ui.add(egui::Slider::new(&mut l.strip_width, 6.0..=20.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p123(lang));
        ui.add(egui::Slider::new(&mut l.channel_length, 50.0..=1000.0).step_by(10.0));
    });

    let l_copy = *l;
    let label = format!(
        "LED channel {}mm × {}mm",
        l_copy.strip_width, l_copy.channel_length
    );
    ui.label(crate::i18n::T::prompt_p124(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, l_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// カードトレー customizer (`card_w × card_h × depth`、hobby-diy § 6)
///
/// top 開口 + front edge finger 半円 notch (r=9mm 固定)
/// Poker 63×88 / Mini Euro 44×68 / Standard Euro 59×92 / Tarot 70×120 対応
fn show_card_tray_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p125(lang)).strong());

    let t = &mut state.customizer_state.card_tray;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p126(lang));
        ui.add(egui::Slider::new(&mut t.card_width, 30.0..=80.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p127(lang));
        ui.add(egui::Slider::new(&mut t.card_height, 50.0..=130.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p128(lang));
        ui.add(egui::Slider::new(&mut t.tray_depth, 10.0..=60.0).step_by(1.0));
    });

    let t_copy = *t;
    let label = crate::i18n::T::prompt_fmt_card_tray_label(
        t_copy.card_width,
        t_copy.card_height,
        t_copy.tray_depth,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p130(lang));
    ui.label(crate::i18n::T::prompt_p131(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, t_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// トークン井戸 customizer (`dia × depth × count`、hobby-diy § 6)
///
/// row 状に count 個の円筒 well、top 開口、印刷正立
/// shallow token 10-15 / dice/meeples 20-25 / miniatures 30-40 mm 深さ目安
fn show_token_well_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p132(lang)).strong());

    let w = &mut state.customizer_state.token_well;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p133(lang));
        ui.add(egui::Slider::new(&mut w.well_diameter, 8.0..=40.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p134(lang));
        ui.add(egui::Slider::new(&mut w.well_depth, 5.0..=50.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p135(lang));
        ui.add(egui::Slider::new(&mut w.well_count, 1..=10).text("(1-10)"));
    });

    let w_copy = *w;
    let label = crate::i18n::T::prompt_fmt_token_well_label(
        w_copy.well_diameter,
        w_copy.well_depth,
        w_copy.well_count,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p137(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, w_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// レンチホルダー customizer (`min_mm × max_mm × count`、tools § 1)
///
/// min-max mm を count 個 等間隔補間 (例: 8, 10, 12, 14, 16, 18)
/// Metric 標準 8-19 (6 slot) / 8-24 (8 slot) / SAE 1/4"-1" 相当は 6.35-25.4mm
fn show_wrench_holder_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p138(lang)).strong());

    let w = &mut state.customizer_state.wrench_holder;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p139(lang));
        ui.add(egui::Slider::new(&mut w.min_size_mm, 6.0..=22.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p140(lang));
        ui.add(egui::Slider::new(&mut w.max_size_mm, 8.0..=32.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p141(lang));
        ui.add(egui::Slider::new(&mut w.count, 3..=12).text("(3-12)"));
    });

    let w_copy = *w;
    let label = crate::i18n::T::prompt_fmt_wrench_label(
        w_copy.min_size_mm,
        w_copy.max_size_mm,
        w_copy.count,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p143(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, w_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// ソケットレール customizer (`post_dia × post_height × count`、tools § 2)
///
/// base plate 上に post を row 配置 1/4"=6.0 / 3/8"=9.2 / 1/2"=12.4 / 3/4"=18.7
fn show_socket_rail_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p144(lang)).strong());

    let s = &mut state.customizer_state.socket_rail;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p145(lang));
        ui.add(egui::Slider::new(&mut s.post_diameter, 5.0..=25.0).step_by(0.1));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p146(lang));
        ui.add(egui::Slider::new(&mut s.post_height, 12.0..=30.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p147(lang));
        ui.add(egui::Slider::new(&mut s.post_count, 3..=15).text("(3-15)"));
    });

    let s_copy = *s;
    let label = crate::i18n::T::prompt_fmt_socket_rail_label(
        s_copy.post_diameter,
        s_copy.post_height,
        s_copy.post_count,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p149(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, s_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// ヘックスビットホルダー customizer (`rows × cols × spacing`、tools § 3)
///
/// grid 状 hex hole、1/4" bit 想定 (across-flats 6.85mm × depth 14mm 固定)
fn show_hex_bit_holder_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_hex_bit_holder_header(lang)).strong());

    let h = &mut state.customizer_state.hex_bit_holder;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p151(lang));
        ui.add(egui::Slider::new(&mut h.rows, 1..=10).text("(1-10)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p152(lang));
        ui.add(egui::Slider::new(&mut h.cols, 1..=10).text("(1-10)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p153(lang));
        ui.add(egui::Slider::new(&mut h.spacing, 10.0..=20.0).step_by(0.5));
    });

    let h_copy = *h;
    let label =
        crate::i18n::T::prompt_fmt_bit_holder_label(h_copy.rows, h_copy.cols, h_copy.spacing, lang);
    ui.label(crate::i18n::T::prompt_p155(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, h_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// Raspberry Pi ケース customizer (`pcb_w × pcb_d × internal_h`、electronics § 1)
///
/// 4 corner standoff peg (M2.5 pilot) + 長辺 port opening (60mm 幅) + top open
/// Default: RPi 5 with Active Cooler (85×56×25mm) / bare Pi (h=15) / Zero 2W (65×30×15)
fn show_raspi_case_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p156(lang)).strong());

    let c = &mut state.customizer_state.raspi_case;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p157(lang));
        ui.add(egui::Slider::new(&mut c.pcb_width, 40.0..=120.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p158(lang));
        ui.add(egui::Slider::new(&mut c.pcb_depth, 20.0..=80.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p159(lang));
        ui.add(egui::Slider::new(&mut c.internal_height, 10.0..=40.0).step_by(1.0));
    });

    let c_copy = *c;
    let label = crate::i18n::T::prompt_fmt_rpi_case_label(
        c_copy.pcb_width,
        c_copy.pcb_depth,
        c_copy.internal_height,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p161(lang));
    ui.label(crate::i18n::T::prompt_p162(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, c_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// ESP32/Arduino エンクロージャ customizer (`pcb_w × pcb_d × internal_h`、electronics § 2)
///
/// standoff なし friction cradle + 短辺 USB opening (9×5mm) + top open
/// Default: ESP32 DevKit V1 (51.6×28.4×15) / Arduino Uno R3 (68.6×53.4×20) / Nano (45×18×12)
fn show_esp32_enclosure_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p163(lang)).strong());

    let e = &mut state.customizer_state.esp32_enclosure;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p157(lang));
        ui.add(egui::Slider::new(&mut e.pcb_width, 30.0..=100.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p158(lang));
        ui.add(egui::Slider::new(&mut e.pcb_depth, 15.0..=80.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p159(lang));
        ui.add(egui::Slider::new(&mut e.internal_height, 8.0..=30.0).step_by(1.0));
    });

    let e_copy = *e;
    let label = crate::i18n::T::prompt_fmt_mcu_case_label(
        e_copy.pcb_width,
        e_copy.pcb_depth,
        e_copy.internal_height,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p165(lang));
    ui.label(crate::i18n::T::prompt_p166(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, e_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// 18650 バッテリーホルダー customizer (`count × wall × floor`、electronics § 3)
///
/// row 状 cylindrical cavity (Ø18.6mm × L68mm 固定)
/// floor=0 なら両端貫通 (cell 挿入 open)、>0 なら片端閉塞 (spring 保持)
fn show_battery_18650_holder_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p167(lang)).strong());

    let b = &mut state.customizer_state.battery_18650_holder;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p168(lang));
        ui.add(egui::Slider::new(&mut b.cell_count, 1..=10).text("(1-10)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p169(lang));
        ui.add(egui::Slider::new(&mut b.wall_thickness, 2.0..=4.0).step_by(0.1));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p170(lang));
        ui.add(egui::Slider::new(&mut b.floor_thickness, 0.0..=5.0).step_by(0.5));
    });

    let b_copy = *b;
    let label = format!(
        "18650 × {} (wall {}mm, floor {}mm)",
        b_copy.cell_count, b_copy.wall_thickness, b_copy.floor_thickness
    );
    ui.label(crate::i18n::T::prompt_p171(lang));
    ui.label(crate::i18n::T::prompt_p172(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, b_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// 歯ブラシホルダー customizer (`count × hole_diameter × height`、bathroom § 7.1)
///
/// row 状 cylindrical hole、top 開口 (Ø15 manual / Ø40 electric)
/// 素材は PETG 推奨 (moisture resistance)
fn show_toothbrush_holder_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p173(lang)).strong());

    let t = &mut state.customizer_state.toothbrush_holder;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p174(lang));
        ui.add(egui::Slider::new(&mut t.count, 1..=8).text("(1-8)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p175(lang));
        ui.add(egui::Slider::new(&mut t.hole_diameter, 10.0..=45.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p176(lang));
        ui.add(egui::Slider::new(&mut t.hole_depth, 50.0..=120.0).step_by(1.0));
    });

    let t_copy = *t;
    let label = crate::i18n::T::prompt_fmt_toothbrush_label(
        t_copy.count,
        t_copy.hole_diameter,
        t_copy.hole_depth,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p178(lang));
    ui.label(crate::i18n::T::prompt_p179(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, t_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// ドリルビットホルダー customizer (`min_mm × max_mm × count`、garage § 8.1)
///
/// row 状 hole、size linear interpolate (wrench_holder の hole 円形版)
fn show_drill_bit_holder_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p180(lang)).strong());

    let d = &mut state.customizer_state.drill_bit_holder;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p181(lang));
        ui.add(egui::Slider::new(&mut d.min_size_mm, 1.0..=8.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p182(lang));
        ui.add(egui::Slider::new(&mut d.max_size_mm, 5.0..=20.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p174(lang));
        ui.add(egui::Slider::new(&mut d.count, 5..=25).text("(5-25)"));
    });

    let d_copy = *d;
    let label = format!(
        "ドリルビット {}-{}mm × {}",
        d_copy.min_size_mm, d_copy.max_size_mm, d_copy.count
    );
    ui.label(crate::i18n::T::prompt_p183(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, d_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// プライヤーラック customizer (`slot_count × slot_width × slot_depth`、garage § 8.4)
///
/// row 状 rect slot、top 開口、pliers 挿入
fn show_pliers_rack_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p184(lang)).strong());

    let p = &mut state.customizer_state.pliers_rack;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p141(lang));
        ui.add(egui::Slider::new(&mut p.slot_count, 3..=12).text("(3-12)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p075(lang));
        ui.add(egui::Slider::new(&mut p.slot_width, 8.0..=30.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p185(lang));
        ui.add(egui::Slider::new(&mut p.slot_depth, 40.0..=90.0).step_by(1.0));
    });

    let p_copy = *p;
    let label = crate::i18n::T::prompt_fmt_pliers_label(
        p_copy.slot_count,
        p_copy.slot_width,
        p_copy.slot_depth,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p187(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, p_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// スパイスラック customizer (`count × jar_diameter × jar_height`、kitchen § 6.1)
///
/// 薄 shelf + jar 用 shallow recess (5mm) + 前縁 lip (jar_height × 15%)
fn show_spice_rack_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new("🧂 スパイスラック (shelf + jar recess + lip)").strong());

    let s = &mut state.customizer_state.spice_rack;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p188(lang));
        ui.add(egui::Slider::new(&mut s.count, 3..=12).text("(3-12)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p189(lang));
        ui.add(egui::Slider::new(&mut s.jar_diameter, 40.0..=55.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p190(lang));
        ui.add(egui::Slider::new(&mut s.jar_height, 70.0..=130.0).step_by(1.0));
    });

    let s_copy = *s;
    let label = format!(
        "スパイスラック {} jar × Ø{}×H{}mm",
        s_copy.count, s_copy.jar_diameter, s_copy.jar_height
    );
    ui.label(crate::i18n::T::prompt_p191(lang));
    ui.label(crate::i18n::T::prompt_p192(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, s_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// 卵トレー customizer (`rows × cols × cup_depth`、kitchen § 6.5)
///
/// 2D grid 状 cup、egg cup Ø40mm 固定、pitch 50mm 固定
fn show_egg_tray_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p193(lang)).strong());

    let e = &mut state.customizer_state.egg_tray;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p151(lang));
        ui.add(egui::Slider::new(&mut e.rows, 1..=8).text("(1-8)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p152(lang));
        ui.add(egui::Slider::new(&mut e.cols, 1..=8).text("(1-8)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p194(lang));
        ui.add(egui::Slider::new(&mut e.cup_depth, 12.0..=25.0).step_by(0.5));
    });

    let e_copy = *e;
    let label =
        crate::i18n::T::prompt_fmt_egg_tray_label(e_copy.rows, e_copy.cols, e_copy.cup_depth, lang);
    ui.label(crate::i18n::T::prompt_p196(lang));
    ui.label(crate::i18n::T::prompt_p197(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, e_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// キッチンツールキャディ customizer (`count × compartment_dia × height`、kitchen § 6.8)
///
/// row 状 large cylindrical compartment (spatula / ladle / whisk / tongs 分別)
fn show_utensil_caddy_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p198(lang)).strong());

    let u = &mut state.customizer_state.utensil_caddy;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p199(lang));
        ui.add(egui::Slider::new(&mut u.count, 1..=6).text("(1-6)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p200(lang));
        ui.add(egui::Slider::new(&mut u.compartment_diameter, 45.0..=80.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p201(lang));
        ui.add(egui::Slider::new(&mut u.height, 100.0..=180.0).step_by(5.0));
    });

    let u_copy = *u;
    let label = crate::i18n::T::prompt_fmt_utensil_caddy_label(
        u_copy.count,
        u_copy.compartment_diameter,
        u_copy.height,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p203(lang));
    ui.label(crate::i18n::T::prompt_p204(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, u_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// フィラメントスプールホルダー customizer
/// (`spool_od × spool_width × bore_dia`、printer § 9.1)
///
/// base plate + 垂直 peg (spool bore over peg、donut on pole style)
fn show_filament_spool_holder_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p205(lang)).strong());

    let f = &mut state.customizer_state.filament_spool_holder;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p206(lang));
        ui.add(egui::Slider::new(&mut f.spool_outer_diameter, 100.0..=300.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p207(lang));
        ui.add(egui::Slider::new(&mut f.spool_width, 30.0..=120.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p208(lang));
        ui.add(egui::Slider::new(&mut f.bore_diameter, 30.0..=100.0).step_by(1.0));
    });

    let f_copy = *f;
    let label = crate::i18n::T::prompt_fmt_spool_label(
        f_copy.spool_outer_diameter,
        f_copy.spool_width,
        f_copy.bore_diameter,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p210(lang));
    ui.label(crate::i18n::T::prompt_p211(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, f_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// ノズルホルダー customizer (`count × hole_diameter × depth`、printer § 9.5)
///
/// row 状 small hole for M6 nozzles (E3D V6 / Bambu M6)
fn show_nozzle_holder_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p212(lang)).strong());

    let n = &mut state.customizer_state.nozzle_holder;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p174(lang));
        ui.add(egui::Slider::new(&mut n.count, 3..=15).text("(3-15)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p175(lang));
        ui.add(egui::Slider::new(&mut n.hole_diameter, 6.0..=15.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p176(lang));
        ui.add(egui::Slider::new(&mut n.hole_depth, 4.0..=15.0).step_by(0.5));
    });

    let n_copy = *n;
    let label = crate::i18n::T::prompt_fmt_nozzle_label(
        n_copy.count,
        n_copy.hole_diameter,
        n_copy.hole_depth,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p214(lang));
    ui.label(crate::i18n::T::prompt_p215(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, n_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// ビルドプレートラック customizer
/// (`slot_count × slot_spacing × height`、printer § 9.6)
///
/// row 状 vertical slot for 5mm-thick build plates
fn show_build_plate_rack_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p216(lang)).strong());

    let r = &mut state.customizer_state.build_plate_rack;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p141(lang));
        ui.add(egui::Slider::new(&mut r.slot_count, 2..=10).text("(2-10)"));
    });
    ui.horizontal(|ui| {
        ui.label("slot spacing (mm):");
        ui.add(egui::Slider::new(&mut r.slot_spacing, 12.0..=25.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p217(lang));
        ui.add(egui::Slider::new(&mut r.height, 150.0..=350.0).step_by(5.0));
    });

    let r_copy = *r;
    let label = crate::i18n::T::prompt_fmt_plate_rack_label(
        r_copy.slot_count,
        r_copy.slot_spacing,
        r_copy.height,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p219(lang));
    ui.label(crate::i18n::T::prompt_p220(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, r_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// カトラリートレー customizer (`slot_count × slot_width × slot_length`、drawer § 3.2)
///
/// row 状 long rect slot (fork/knife/spoon 分別、drawer 引き出し向け)
fn show_cutlery_tray_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p221(lang)).strong());

    let c = &mut state.customizer_state.cutlery_tray;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p141(lang));
        ui.add(egui::Slider::new(&mut c.slot_count, 2..=8).text("(2-8)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p075(lang));
        ui.add(egui::Slider::new(&mut c.slot_width, 20.0..=60.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p222(lang));
        ui.add(egui::Slider::new(&mut c.slot_length, 150.0..=350.0).step_by(5.0));
    });

    let c_copy = *c;
    let label = crate::i18n::T::prompt_fmt_cutlery_label(
        c_copy.slot_count,
        c_copy.slot_width,
        c_copy.slot_length,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p224(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, c_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// 薬箱 customizer (`rows × cols × cell_size`、drawer § 3.6)
///
/// 2D grid rect cells (weekly pill box、egg_tray の rect 版)
fn show_pill_organizer_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p225(lang)).strong());

    let p = &mut state.customizer_state.pill_organizer;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p151(lang));
        ui.add(egui::Slider::new(&mut p.rows, 1..=14).text("(1-14)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p152(lang));
        ui.add(egui::Slider::new(&mut p.cols, 1..=8).text("(1-8)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p226(lang));
        ui.add(egui::Slider::new(&mut p.cell_size, 15.0..=30.0).step_by(0.5));
    });

    let p_copy = *p;
    let label =
        crate::i18n::T::prompt_fmt_pill_label(p_copy.rows, p_copy.cols, p_copy.cell_size, lang);
    ui.label(crate::i18n::T::prompt_p228(lang));
    ui.label(crate::i18n::T::prompt_p229(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, p_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// マグネットストリップ customizer
/// (`magnet_count × magnet_diameter × spacing`、wall § 4.6)
///
/// long thin bar + row of magnet holes (kitchen knife rail / tool retention)
fn show_magnetic_strip_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new("🧲 マグネットストリップ (long thin bar + magnet)").strong());

    let m = &mut state.customizer_state.magnetic_strip;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p230(lang));
        ui.add(egui::Slider::new(&mut m.magnet_count, 3..=15).text("(3-15)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p231(lang));
        ui.add(egui::Slider::new(&mut m.magnet_diameter, 4.0..=15.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label("magnet spacing (mm):");
        ui.add(egui::Slider::new(&mut m.magnet_spacing, 20.0..=60.0).step_by(1.0));
    });

    let m_copy = *m;
    let label = crate::i18n::T::prompt_fmt_magnetic_label(
        m_copy.magnet_count,
        m_copy.magnet_diameter,
        m_copy.magnet_spacing,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p233(lang));
    ui.label(crate::i18n::T::prompt_p234(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, m_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// ヘアドライヤーホルダー customizer
/// (`barrel_diameter × holster_depth × wall_thickness`、bathroom § 7.7)
///
/// 大径 cylindrical holster (Dyson Supersonic / 汎用ドライヤー対応)
fn show_hairdryer_holder_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p235(lang)).strong());

    let h = &mut state.customizer_state.hairdryer_holder;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p236(lang));
        ui.add(egui::Slider::new(&mut h.barrel_diameter, 40.0..=120.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p237(lang));
        ui.add(egui::Slider::new(&mut h.holster_depth, 80.0..=150.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p055(lang));
        ui.add(egui::Slider::new(&mut h.wall_thickness, 2.0..=6.0).step_by(0.5));
    });

    let h_copy = *h;
    let label = crate::i18n::T::prompt_fmt_hairdryer_label(
        h_copy.barrel_diameter,
        h_copy.holster_depth,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p239(lang));
    ui.label(crate::i18n::T::prompt_p240(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, h_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// K-Cup ホルダー customizer (`rows × cols × capsule_diameter`、kitchen § 6.7)
///
/// 2D grid K-Cup wells (K-Cup Ø53 / Nespresso Ø39 / Dolce Gusto Ø55 対応)
fn show_kcup_holder_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p241(lang)).strong());

    let k = &mut state.customizer_state.kcup_holder;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p151(lang));
        ui.add(egui::Slider::new(&mut k.rows, 1..=6).text("(1-6)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p152(lang));
        ui.add(egui::Slider::new(&mut k.cols, 1..=6).text("(1-6)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p242(lang));
        ui.add(egui::Slider::new(&mut k.capsule_diameter, 35.0..=60.0).step_by(1.0));
    });

    let k_copy = *k;
    let label = crate::i18n::T::prompt_fmt_kcup_label(
        k_copy.rows,
        k_copy.cols,
        k_copy.capsule_diameter,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p244(lang));
    ui.label(crate::i18n::T::prompt_p245(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, k_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// ヘックスキーホルダー customizer
/// (`count × min_key_mm × max_key_mm`、garage § 8.2)
///
/// row 状 hole linear interpolate (Metric 9-piece / SAE 12-piece、drill_bit pattern)
fn show_hex_key_holder_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p246(lang)).strong());

    let h = &mut state.customizer_state.hex_key_holder;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p247(lang));
        ui.add(egui::Slider::new(&mut h.count, 5..=15).text("(5-15)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p248(lang));
        ui.add(egui::Slider::new(&mut h.min_key_mm, 1.0..=4.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p249(lang));
        ui.add(egui::Slider::new(&mut h.max_key_mm, 6.0..=15.0).step_by(0.5));
    });

    let h_copy = *h;
    let label = crate::i18n::T::prompt_fmt_hexkey_label(
        h_copy.min_key_mm,
        h_copy.max_key_mm,
        h_copy.count,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p251(lang));
    ui.label(crate::i18n::T::prompt_p252(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, h_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// wrap/foil ロールホルダー customizer
/// (`roll_diameter × roll_width × wall_thickness`、kitchen § 6.2)
///
/// 長 body + 上端 半円 cradle (roll が 60% 埋め込む形)
fn show_wrap_holder_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p253(lang)).strong());

    let w = &mut state.customizer_state.wrap_holder;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p254(lang));
        ui.add(egui::Slider::new(&mut w.roll_diameter, 40.0..=65.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p255(lang));
        ui.add(egui::Slider::new(&mut w.roll_width, 200.0..=460.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p055(lang));
        ui.add(egui::Slider::new(&mut w.wall_thickness, 2.0..=5.0).step_by(0.5));
    });

    let w_copy = *w;
    let label =
        crate::i18n::T::prompt_fmt_wrap_label(w_copy.roll_diameter, w_copy.roll_width, lang);
    ui.label(crate::i18n::T::prompt_p257(lang));
    ui.label(crate::i18n::T::prompt_p258(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, w_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// 靴下 divider customizer (`cell_count × cell_width × height`、drawer § 3.7)
///
/// 外周 frame + (count-1) 内部 partition walls
fn show_sock_divider_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p259(lang)).strong());

    let d = &mut state.customizer_state.sock_divider;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p168(lang));
        ui.add(egui::Slider::new(&mut d.cell_count, 2..=10).text("(2-10)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p260(lang));
        ui.add(egui::Slider::new(&mut d.cell_width, 50.0..=150.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label("height (mm):");
        ui.add(egui::Slider::new(&mut d.height, 50.0..=120.0).step_by(1.0));
    });

    let d_copy = *d;
    let label = crate::i18n::T::prompt_fmt_sock_label(
        d_copy.cell_count,
        d_copy.cell_width,
        d_copy.height,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p262(lang));
    ui.label(crate::i18n::T::prompt_p263(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, d_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// 石鹸トレー customizer
/// (`tray_length × tray_width × drain_slot_count`、bathroom § 7.3)
///
/// rect tray + 底面 drain slots
fn show_soap_tray_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p264(lang)).strong());

    let s = &mut state.customizer_state.soap_tray;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p265(lang));
        ui.add(egui::Slider::new(&mut s.tray_length, 100.0..=300.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p266(lang));
        ui.add(egui::Slider::new(&mut s.tray_width, 60.0..=150.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p267(lang));
        ui.add(egui::Slider::new(&mut s.drain_slot_count, 2..=15).text("(2-15)"));
    });

    let s_copy = *s;
    let label = crate::i18n::T::prompt_fmt_soap_label(
        s_copy.tray_length,
        s_copy.tray_width,
        s_copy.drain_slot_count,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p269(lang));
    ui.label(crate::i18n::T::prompt_p270(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, s_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// カミソリホルダー customizer
/// (`slot_width × slot_depth × mount_hole_diameter`、bathroom § 7.2)
///
/// wall-mount narrow slot + M4 mount hole (Mach3/Fusion cartridge razor 対応)
fn show_razor_holder_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p271(lang)).strong());

    let r = &mut state.customizer_state.razor_holder;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p075(lang));
        ui.add(egui::Slider::new(&mut r.slot_width, 8.0..=16.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p185(lang));
        ui.add(egui::Slider::new(&mut r.slot_depth, 15.0..=30.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p272(lang));
        ui.add(egui::Slider::new(&mut r.mount_hole_diameter, 3.0..=6.0).step_by(0.5));
    });

    let r_copy = *r;
    let label = crate::i18n::T::prompt_fmt_razor_label(
        r_copy.slot_width,
        r_copy.slot_depth,
        r_copy.mount_hole_diameter,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p274(lang));
    ui.label(crate::i18n::T::prompt_p275(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, r_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// 箸ホルダー customizer (`pair_count × slot_width × slot_length`、drawer § 3.3)
///
/// row 状 narrow long slots (cutlery_tray より narrow、adult chopsticks 260mm)
fn show_chopstick_holder_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p276(lang)).strong());

    let c = &mut state.customizer_state.chopstick_holder;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p277(lang));
        ui.add(egui::Slider::new(&mut c.pair_count, 2..=10).text("(2-10)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p075(lang));
        ui.add(egui::Slider::new(&mut c.slot_width, 8.0..=20.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p222(lang));
        ui.add(egui::Slider::new(&mut c.slot_length, 200.0..=330.0).step_by(5.0));
    });

    let c_copy = *c;
    let label = crate::i18n::T::prompt_fmt_chopstick_label(
        c_copy.pair_count,
        c_copy.slot_width,
        c_copy.slot_length,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p279(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, c_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// フィラメントスウォッチホルダー customizer
/// (`rows × cols × swatch_width`、printer § 9.7)
///
/// 2D grid narrow rect slots for filament sample cards
fn show_swatch_holder_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p280(lang)).strong());

    let s = &mut state.customizer_state.swatch_holder;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p151(lang));
        ui.add(egui::Slider::new(&mut s.rows, 2..=20).text("(2-20)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p152(lang));
        ui.add(egui::Slider::new(&mut s.cols, 1..=10).text("(1-10)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p281(lang));
        ui.add(egui::Slider::new(&mut s.swatch_width, 20.0..=60.0).step_by(1.0));
    });

    let s_copy = *s;
    let label = crate::i18n::T::prompt_fmt_swatch_label(
        s_copy.rows,
        s_copy.cols,
        s_copy.swatch_width,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p283(lang));
    ui.label(crate::i18n::T::prompt_p284(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, s_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// トイレットペーパーホルダー customizer
/// (`inner_diameter × roll_width × wall_thickness`、bathroom § 7.6)
///
/// Wall-mount backplate + Z-axis axle + M4 mount holes
fn show_tp_holder_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p285(lang)).strong());

    let t = &mut state.customizer_state.tp_holder;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p286(lang));
        ui.add(egui::Slider::new(&mut t.inner_diameter, 35.0..=50.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p287(lang));
        ui.add(egui::Slider::new(&mut t.roll_width, 90.0..=150.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p288(lang));
        ui.add(egui::Slider::new(&mut t.wall_thickness, 3.0..=10.0).step_by(0.5));
    });

    let t_copy = *t;
    let label = crate::i18n::T::prompt_fmt_tp_label(
        t_copy.inner_diameter,
        t_copy.roll_width,
        t_copy.wall_thickness,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p290(lang));
    ui.label(crate::i18n::T::prompt_p291(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, t_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// SD カードホルダー customizer
/// (`rows × cols × card_width`、printer § 9.4)
///
/// 2D grid narrow rect slots for SD/microSD cards
fn show_sd_card_holder_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p292(lang)).strong());

    let s = &mut state.customizer_state.sd_card_holder;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p151(lang));
        ui.add(egui::Slider::new(&mut s.rows, 2..=8).text("(2-8)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p152(lang));
        ui.add(egui::Slider::new(&mut s.cols, 2..=8).text("(2-8)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p126(lang));
        ui.add(egui::Slider::new(&mut s.card_width, 12.0..=30.0).step_by(1.0));
    });

    let s_copy = *s;
    let label =
        crate::i18n::T::prompt_fmt_sd_label(s_copy.rows, s_copy.cols, s_copy.card_width, lang);
    ui.label(crate::i18n::T::prompt_p294(lang));
    ui.label(crate::i18n::T::prompt_p295(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, s_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// ドライバーラック customizer
/// (`slot_count × slot_diameter × height`、garage § 8.5)
///
/// Row 状 large cyl hole for screwdriver handles
fn show_driver_rack_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p296(lang)).strong());

    let d = &mut state.customizer_state.driver_rack;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p141(lang));
        ui.add(egui::Slider::new(&mut d.slot_count, 4..=16).text("(4-16)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p297(lang));
        ui.add(egui::Slider::new(&mut d.slot_diameter, 15.0..=40.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p298(lang));
        ui.add(egui::Slider::new(&mut d.height, 60.0..=150.0).step_by(5.0));
    });

    let d_copy = *d;
    let label = crate::i18n::T::prompt_fmt_driver_label(
        d_copy.slot_count,
        d_copy.slot_diameter,
        d_copy.height,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p300(lang));
    ui.label(crate::i18n::T::prompt_p301(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, d_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// 綿棒/コットン ディスペンサー customizer
/// (`count × inner_diameter × height`、bathroom § 7.4)
///
/// Open top cyl + inner cavity (pen_cup pattern の large version)
fn show_cotton_dispenser_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p302(lang)).strong());

    let c = &mut state.customizer_state.cotton_dispenser;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p303(lang));
        ui.add(egui::Slider::new(&mut c.count, 20..=200).text("(20-200)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p304(lang));
        ui.add(egui::Slider::new(&mut c.inner_diameter, 60.0..=120.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p305(lang));
        ui.add(egui::Slider::new(&mut c.height, 60.0..=150.0).step_by(5.0));
    });

    let c_copy = *c;
    let label = crate::i18n::T::prompt_fmt_cotton_label(
        c_copy.count,
        c_copy.inner_diameter,
        c_copy.height,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p307(lang));
    ui.label(crate::i18n::T::prompt_p308(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, c_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// スポンジホルダー customizer
/// (`tray_length × tray_width × drain_hole_count`、kitchen § 6.9)
///
/// Rect tray + Y-axis drain cyl holes (soap_tray pattern の kitchen scaled 版)
fn show_sink_caddy_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p309(lang)).strong());

    let s = &mut state.customizer_state.sink_caddy;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p310(lang));
        ui.add(egui::Slider::new(&mut s.tray_length, 150.0..=300.0).step_by(10.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p311(lang));
        ui.add(egui::Slider::new(&mut s.tray_width, 80.0..=150.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p312(lang));
        ui.add(egui::Slider::new(&mut s.drain_hole_count, 4..=16).text("(4-16)"));
    });

    let s_copy = *s;
    let label = crate::i18n::T::prompt_fmt_sponge_label(
        s_copy.tray_length,
        s_copy.tray_width,
        s_copy.drain_hole_count,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p314(lang));
    ui.label(crate::i18n::T::prompt_p315(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, s_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// クランプ壁掛けラック customizer
/// (`hook_count × hook_width × height`、garage § 8.8)
///
/// Row 状 hook + backplate + M4 mount holes (wall_hook の row 状拡張)
fn show_clamp_rack_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p316(lang)).strong());

    let c = &mut state.customizer_state.clamp_rack;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p317(lang));
        ui.add(egui::Slider::new(&mut c.hook_count, 2..=10).text("(2-10)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p318(lang));
        ui.add(egui::Slider::new(&mut c.hook_width, 20.0..=60.0).step_by(2.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p305(lang));
        ui.add(egui::Slider::new(&mut c.height, 100.0..=300.0).step_by(10.0));
    });

    let c_copy = *c;
    let label = format!(
        "クランプラック {} hook × W{} × H{}mm",
        c_copy.hook_count, c_copy.hook_width, c_copy.height
    );
    ui.label(crate::i18n::T::prompt_p319(lang));
    ui.label(crate::i18n::T::prompt_p320(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, c_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// フィラメント dry box customizer
/// (`rows × cols × filament_diameter`、printer § 9.3)
///
/// 2D grid cyl cavity for filament spools (utensil_caddy の 2D grid 版)
fn show_dry_box_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new("📦 フィラメント dry box (2D grid spool cavity)").strong());

    let d = &mut state.customizer_state.dry_box;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p321(lang));
        ui.add(egui::Slider::new(&mut d.rows, 1..=4).text("(1-4)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p322(lang));
        ui.add(egui::Slider::new(&mut d.cols, 1..=4).text("(1-4)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p206(lang));
        ui.add(egui::Slider::new(&mut d.filament_diameter, 60.0..=90.0).step_by(2.0));
    });

    let d_copy = *d;
    let label = format!(
        "dry box {}×{} spool × Ø{}mm",
        d_copy.rows, d_copy.cols, d_copy.filament_diameter
    );
    ui.label(crate::i18n::T::prompt_p323(lang));
    ui.label(crate::i18n::T::prompt_p324(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, d_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// 屋外用 IP54 密閉筐体 customizer
/// (`internal_w × internal_d × internal_h`、electronics § 5)
///
/// raspi_case + gasket groove (top rim seal for O-ring)
fn show_outdoor_enclosure_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p325(lang)).strong());

    let e = &mut state.customizer_state.outdoor_enclosure;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p108(lang));
        ui.add(egui::Slider::new(&mut e.internal_width, 80.0..=200.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p326(lang));
        ui.add(egui::Slider::new(&mut e.internal_depth, 60.0..=150.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p327(lang));
        ui.add(egui::Slider::new(&mut e.internal_height, 30.0..=100.0).step_by(5.0));
    });

    let e_copy = *e;
    let label = crate::i18n::T::prompt_fmt_ip54_label(
        e_copy.internal_width,
        e_copy.internal_depth,
        e_copy.internal_height,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p329(lang));
    ui.label(crate::i18n::T::prompt_p330(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, e_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// ジュエリー段付きスタンド customizer
/// (`tier_count × bottom_tier_diameter × height`、drawer § 3.4)
///
/// Multi-tier disk stack + central pillar (wedding cake style)
fn show_jewelry_stand_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p331(lang)).strong());

    let j = &mut state.customizer_state.jewelry_stand;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p332(lang));
        ui.add(egui::Slider::new(&mut j.tier_count, 2..=5).text("(2-5)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p333(lang));
        ui.add(egui::Slider::new(&mut j.bottom_tier_diameter, 60.0..=150.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p305(lang));
        ui.add(egui::Slider::new(&mut j.height, 60.0..=200.0).step_by(10.0));
    });

    let j_copy = *j;
    let label = crate::i18n::T::prompt_fmt_jewelry_label(
        j_copy.tier_count,
        j_copy.bottom_tier_diameter,
        j_copy.height,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p335(lang));
    ui.label(crate::i18n::T::prompt_p336(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, j_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// 充電ドック customizer
/// (`width × upright_height × cable_diameter`、electronics § 4、multi-component)
///
/// Base + tilted upright (15deg 傾斜) + USB-C ケーブル貫通穴 (through-hole vertical)
fn show_phone_dock_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p337(lang)).strong());

    let p = &mut state.customizer_state.phone_dock;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p338(lang));
        ui.add(egui::Slider::new(&mut p.width, 60.0..=120.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p339(lang));
        ui.add(egui::Slider::new(&mut p.upright_height, 60.0..=150.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p340(lang));
        ui.add(egui::Slider::new(&mut p.cable_diameter, 6.0..=12.0).step_by(0.5));
    });

    let p_copy = *p;
    let label = crate::i18n::T::prompt_fmt_phone_dock_label(
        p_copy.width,
        p_copy.upright_height,
        p_copy.cable_diameter,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p342(lang));
    ui.label(crate::i18n::T::prompt_p343(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, p_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// まな板ラック customizer
/// (`slot_count × slot_width × height`、kitchen § 6.6)
///
/// Tall vertical slots (build_plate_rack の tall + deep 版)
fn show_cutting_board_rack_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p344(lang)).strong());

    let c = &mut state.customizer_state.cutting_board_rack;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p141(lang));
        ui.add(egui::Slider::new(&mut c.slot_count, 2..=6).text("(2-6)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p075(lang));
        ui.add(egui::Slider::new(&mut c.slot_width, 8.0..=25.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p298(lang));
        ui.add(egui::Slider::new(&mut c.height, 150.0..=350.0).step_by(10.0));
    });

    let c_copy = *c;
    let label = crate::i18n::T::prompt_fmt_cutting_board_label(
        c_copy.slot_count,
        c_copy.slot_width,
        c_copy.height,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p346(lang));
    ui.label(crate::i18n::T::prompt_p347(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, c_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// テープ dispenser customizer
/// (`inner_diameter × roll_width × wall_thickness`、garage § 8.3、multi-component)
///
/// Base plate + back wall + hood + Z-axis axle + tear edge (4 component composite)
fn show_tape_dispenser_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p348(lang)).strong());

    let t = &mut state.customizer_state.tape_dispenser;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p286(lang));
        ui.add(egui::Slider::new(&mut t.inner_diameter, 25.0..=100.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p349(lang));
        ui.add(egui::Slider::new(&mut t.roll_width, 12.0..=100.0).step_by(2.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p055(lang));
        ui.add(egui::Slider::new(&mut t.wall_thickness, 3.0..=8.0).step_by(0.5));
    });

    let t_copy = *t;
    let label = crate::i18n::T::prompt_fmt_tape_label(
        t_copy.inner_diameter,
        t_copy.roll_width,
        t_copy.wall_thickness,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p351(lang));
    ui.label(crate::i18n::T::prompt_p352(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, t_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// シャワー用棚 customizer
/// (`tier_count × tier_length × tier_depth`、bathroom § 7.5、multi-component)
///
/// Multi-tier wall-mount tray (backplate + N tier tray + drain hole + M4 mount)
fn show_shower_caddy_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p353(lang)).strong());

    let s = &mut state.customizer_state.shower_caddy;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p332(lang));
        ui.add(egui::Slider::new(&mut s.tier_count, 1..=4).text("(1-4)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p354(lang));
        ui.add(egui::Slider::new(&mut s.tier_length, 150.0..=350.0).step_by(10.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p355(lang));
        ui.add(egui::Slider::new(&mut s.tier_depth, 80.0..=180.0).step_by(5.0));
    });

    let s_copy = *s;
    let label = crate::i18n::T::prompt_fmt_shower_label(
        s_copy.tier_count,
        s_copy.tier_length,
        s_copy.tier_depth,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p357(lang));
    ui.label(crate::i18n::T::prompt_p358(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, s_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// ノギスホルダー customizer
/// (`jaw_length × throat_depth × count`、tools § 4、multi-component)
///
/// Wall-mount backplate + N caliper slot + 4 corner M4 mount
fn show_caliper_holder_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p359(lang)).strong());

    let c = &mut state.customizer_state.caliper_holder;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p360(lang));
        ui.add(egui::Slider::new(&mut c.jaw_length, 100.0..=300.0).step_by(10.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p361(lang));
        ui.add(egui::Slider::new(&mut c.throat_depth, 25.0..=60.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p362(lang));
        ui.add(egui::Slider::new(&mut c.count, 1..=6).text("(1-6)"));
    });

    let c_copy = *c;
    let label = crate::i18n::T::prompt_fmt_caliper_label(
        c_copy.jaw_length,
        c_copy.throat_depth,
        c_copy.count,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p364(lang));
    ui.label(crate::i18n::T::prompt_p365(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, c_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// 袋クリップ整理 customizer
/// (`slot_count × slot_width × height`、kitchen § 6.3)
///
/// 縦 slot rack (magnetic_strip の vertical 変種、chip bag clip 収納)
fn show_bag_clip_org_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p366(lang)).strong());

    let b = &mut state.customizer_state.bag_clip_org;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p141(lang));
        ui.add(egui::Slider::new(&mut b.slot_count, 4..=16).text("(4-16)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p075(lang));
        ui.add(egui::Slider::new(&mut b.slot_width, 5.0..=15.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p305(lang));
        ui.add(egui::Slider::new(&mut b.height, 60.0..=150.0).step_by(5.0));
    });

    let b_copy = *b;
    let label = crate::i18n::T::prompt_fmt_bagclip_label(
        b_copy.slot_count,
        b_copy.slot_width,
        b_copy.height,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p368(lang));
    ui.label(crate::i18n::T::prompt_p369(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, b_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// 缶ラック customizer
/// (`rows × can_diameter × tilt_angle_deg`、kitchen § 6.4、multi-tier)
///
/// Gravity feed tilted shelf (2 tier tilted shelf + 側壁 + 前 lip、cans 転がって前へ)
fn show_can_rack_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p370(lang)).strong());

    let c = &mut state.customizer_state.can_rack;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p371(lang));
        ui.add(egui::Slider::new(&mut c.rows, 1..=4).text("(1-4)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p372(lang));
        ui.add(egui::Slider::new(&mut c.can_diameter, 50.0..=80.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p373(lang));
        ui.add(egui::Slider::new(&mut c.tilt_angle_deg, 5.0..=20.0).step_by(1.0));
    });

    let c_copy = *c;
    let label = crate::i18n::T::prompt_fmt_can_rack_label(
        c_copy.rows,
        c_copy.can_diameter,
        c_copy.tilt_angle_deg,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p375(lang));
    ui.label(crate::i18n::T::prompt_p376(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, c_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// LED hub 筐体 customizer
/// (`internal_w × internal_d × internal_h`、electronics § 6、multi-component)
///
/// raspi_case + front LED window + top-right antenna hole (3-component composite)
fn show_led_hub_box_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p377(lang)).strong());

    let l = &mut state.customizer_state.led_hub_box;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p108(lang));
        ui.add(egui::Slider::new(&mut l.internal_width, 60.0..=150.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p326(lang));
        ui.add(egui::Slider::new(&mut l.internal_depth, 40.0..=120.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p327(lang));
        ui.add(egui::Slider::new(&mut l.internal_height, 20.0..=80.0).step_by(5.0));
    });

    let l_copy = *l;
    let label = crate::i18n::T::prompt_fmt_led_hub_label(
        l_copy.internal_width,
        l_copy.internal_depth,
        l_copy.internal_height,
        lang,
    );
    ui.label(crate::i18n::T::prompt_p379(lang));
    ui.label(crate::i18n::T::prompt_p380(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, l_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

/// メイク整理 customizer
/// (`rows × cols × cell_size`、drawer § 3.5)
///
/// 2D grid multi-cell (pill_organizer の large 版、makeup brush / lipstick 用)
fn show_makeup_organizer_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p381(lang)).strong());

    let m = &mut state.customizer_state.makeup_organizer;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p151(lang));
        ui.add(egui::Slider::new(&mut m.rows, 2..=6).text("(2-6)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p152(lang));
        ui.add(egui::Slider::new(&mut m.cols, 2..=8).text("(2-8)"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p382(lang));
        ui.add(egui::Slider::new(&mut m.cell_size, 25.0..=80.0).step_by(5.0));
    });

    let m_copy = *m;
    let label =
        crate::i18n::T::prompt_fmt_makeup_label(m_copy.rows, m_copy.cols, m_copy.cell_size, lang);
    ui.label(crate::i18n::T::prompt_p384(lang));
    ui.label(crate::i18n::T::prompt_p385(lang));
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, m_copy.to_lol(), &label, lang);
    }

    ui.add_space(2.0);
}

// ── Sprint 21-22 + Multi-domain 16 archetype customizer (2026-08-31、Task A) ──

fn show_vesa_mount_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p386(lang)).strong());
    let g = &mut state.customizer_state.vesa_mount;
    ui.horizontal(|ui| {
        ui.label("VESA サイズ (mm):");
        ui.add(egui::Slider::new(&mut g.vesa_size, 50.0..=150.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p387(lang));
        ui.add(egui::Slider::new(&mut g.plate_thickness, 3.0..=10.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p388(lang));
        ui.add(egui::Slider::new(&mut g.hole_m_size, 3.0..=8.0).step_by(1.0));
    });
    let g_copy = *g;
    let label = crate::i18n::T::prompt_fmt_vesa_label(
        g_copy.vesa_size,
        g_copy.vesa_size,
        g_copy.hole_m_size,
        g_copy.plate_thickness,
        lang,
    );
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, g_copy.to_lol(), &label, lang);
    }
    ui.add_space(2.0);
}

fn show_l_bracket_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p390(lang)).strong());
    let g = &mut state.customizer_state.l_bracket;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p391(lang));
        ui.add(egui::Slider::new(&mut g.arm_width, 30.0..=150.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p392(lang));
        ui.add(egui::Slider::new(&mut g.arm_height, 30.0..=150.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p387(lang));
        ui.add(egui::Slider::new(&mut g.plate_thickness, 2.0..=8.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p388(lang));
        ui.add(egui::Slider::new(&mut g.m_size, 3.0..=8.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p393(lang));
        ui.add(egui::Slider::new(&mut g.holes_per_arm, 1..=4));
    });
    let g_copy = *g;
    let label = crate::i18n::T::prompt_fmt_l_bracket_label(
        g_copy.m_size,
        g_copy.holes_per_arm,
        g_copy.arm_width,
        g_copy.arm_height,
        g_copy.plate_thickness,
        lang,
    );
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, g_copy.to_lol(), &label, lang);
    }
    ui.add_space(2.0);
}

fn show_t_slot_bracket_2020_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new("🔧 2020 T-slot ブラケット (M5 CB)").strong());
    let g = &mut state.customizer_state.t_slot_bracket_2020;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p080(lang));
        ui.add(egui::Slider::new(&mut g.arm_size, 20.0..=80.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label("depth (mm):");
        ui.add(egui::Slider::new(&mut g.depth, 20.0..=80.0).step_by(5.0));
    });
    let g_copy = *g;
    let label = format!(
        "2020 T-slot ブラケット {}×{}mm",
        g_copy.arm_size, g_copy.depth
    );
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, g_copy.to_lol(), &label, lang);
    }
    ui.add_space(2.0);
}

fn show_raspi_mount_plate_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p395(lang)).strong());
    let g = &mut state.customizer_state.raspi_mount_plate;
    ui.horizontal(|ui| {
        ui.label("Pi model:");
        ui.add(egui::Slider::new(&mut g.model, 0..=5).text("0=Zero, 3=3B+, 4=4B, 5=Pi5"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p396(lang));
        ui.add(
            egui::Slider::new(&mut g.extra_m4_holes, 0..=4).text(crate::i18n::T::prompt_p397(lang)),
        );
    });
    let g_copy = *g;
    let label =
        crate::i18n::T::prompt_fmt_raspi_mount_label(g_copy.model, g_copy.extra_m4_holes, lang);
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, g_copy.to_lol(), &label, lang);
    }
    ui.add_space(2.0);
}

fn show_heat_set_array_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new("🔥 Heat-set insert grid (McMaster/Voxel8)").strong());
    let g = &mut state.customizer_state.heat_set_array;
    ui.horizontal(|ui| {
        ui.label("rows:");
        ui.add(egui::Slider::new(&mut g.rows, 1..=6));
    });
    ui.horizontal(|ui| {
        ui.label("cols:");
        ui.add(egui::Slider::new(&mut g.cols, 1..=6));
    });
    ui.horizontal(|ui| {
        ui.label("insert M:");
        ui.add(egui::Slider::new(&mut g.insert_m, 3.0..=8.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label("pitch (mm):");
        ui.add(egui::Slider::new(&mut g.pitch, 15.0..=40.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p387(lang));
        ui.add(egui::Slider::new(&mut g.base_thickness, 4.0..=12.0).step_by(0.5));
    });
    let g_copy = *g;
    let label = format!(
        "Heat-set grid M{}×{} ({}×{})",
        g_copy.insert_m,
        g_copy.rows * g_copy.cols,
        g_copy.rows,
        g_copy.cols
    );
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, g_copy.to_lol(), &label, lang);
    }
    ui.add_space(2.0);
}

fn show_flange_mount_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p399(lang)).strong());
    let g = &mut state.customizer_state.flange_mount;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p400(lang));
        ui.add(egui::Slider::new(&mut g.outer_dia, 40.0..=150.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label("ボルト M:");
        ui.add(egui::Slider::new(&mut g.bolt_m, 3.0..=8.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p401(lang));
        ui.add(egui::Slider::new(&mut g.hole_count, 3..=8));
    });
    let g_copy = *g;
    let label = format!(
        "フランジ Φ{} M{}×{}",
        g_copy.outer_dia, g_copy.bolt_m, g_copy.hole_count
    );
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, g_copy.to_lol(), &label, lang);
    }
    ui.add_space(2.0);
}

fn show_dovetail_pair_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p402(lang)).strong());
    let g = &mut state.customizer_state.dovetail_pair;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p403(lang));
        ui.add(egui::Slider::new(&mut g.base_width, 10.0..=40.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p062(lang));
        ui.add(egui::Slider::new(&mut g.height, 8.0..=30.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p404(lang));
        ui.add(egui::Slider::new(&mut g.depth, 5.0..=30.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p405(lang));
        ui.add(egui::Slider::new(&mut g.gender, 0..=1).text("0=male, 1=female"));
    });
    let g_copy = *g;
    let gender_str = if g_copy.gender == 0 {
        "オス"
    } else {
        "メス"
    };
    let label = crate::i18n::T::prompt_fmt_dovetail_label(
        gender_str,
        g_copy.base_width,
        g_copy.height,
        g_copy.depth,
        lang,
    );
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, g_copy.to_lol(), &label, lang);
    }
    ui.add_space(2.0);
}

fn show_profile_extrusion_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new("📏 アルミプロファイル (2020 / 3030)").strong());
    let g = &mut state.customizer_state.profile_extrusion;
    ui.horizontal(|ui| {
        ui.label("kind:");
        ui.add(egui::Slider::new(&mut g.kind, 20..=30).text("20=2020, 30=3030"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p407(lang));
        ui.add(egui::Slider::new(&mut g.length, 30.0..=500.0).step_by(10.0));
    });
    let g_copy = *g;
    let label = format!("プロファイル {} × {}mm", g_copy.kind, g_copy.length);
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, g_copy.to_lol(), &label, lang);
    }
    ui.add_space(2.0);
}

fn show_snap_fit_pair_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new("🪝 スナップフィット cantilever (PLA)").strong());
    let g = &mut state.customizer_state.snap_fit_pair;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p408(lang));
        ui.add(egui::Slider::new(&mut g.length, 10.0..=40.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p409(lang));
        ui.add(egui::Slider::new(&mut g.width, 3.0..=10.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p410(lang));
        ui.add(egui::Slider::new(&mut g.thickness, 1.0..=4.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p411(lang));
        ui.add(egui::Slider::new(&mut g.hook_height, 0.5..=2.0).step_by(0.1));
    });
    let g_copy = *g;
    let label = format!(
        "スナップフィット {}×{}×{} hook{}",
        g_copy.length, g_copy.width, g_copy.thickness, g_copy.hook_height
    );
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, g_copy.to_lol(), &label, lang);
    }
    ui.add_space(2.0);
}

fn show_boss_array_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p412(lang)).strong());
    let g = &mut state.customizer_state.boss_array;
    ui.horizontal(|ui| {
        ui.label("rows:");
        ui.add(egui::Slider::new(&mut g.rows, 1..=6));
    });
    ui.horizontal(|ui| {
        ui.label("cols:");
        ui.add(egui::Slider::new(&mut g.cols, 1..=6));
    });
    ui.horizontal(|ui| {
        ui.label("ネジ M:");
        ui.add(egui::Slider::new(&mut g.screw_m, 3.0..=8.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p413(lang));
        ui.add(egui::Slider::new(&mut g.boss_height, 5.0..=25.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label("pitch (mm):");
        ui.add(egui::Slider::new(&mut g.pitch, 15.0..=40.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p387(lang));
        ui.add(egui::Slider::new(&mut g.base_thickness, 1.0..=6.0).step_by(0.5));
    });
    let g_copy = *g;
    let label = format!(
        "Boss array M{}×{} ({}×{}) h{}",
        g_copy.screw_m,
        g_copy.rows * g_copy.cols,
        g_copy.rows,
        g_copy.cols,
        g_copy.boss_height
    );
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, g_copy.to_lol(), &label, lang);
    }
    ui.add_space(2.0);
}

fn show_bearing_seat_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p414(lang)).strong());
    let g = &mut state.customizer_state.bearing_seat;
    ui.horizontal(|ui| {
        ui.label("bearing OD (mm):");
        ui.add(
            egui::Slider::new(&mut g.bearing_size, 16.0..=35.0)
                .step_by(1.0)
                .text("16=688, 22=608, 28=6001, 35=6202"),
        );
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p387(lang));
        ui.add(egui::Slider::new(&mut g.plate_thickness, 3.0..=15.0).step_by(0.5));
    });
    ui.horizontal(|ui| {
        ui.label("style:");
        ui.add(
            egui::Slider::new(&mut g.style, 0..=2)
                .text("0=press-fit, 1=slip fit, 2=through shoulder"),
        );
    });
    let g_copy = *g;
    let label = crate::i18n::T::prompt_fmt_bearing_label(
        g_copy.bearing_size,
        g_copy.plate_thickness,
        g_copy.style,
        lang,
    );
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, g_copy.to_lol(), &label, lang);
    }
    ui.add_space(2.0);
}

fn show_cable_grommet_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p416(lang)).strong());
    let g = &mut state.customizer_state.cable_grommet;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p400(lang));
        ui.add(egui::Slider::new(&mut g.outer_dia, 30.0..=120.0).step_by(5.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p062(lang));
        ui.add(egui::Slider::new(&mut g.height, 10.0..=40.0).step_by(1.0));
    });
    let g_copy = *g;
    let label = format!("グロメット Ø{}×{}mm", g_copy.outer_dia, g_copy.height);
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, g_copy.to_lol(), &label, lang);
    }
    ui.add_space(2.0);
}

fn show_curtain_rod_bracket_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p417(lang)).strong());
    let g = &mut state.customizer_state.curtain_rod_bracket;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p418(lang));
        ui.add(egui::Slider::new(&mut g.rod_dia, 15.0..=40.0).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p419(lang));
        ui.add(egui::Slider::new(&mut g.projection, 80.0..=200.0).step_by(10.0));
    });
    let g_copy = *g;
    let label = crate::i18n::T::prompt_fmt_curtain_label(g_copy.rod_dia, g_copy.projection, lang);
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, g_copy.to_lol(), &label, lang);
    }
    ui.add_space(2.0);
}

fn show_arduino_mount_plate_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p421(lang)).strong());
    let g = &mut state.customizer_state.arduino_mount_plate;
    ui.horizontal(|ui| {
        ui.label("board:");
        ui.add(egui::Slider::new(&mut g.board_type, 1..=3).text("1=Uno, 2=Mega, 3=Nano"));
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::T::prompt_p422(lang));
        ui.add(
            egui::Slider::new(&mut g.extra_m4_holes, 0..=4).text(crate::i18n::T::prompt_p397(lang)),
        );
    });
    let g_copy = *g;
    let board_str = match g_copy.board_type {
        2 => "Mega",
        3 => "Nano",
        _ => "Uno",
    };
    let label = crate::i18n::T::prompt_fmt_arduino_label(board_str, g_copy.extra_m4_holes, lang);
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, g_copy.to_lol(), &label, lang);
    }
    ui.add_space(2.0);
}

fn show_pixhawk_mount_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new("🚁 Pixhawk マウント (drone/FPV autopilot)").strong());
    let g = &mut state.customizer_state.pixhawk_mount;
    ui.horizontal(|ui| {
        ui.label("hole pattern (mm):");
        ui.add(
            egui::Slider::new(&mut g.hole_pattern_size, 30.0..=60.0)
                .step_by(5.0)
                .text("45=full, 30=mini"),
        );
    });
    ui.horizontal(|ui| {
        ui.label("damper style:");
        ui.add(egui::Slider::new(&mut g.damper_style, 0..=1).text("0=solid, 1=Ø10 damper"));
    });
    let g_copy = *g;
    let damper_str = if g_copy.damper_style == 0 {
        "solid"
    } else {
        "damper"
    };
    let label = format!(
        "Pixhawk {}×{}mm ({})",
        g_copy.hole_pattern_size, g_copy.hole_pattern_size, damper_str
    );
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, g_copy.to_lol(), &label, lang);
    }
    ui.add_space(2.0);
}

fn show_servo_mount_customizer(ui: &mut egui::Ui, state: &mut AppState, lang: Lang) {
    ui.label(egui::RichText::new(crate::i18n::T::prompt_p424(lang)).strong());
    let g = &mut state.customizer_state.servo_mount;
    ui.horizontal(|ui| {
        ui.label("servo type:");
        ui.add(egui::Slider::new(&mut g.servo_type, 1..=2).text("1=SG90 mini, 2=MG996R standard"));
    });
    let g_copy = *g;
    let servo_str = if g_copy.servo_type == 1 {
        "SG90 mini (M2×2)"
    } else {
        "MG996R standard (M3×2)"
    };
    let label = crate::i18n::T::prompt_fmt_servo_label(servo_str, lang);
    if ui
        .button(format!("{}: {label}", crate::i18n::T::prompt_create(lang)))
        .clicked()
    {
        state.prompt_input.clear();
        state.prompt_focused_once = false;
        start_generation_from_lol(state, g_copy.to_lol(), &label, lang);
    }
    ui.add_space(2.0);
}
