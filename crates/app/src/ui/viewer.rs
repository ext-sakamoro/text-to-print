use egui::Ui;
use glam::Vec3;
use std::sync::Arc;

use crate::state::{AppState, GenerationStatus};

/// Mesh preview viewer state
///
/// Holds the last-known mesh AABB for framing and a monotonic version
/// counter that flags when a new mesh needs to be uploaded to the GPU
#[derive(Default)]
pub struct MeshViewer {
    /// Set to true once a mesh has been uploaded to GPU buffers
    pub has_mesh: bool,
    /// Serial number of the mesh currently on the GPU — compared against
    /// `AppState::mesh_version` to trigger re-upload on new generations
    pub uploaded_version: u64,
    /// AABB of the currently uploaded mesh, used for camera framing
    pub aabb: Option<(Vec3, Vec3)>,
}

/// Legacy alias so main.rs and existing UI code keep compiling while the
/// mesh-preview refactor lands, keeping the diff minimal
pub type SdfViewer = MeshViewer;

impl MeshViewer {
    /// Notice a new mesh from `state.viewer_mesh` and upload it to GPU
    ///
    /// - No-op when the state's mesh version matches what we already
    ///   uploaded, so this can be called every frame cheaply
    /// - Frames the camera automatically on the first upload of each new
    ///   mesh so the shape is visible without manual camera reset
    pub fn sync_with_state(
        &mut self,
        state: &AppState,
        render_state: &egui_wgpu::RenderState,
    ) -> bool {
        if state.mesh_version == self.uploaded_version && self.has_mesh {
            return false;
        }
        let Some(mesh_arc) = state.viewer_mesh.as_ref() else {
            return false;
        };
        let mut renderer = render_state.renderer.write();
        let Some(res) = renderer
            .callback_resources
            .get_mut::<crate::sdf::MeshResources>()
        else {
            return false;
        };
        res.pipeline.upload_mesh(&render_state.device, mesh_arc);
        let (mn, mx) = compute_aabb(mesh_arc);
        self.aabb = Some((mn, mx));
        self.has_mesh = true;
        self.uploaded_version = state.mesh_version;

        // Auto-frame on new upload
        if let Ok(mut cam) = res.camera.lock() {
            // Aspect is not known yet (viewport size only reached in
            // paint) so use 1.0 as a neutral fit; `Camera::frame` clamps
            cam.frame(mn, mx, 1.0);
        }
        tracing::info!(
            vertices = mesh_arc.vertices.len(),
            triangles = mesh_arc.indices.len() / 3,
            aabb_min = ?mn,
            aabb_max = ?mx,
            "mesh uploaded to preview"
        );
        true
    }
}

fn compute_aabb(mesh: &alice_sdf::mesh::Mesh) -> (Vec3, Vec3) {
    let mut mn = Vec3::splat(f32::INFINITY);
    let mut mx = Vec3::splat(f32::NEG_INFINITY);
    for v in &mesh.vertices {
        mn = mn.min(v.position);
        mx = mx.max(v.position);
    }
    if !mn.is_finite() || !mx.is_finite() {
        return (Vec3::splat(-50.0), Vec3::splat(50.0));
    }
    (mn, mx)
}

