use egui_wgpu::{CallbackResources, CallbackTrait, ScreenDescriptor};
use glam::{Mat4, Vec3};
use std::sync::{Arc, Mutex};

use super::pipeline::{MeshPipeline, MeshUniforms};

/// Camera state shared between the UI (for orbit / dolly / reset) and the
/// wgpu render callback (for view matrix)
///
/// Z-up world: matches Bambu Studio and the 3MF spec so the mesh preview
/// shows the same orientation as the exported file
#[derive(Debug, Clone)]
pub struct Camera {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            position: Vec3::new(200.0, -200.0, 200.0),
            target: Vec3::new(0.0, 0.0, 30.0),
            up: Vec3::Z,
            fov: std::f32::consts::FRAC_PI_4,
            near: 1.0,
            far: 5000.0,
        }
    }
}

impl Camera {
    /// Orbit around `target` in spherical coordinates with Z as up
    pub fn orbit(&mut self, delta_yaw: f32, delta_pitch: f32) {
        let offset = self.position - self.target;
        let radius = offset.length().max(1e-3);
        let mut yaw = offset.y.atan2(offset.x);
        let mut pitch = (offset.z / radius).clamp(-1.0, 1.0).asin();

        yaw += delta_yaw;
        pitch = (pitch + delta_pitch).clamp(
            -std::f32::consts::FRAC_PI_2 + 0.05,
            std::f32::consts::FRAC_PI_2 - 0.05,
        );

        let cos_p = pitch.cos();
        self.position = self.target
            + Vec3::new(
                radius * cos_p * yaw.cos(),
                radius * cos_p * yaw.sin(),
                radius * pitch.sin(),
            );
    }

    /// Move along the view direction; positive = zoom in
    pub fn dolly(&mut self, distance: f32) {
        let dir = (self.target - self.position).normalize_or_zero();
        let candidate = self.position + dir * distance;
        // Preserve a minimum standoff so we never end up at the target
        if (self.target - candidate).length() > 5.0 {
            self.position = candidate;
        }
    }

    /// Frame the camera so `aabb_min..=aabb_max` fits the given aspect
    pub fn frame(&mut self, min: Vec3, max: Vec3, aspect: f32) {
        let center = (min + max) * 0.5;
        let extent = max - min;
        let max_dim = extent.x.max(extent.y).max(extent.z).max(1.0);
        // fov half-angle × distance = half of vertical view -> use adjusted
        // dim so wider aspects don't crop
        let fit_dim = max_dim * 1.4 / aspect.clamp(0.5, 1.0);
        let distance = fit_dim / (self.fov * 0.5).tan();
        // Place camera looking down at 45° from the +X/-Y quadrant so the
        // Z-up orientation is immediately readable (bed grid horizontal)
        let dir = Vec3::new(1.0, -1.2, 0.9).normalize();
        self.target = center;
        self.position = center + dir * distance;
        self.up = Vec3::Z;
    }
}

/// Resources stored in egui's per-frame `CallbackResources` map wgpu
/// pipeline + uploaded mesh + camera snapshot The UI thread mutates the
/// camera via `Arc<Mutex<Camera>>` and the render thread reads it inside
/// `prepare()`
pub struct MeshResources {
    pub pipeline: MeshPipeline,
    pub camera: Arc<Mutex<Camera>>,
}

impl MeshResources {
    pub fn init(render_state: &egui_wgpu::RenderState) -> Self {
        Self {
            pipeline: MeshPipeline::new(&render_state.device, render_state.target_format),
            camera: Arc::new(Mutex::new(Camera::default())),
        }
    }
}

/// egui paint callback that renders the currently uploaded mesh with a
/// depth-tested Phong shader When no mesh is uploaded, the callback is a
/// no-op and the caller's placeholder background remains visible
pub struct MeshRenderCallback;

impl CallbackTrait for MeshRenderCallback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen_descriptor: &ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(resources) = callback_resources.get_mut::<MeshResources>() else {
            return Vec::new();
        };
        let [w, h] = screen_descriptor.size_in_pixels;

        let camera = resources
            .camera
            .lock()
            .map(|c| c.clone())
            .unwrap_or_default();
        let aspect = (w as f32 / h.max(1) as f32).max(0.1);
        let view = Mat4::look_at_rh(camera.position, camera.target, camera.up);
        let proj = Mat4::perspective_rh(camera.fov, aspect, camera.near, camera.far);
        let uniforms = MeshUniforms::from_camera(camera.position, proj * view);
        queue.write_buffer(
            &resources.pipeline.uniform_buffer,
            0,
            bytemuck::cast_slice(&[uniforms]),
        );
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &CallbackResources,
    ) {
        let Some(resources) = callback_resources.get::<MeshResources>() else {
            return;
        };
        let Some(mesh) = &resources.pipeline.mesh else {
            return;
        };
        render_pass.set_pipeline(&resources.pipeline.render_pipeline);
        render_pass.set_bind_group(0, &resources.pipeline.bind_group, &[]);
        render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
        render_pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        render_pass.draw_indexed(0..mesh.index_count, 0, 0..1);
    }
}
