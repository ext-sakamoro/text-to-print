use wgpu::*;

const RAYMARCHING_TEMPLATE: &str = include_str!("raymarching.wgsl");

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SdfUniforms {
    pub resolution: [f32; 2],
    pub time: f32,
    pub _pad0: f32,
    pub camera_pos: [f32; 4],
    pub camera_target: [f32; 4],
    pub camera_up: [f32; 4],
    pub max_steps: u32,
    pub max_distance: f32,
    pub epsilon: f32,
    pub flags: u32,
    pub scene_id: u32,
    pub light_intensity: f32,
    pub ambient_intensity: f32,
    pub quality_flags: u32,
    pub light_dir: [f32; 4],
    pub bg_color: [f32; 4],
}

pub struct SdfPipeline {
    pub render_pipeline: RenderPipeline,
    pub uniform_buffer: Buffer,
    pub bind_group: BindGroup,
    format: TextureFormat,
}

impl SdfPipeline {
    pub fn new(device: &Device, format: TextureFormat) -> Self {
        Self::new_with_shader(device, format, RAYMARCHING_TEMPLATE)
    }

    fn new_with_shader(device: &Device, format: TextureFormat, shader_source: &str) -> Self {
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("SDF Raymarching Shader"),
            source: ShaderSource::Wgsl(shader_source.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("SDF Bind Group Layout"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let uniform_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("SDF Uniform Buffer"),
            size: std::mem::size_of::<SdfUniforms>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("SDF Bind Group"),
            layout: &bind_group_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("SDF Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("SDF Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(ColorTargetState {
                    format,
                    blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            render_pipeline,
            uniform_buffer,
            bind_group,
            format,
        }
    }

    pub fn rebuild_with_dynamic_sdf(&self, device: &Device, sdf_wgsl: &str) -> Self {
        let (helpers, body, material_fn) = split_helpers_body_material(sdf_wgsl);

        let material_section = if material_fn.is_empty() {
            "fn sdf_material_dynamic(p: vec3<f32>) -> f32 { return 0.0; }".to_string()
        } else {
            material_fn.replace("sdf_eval_material", "sdf_material_dynamic")
        };

        let dynamic_function = format!(
            "{helpers}\n\
             fn sdf_eval_dynamic(p: vec3<f32>) -> f32 {{\n\
             {body}\n\
             }}\n\n\
             {material_section}",
        );

        let shader_source = RAYMARCHING_TEMPLATE.replace(
            "// {{DYNAMIC_SDF_FUNCTION}}\n// Default fallback when no .asdf is loaded\nfn sdf_eval_dynamic(p: vec3<f32>) -> f32 {\n    return length(p) - 1.0;  // Simple sphere fallback\n}\nfn sdf_material_dynamic(p: vec3<f32>) -> f32 {\n    return 0.0;\n}",
            &dynamic_function,
        );

        Self::new_with_shader(device, self.format, &shader_source)
    }
}

fn split_helpers_body_material(sdf_wgsl: &str) -> (String, String, String) {
    if let Some(eval_pos) = sdf_wgsl.find("fn sdf_eval(") {
        let helpers = sdf_wgsl[..eval_pos].trim().to_string();
        let after_helpers = &sdf_wgsl[eval_pos..];

        let mut brace_depth = 0i32;
        let mut eval_end = after_helpers.len();
        let mut found_start = false;
        for (i, c) in after_helpers.char_indices() {
            if c == '{' {
                brace_depth += 1;
                found_start = true;
            } else if c == '}' {
                brace_depth -= 1;
                if found_start && brace_depth == 0 {
                    eval_end = i + 1;
                    break;
                }
            }
        }

        let eval_fn = &after_helpers[..eval_end];
        let material_section = after_helpers[eval_end..].trim().to_string();

        if let Some(start) = eval_fn.find('{') {
            let body = eval_fn[start + 1..eval_end - 1].trim().to_string();
            return (helpers, body, material_section);
        }

        (helpers, eval_fn.to_string(), material_section)
    } else {
        (String::new(), sdf_wgsl.to_string(), String::new())
    }
}
