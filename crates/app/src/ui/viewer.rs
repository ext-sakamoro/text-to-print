use egui::Ui;

use crate::sdf::SdfRenderCallback;
use crate::state::{AppState, GenerationStatus};

/// SDF ビューアの状態
#[derive(Default)]
pub struct SdfViewer {
    pub has_sdf: bool,
    pub pending_wgsl: Option<String>,
    pub camera: alice_view::app::Camera3D,
    /// この LOL source で parse を既に試行したか (成功でも失敗でも true)
    /// UI が毎 frame set_lol を呼ぶため、失敗時に log spam を防ぐ
    /// 新しい LOL source が来たら reset される (set_lol 内で管理)
    last_attempted_lol: Option<String>,
}

impl SdfViewer {
    pub fn set_lol(&mut self, lol_source: &str) {
        // 同 LOL source を毎 frame 再試行しない (parse 失敗時 30-60 FPS で
        // log spam していた事案対策) 新規 LOL が来たら再試行
        if self
            .last_attempted_lol
            .as_deref()
            .is_some_and(|prev| prev == lol_source)
        {
            return;
        }
        self.last_attempted_lol = Some(lol_source.to_string());
        match text_to_print_core::pipeline::lol_to_wgsl(lol_source) {
            Ok(wgsl) => {
                self.pending_wgsl = Some(wgsl);
                self.has_sdf = true;
                tracing::info!("WGSL shader generated for SDF preview");
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to generate WGSL: {e} (LOL: {})",
                    lol_source.chars().take(80).collect::<String>()
                );
                self.has_sdf = false;
            }
        }
    }
}

pub fn show(ui: &mut Ui, state: &AppState, viewer: &mut SdfViewer) {
    // 生成完了時に WGSL を生成
    if let GenerationStatus::Done { lol_source, .. } = &state.generation_status {
        viewer.set_lol(lol_source);
    }

    let rect = ui.available_rect_before_wrap();

    if viewer.has_sdf {
        // SDF Raymarching を PaintCallback で描画
        let callback = egui_wgpu::Callback::new_paint_callback(rect, SdfRenderCallback);
        ui.painter().add(callback);

        // カメラ操作
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());

        if response.dragged() {
            let delta = response.drag_delta();
            viewer.camera.orbit(-delta.x * 0.01, delta.y * 0.01);
            ui.ctx().request_repaint();
        }

        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.1 {
            viewer.camera.dolly(scroll * 0.05);
            ui.ctx().request_repaint();
        }

        // 右上: カメラリセット
        let reset_rect = egui::Rect::from_min_size(
            egui::pos2(rect.max.x - 96.0, rect.min.y + 8.0),
            egui::vec2(88.0, 24.0),
        );
        let mut reset_ui = ui.new_child(egui::UiBuilder::new().max_rect(reset_rect));
        if reset_ui.button("カメラリセット").clicked() {
            viewer.camera = alice_view::app::Camera3D::default();
            reset_ui.ctx().request_repaint();
        }

        // オーバーレイ
        let cam = &viewer.camera;
        ui.painter().text(
            egui::pos2(rect.min.x + 8.0, rect.min.y + 8.0),
            egui::Align2::LEFT_TOP,
            format!(
                "SDF Raymarching | Cam: ({:.1}, {:.1}, {:.1})",
                cam.position.x, cam.position.y, cam.position.z
            ),
            egui::FontId::monospace(11.0),
            egui::Color32::from_rgba_unmultiplied(200, 200, 220, 180),
        );

        if let GenerationStatus::Done {
            mesh_stats: Some(stats),
            ..
        } = &state.generation_status
        {
            ui.painter().text(
                egui::pos2(rect.min.x + 8.0, rect.min.y + 24.0),
                egui::Align2::LEFT_TOP,
                format!(
                    "頂点: {}  三角形: {}",
                    stats.vertex_count, stats.triangle_count
                ),
                egui::FontId::monospace(11.0),
                egui::Color32::from_rgba_unmultiplied(180, 220, 180, 180),
            );
        }
    } else {
        // プレースホルダー
        ui.painter()
            .rect_filled(rect, 4.0, egui::Color32::from_rgb(24, 24, 32));

        let msg = match &state.generation_status {
            GenerationStatus::Generating => "生成中...",
            _ => "テキストを入力して「生成」を押してください",
        };

        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            msg,
            egui::FontId::proportional(16.0),
            egui::Color32::from_rgb(100, 100, 120),
        );

        ui.allocate_rect(rect, egui::Sense::hover());
    }
}
