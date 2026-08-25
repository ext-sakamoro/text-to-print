use egui::TextureId;
use glam::{Mat4, Vec3};
use std::sync::{Arc, Mutex};

use super::pipeline::{MeshPipeline, MeshUniforms, COLOR_FORMAT, DEPTH_FORMAT};

/// Camera state shared between the UI (for orbit / dolly / reset) and the
/// offscreen render pass
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

/// Offscreen render target: color + depth textures at a given size
/// Recreated when the viewport size changes
struct OffscreenTargets {
    size: (u32, u32),
    /// Color texture view — displayed via egui::Image
    color_view: wgpu::TextureView,
    /// Depth texture view — used by the render pass, never displayed
    depth_view: wgpu::TextureView,
    /// egui-registered TextureId of the color view
    egui_id: TextureId,
}

/// Mesh preview render resources: wgpu pipeline + shared camera +
/// offscreen render targets All state needed to render one frame of the
/// mesh preview into a texture that egui can then display via Image
pub struct MeshResources {
    pub pipeline: MeshPipeline,
    pub camera: Arc<Mutex<Camera>>,
    /// Lazily created / recreated on viewport size change
    targets: Option<OffscreenTargets>,
}

impl MeshResources {
    pub fn init(render_state: &egui_wgpu::RenderState) -> Self {
        Self {
            pipeline: MeshPipeline::new(&render_state.device, render_state.target_format),
            camera: Arc::new(Mutex::new(Camera::default())),
            targets: None,
        }
    }

    /// Ensure offscreen color+depth textures exist at the requested size
    /// Recreates them (and re-registers the color view with egui) whenever
    /// the size changes, and unregisters the previous egui texture id
    ///
    /// Caller must hold the write guard on the egui renderer (this is why
    /// `renderer` is passed by `&mut` rather than acquiring internally —
    /// std RwLock is not re-entrant, so nested `.write()` would deadlock)
    pub fn ensure_targets(
        &mut self,
        device: &wgpu::Device,
        renderer: &mut egui_wgpu::Renderer,
        size: (u32, u32),
    ) {
        if let Some(t) = &self.targets
            && t.size == size
        {
            return;
        }

        let color_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mesh_view offscreen color"),
            size: wgpu::Extent3d {
                width: size.0.max(1),
                height: size.1.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let color_view = color_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let depth_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mesh_view offscreen depth"),
            size: wgpu::Extent3d {
                width: size.0.max(1),
                height: size.1.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_tex.create_view(&wgpu::TextureViewDescriptor::default());

        // Free the previous egui texture id (if any) before registering
        // the new one, so egui's texture atlas doesn't leak
        if let Some(prev) = self.targets.take() {
            renderer.free_texture(&prev.egui_id);
        }
        let egui_id =
            renderer.register_native_texture(device, &color_view, wgpu::FilterMode::Linear);

        self.targets = Some(OffscreenTargets {
            size,
            color_view,
            depth_view,
            egui_id,
        });
    }

    /// Render one frame of the mesh preview to the offscreen color+depth
    /// texture, returning the egui TextureId that displays the result
    /// via egui::Image
    ///
    /// Returns `None` when no mesh has been uploaded yet or when no
    /// targets have been created Caller must have called
    /// [`Self::ensure_targets`] at the current viewport size before
    /// invoking this (typically in the same write-lock scope on
    /// `render_state.renderer`)
    pub fn render_frame(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Option<TextureId> {
        self.pipeline.mesh.as_ref()?;
        let targets = self.targets.as_ref()?;

        let camera = self
            .camera
            .lock()
            .map(|c| c.clone())
            .unwrap_or_default();
        let aspect = (targets.size.0 as f32 / targets.size.1.max(1) as f32).max(0.1);
        let view = Mat4::look_at_rh(camera.position, camera.target, camera.up);
        let proj = Mat4::perspective_rh(camera.fov, aspect, camera.near, camera.far);
        let uniforms = MeshUniforms::from_camera(camera.position, proj * view);

        self.pipeline
            .render(device, queue, &targets.color_view, &targets.depth_view, uniforms);

        Some(targets.egui_id)
    }
}
