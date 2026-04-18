use egui_wgpu::{CallbackResources, CallbackTrait, ScreenDescriptor};
use std::time::Instant;

use super::pipeline::{SdfPipeline, SdfUniforms};

/// egui CallbackResources に格納する SDF リソース
pub struct SdfResources {
    pub pipeline: SdfPipeline,
    pub start_time: Instant,
    pub camera_pos: [f32; 3],
    pub camera_target: [f32; 3],
    pub camera_up: [f32; 3],
    pub camera_fov: f32,
}

impl SdfResources {
    pub fn init(render_state: &egui_wgpu::RenderState) -> Self {
        let device = &render_state.device;
        let format = render_state.target_format;
        let pipeline = SdfPipeline::new(device, format);

        Self {
            pipeline,
            start_time: Instant::now(),
            camera_pos: [0.0, 0.0, 5.0],
            camera_target: [0.0, 0.0, 0.0],
            camera_up: [0.0, 1.0, 0.0],
            camera_fov: std::f32::consts::FRAC_PI_4,
        }
    }

    pub fn rebuild_with_wgsl(&mut self, device: &wgpu::Device, wgsl: &str) {
        self.pipeline = self.pipeline.rebuild_with_dynamic_sdf(device, wgsl);
    }
}

/// SDF レンダリング用の egui PaintCallback
pub struct SdfRenderCallback;

impl CallbackTrait for SdfRenderCallback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen_descriptor: &ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(resources) = callback_resources.get::<SdfResources>() {
            let time = resources.start_time.elapsed().as_secs_f32();
            let [w, h] = screen_descriptor.size_in_pixels;

            let uniforms = SdfUniforms {
                resolution: [w as f32, h as f32],
                time,
                _pad0: 0.0,
                camera_pos: [
                    resources.camera_pos[0],
                    resources.camera_pos[1],
                    resources.camera_pos[2],
                    0.0,
                ],
                camera_target: [
                    resources.camera_target[0],
                    resources.camera_target[1],
                    resources.camera_target[2],
                    resources.camera_fov,
                ],
                camera_up: [
                    resources.camera_up[0],
                    resources.camera_up[1],
                    resources.camera_up[2],
                    0.0,
                ],
                max_steps: 128,
                max_distance: 100.0,
                epsilon: 0.001,
                flags: 2, // AO on
                scene_id: 0,
                light_intensity: 1.0,
                ambient_intensity: 0.15,
                quality_flags: 1, // adaptive quality
                light_dir: [0.5, 1.0, 0.3, 0.0],
                bg_color: [0.02, 0.02, 0.05, 1.0],
            };

            queue.write_buffer(
                &resources.pipeline.uniform_buffer,
                0,
                bytemuck::cast_slice(&[uniforms]),
            );
        }

        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &CallbackResources,
    ) {
        if let Some(resources) = callback_resources.get::<SdfResources>() {
            render_pass.set_pipeline(&resources.pipeline.render_pipeline);
            render_pass.set_bind_group(0, &resources.pipeline.bind_group, &[]);
            render_pass.draw(0..3, 0..1);
        }
    }
}