pub fn show(
    ui: &mut Ui,
    state: &AppState,
    viewer: &mut MeshViewer,
    render_state: Option<&egui_wgpu::RenderState>,
    lang: crate::i18n::Lang,
) {
    let rect = ui.available_rect_before_wrap();

    if viewer.has_mesh {
        // Render mesh to offscreen color+depth texture, then display the
        // resulting texture as an egui Image so hollow shapes show with
        // correct depth ordering (the old egui-pass callback had no depth
        // attachment, causing "just a box" blob-out)
        //
        // Single write-lock scope: we take MeshResources out of
        // callback_resources so we can call `ensure_targets` (which needs
        // `&mut Renderer` for `register_native_texture`) AND methods on
        // MeshResources without split-borrow issues Then insert back
        // std RwLock is not re-entrant, so nested `renderer.write()`
        // would deadlock (mouse spinner, UI hang)
        let mut rendered_id: Option<egui::TextureId> = None;
        if let Some(rs) = render_state {
            // Multiply by pixels_per_point so the offscreen texture has
            // native display resolution (avoids blurry preview on hi-DPI)
            let ppp = ui.ctx().pixels_per_point();
            let px_w = (rect.width() * ppp).max(1.0) as u32;
            let px_h = (rect.height() * ppp).max(1.0) as u32;
            let mut renderer = rs.renderer.write();
            if let Some(mut res) = renderer
                .callback_resources
                .remove::<crate::sdf::MeshResources>()
            {
                res.ensure_targets(&rs.device, &mut renderer, (px_w, px_h));
                rendered_id = res.render_frame(&rs.device, &rs.queue);
                renderer.callback_resources.insert(res);
            }
        }
        if let Some(id) = rendered_id {
            let img = egui::Image::new(egui::load::SizedTexture::new(id, rect.size()));
            ui.put(rect, img);
        }

        // Camera interaction — drag orbits, scroll dollies
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        let mut camera_changed = false;

        if response.dragged() {
            let delta = response.drag_delta();
            if let Some(res) = ui.ctx().memory(|m| {
                m.data
                    .get_temp::<Arc<std::sync::Mutex<crate::sdf::Camera>>>(camera_id())
            }) && let Ok(mut cam) = res.lock()
            {
                cam.orbit(-delta.x * 0.008, -delta.y * 0.008);
                camera_changed = true;
            }
        }

        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.1
            && let Some(res) = ui.ctx().memory(|m| {
                m.data
                    .get_temp::<Arc<std::sync::Mutex<crate::sdf::Camera>>>(camera_id())
            })
            && let Ok(mut cam) = res.lock()
        {
            let dolly = -scroll
                * (viewer
                    .aabb
                    .map(|(mn, mx)| (mx - mn).length())
                    .unwrap_or(100.0)
                    * 0.002);
            cam.dolly(dolly);
            camera_changed = true;
        }

        if camera_changed {
            ui.ctx().request_repaint();
        }

        // Reset button (top-right)
        let reset_rect = egui::Rect::from_min_size(
            egui::pos2(rect.max.x - 100.0, rect.min.y + 8.0),
            egui::vec2(92.0, 24.0),
        );
        let mut reset_ui = ui.new_child(egui::UiBuilder::new().max_rect(reset_rect));
        if reset_ui.button("カメラリセット").clicked()
            && let Some(res) = ui.ctx().memory(|m| {
                m.data
                    .get_temp::<Arc<std::sync::Mutex<crate::sdf::Camera>>>(camera_id())
            })
            && let Ok(mut cam) = res.lock()
            && let Some((mn, mx)) = viewer.aabb
        {
            let aspect = (rect.width() / rect.height().max(1.0)).max(0.1);
            cam.frame(mn, mx, aspect);
            reset_ui.ctx().request_repaint();
        }

        // Stats overlay
        if let GenerationStatus::Done {
            mesh_stats: Some(stats),
            ..
        } = &state.generation_status
        {
            ui.painter().text(
                egui::pos2(rect.min.x + 8.0, rect.min.y + 8.0),
                egui::Align2::LEFT_TOP,
                format!(
                    "{}  {} {}  {} {}",
                    crate::i18n::T::mesh_preview_label(lang),
                    crate::i18n::T::vertices(lang),
                    stats.vertex_count,
                    crate::i18n::T::triangles(lang),
                    stats.triangle_count
                ),
                egui::FontId::monospace(11.0),
                egui::Color32::from_rgba_unmultiplied(210, 210, 220, 200),
            );
        }
    } else {
        // Placeholder background + hint text
        ui.painter()
            .rect_filled(rect, 4.0, egui::Color32::from_rgb(28, 28, 34));

        let msg = match &state.generation_status {
            GenerationStatus::Generating => crate::i18n::T::generating(lang),
            GenerationStatus::Error(_) => crate::i18n::T::generation_failed(lang),
            _ => crate::i18n::T::enter_prompt(lang),
        };
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            msg,
            egui::FontId::proportional(16.0),
            egui::Color32::from_rgb(120, 120, 140),
        );
        ui.allocate_rect(rect, egui::Sense::hover());
    }
}

/// egui memory id used to stash the shared camera handle so the drag / scroll
/// handlers can reach it without threading the wgpu render_state through
fn camera_id() -> egui::Id {
    egui::Id::new("text_to_print_mesh_camera")
}

/// Publish the current mesh_view camera handle to egui memory so `show()`
/// can mutate it in response to drag / scroll input Called once per frame
/// from `App::update` after wgpu resources have been ensured
pub fn publish_camera(ctx: &egui::Context, camera: Arc<std::sync::Mutex<crate::sdf::Camera>>) {
    ctx.memory_mut(|m| m.data.insert_temp(camera_id(), camera));
}
