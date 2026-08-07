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
        let result = backend::generate_with_retry(
            &backend,
            &inference_params,
            prompt::SYSTEM_PROMPT,
            &prompt_text,
            3,
            |response| {
                let _ = tx_retry.send(GenerationMessage::PhaseStart(GenerationPhase::Llm));
                let lol = pipeline::extract_lol(response).unwrap_or_else(|| response.to_string());
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
                let lol = pipeline::extract_lol(&response).unwrap_or_else(|| response.clone());
                tracing::info!(
                    extracted_len = lol.len(),
                    extracted_preview = %lol.chars().take(200).collect::<String>(),
                    "LOL extracted from LLM response"
                );
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

                let _ = tx.send(GenerationMessage::Success {
                    id,
                    lol_source: lol,
                    mesh_stats: mesh_stats.map(Box::new),
                    retry_count,
                    share_dry_run,
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

/// テンプレート prompt を prompt 入力欄に注入する UI section
///
/// 3 カテゴリ (実用品 / DIY / ゲーム・装飾) 各 4-5 template
/// ボタンをクリックすると `state.prompt_input` に text を書き込む
/// user は数値を修正して生成 生成中は disabled
fn show_prompt_templates(ui: &mut egui::Ui, state: &mut AppState, is_generating: bool) {
    ui.collapsing("テンプレート (クリックで prompt に挿入)", |ui| {
        ui.add_enabled_ui(!is_generating, |ui| {
            const CATEGORIES: &[(&str, &[(&str, &str)])] = &[
                (
                    "実用品",
                    &[
                        ("ネジ M6", "M6 の六角ボルト、頭径 10mm、頭厚 4mm、ネジ部長さ 25mm、ネジ径 6mm"),
                        (
                            "L字フック",
                            "L字型のフック、長辺 60mm、短辺 40mm、幅 15mm、厚さ 5mm、両端に直径 5mm のネジ穴",
                        ),
                        ("ワッシャー", "ワッシャー、外径 20mm、内径 8mm、厚さ 2mm"),
                        (
                            "L字ブラケット",
                            "L字ブラケット、辺 50mm × 50mm、幅 30mm、厚さ 4mm、両辺に直径 5mm のネジ穴を 2 つずつ",
                        ),
                        (
                            "スペーサー",
                            "円柱型スペーサー、外径 12mm、内径 4mm、高さ 15mm",
                        ),
                    ],
                ),
                (
                    "DIY / インテリア",
                    &[
                        (
                            "壁掛けフック",
                            "壁掛け用フック、ベース板 40mm × 60mm × 5mm、フック部分 30mm 突き出し、ネジ穴 2 個 (直径 5mm)",
                        ),
                        (
                            "取っ手 (ノブ)",
                            "ドロワーノブ、ヘッド直径 30mm、高さ 20mm、ネジ穴 M4 深さ 12mm",
                        ),
                        (
                            "スマホスタンド",
                            "スマホスタンド、幅 80mm、奥行 60mm、高さ 40mm、傾斜角 65 度、ケーブル穴 直径 10mm",
                        ),
                        (
                            "コースター",
                            "円形コースター、直径 90mm、厚さ 4mm、縁 2mm 立ち上がり",
                        ),
                        (
                            "植木鉢",
                            "円柱型植木鉢、外径 80mm、高さ 100mm、壁厚 3mm、底に排水穴 5mm × 4 個",
                        ),
                    ],
                ),
                (
                    "ゲーム / 装飾",
                    &[
                        (
                            "椅子",
                            "シンプルな椅子、座面 40mm × 40mm × 4mm、脚 4 本 (角柱 4mm × 4mm × 40mm)、背もたれ 40mm × 45mm × 4mm",
                        ),
                        (
                            "テーブル",
                            "四角いテーブル、天板 80mm × 60mm × 5mm、脚 4 本 (角柱 5mm × 5mm × 30mm)",
                        ),
                        ("本棚", "本棚、幅 60mm、高さ 80mm、奥行 20mm、棚板 3 枚、板厚 3mm"),
                        (
                            "剣",
                            "ファンタジー風の剣、刃長 100mm、刃幅 15mm、刃厚 3mm、鍔 30mm × 8mm、柄 40mm × 10mm",
                        ),
                        (
                            "宝箱",
                            "宝箱、本体 60mm × 40mm × 30mm、蓋 60mm × 40mm × 15mm (アーチ状)、金具 4 個",
                        ),
                    ],
                ),
            ];
            for (cat_name, items) in CATEGORIES {
                ui.label(egui::RichText::new(*cat_name).strong());
                ui.horizontal_wrapped(|ui| {
                    for (label, template) in *items {
                        if ui.button(*label).clicked() {
                            state.prompt_input = (*template).to_string();
                            state.prompt_focused_once = false;
                        }
                    }
                });
                ui.add_space(2.0);
            }
        });
    });
}
