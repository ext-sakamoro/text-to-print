use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;
use wgpu::*;

/// WGSL shader for basic Phong-lit mesh preview
///
/// - Vertex: MVP transform, pass world position + normal to fragment
/// - Fragment: ambient + diffuse + specular with a single directional light
/// - Depth-tested, no alpha blending (opaque solid)
const MESH_SHADER: &str = r#"
struct Uniforms {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
    camera_pos: vec4<f32>,
    light_dir: vec4<f32>,
    base_color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    let world = u.model * vec4<f32>(in.position, 1.0);
    var out: VsOut;
    out.clip_pos = u.view_proj * world;
    out.world_pos = world.xyz;
    out.world_normal = (u.model * vec4<f32>(in.normal, 0.0)).xyz;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);
    let l = normalize(u.light_dir.xyz);
    let v = normalize(u.camera_pos.xyz - in.world_pos);
    let h = normalize(l + v);

    let ambient = 0.20;
    let diffuse = max(dot(n, l), 0.0);
    let specular = pow(max(dot(n, h), 0.0), 32.0) * 0.35;

    // Add a subtle rim light so back faces / silhouettes are still readable
    let rim = pow(1.0 - max(dot(n, v), 0.0), 3.0) * 0.15;

    let lit = u.base_color.rgb * (ambient + diffuse) + vec3<f32>(specular) + vec3<f32>(rim);
    return vec4<f32>(lit, u.base_color.a);
}
"#;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MeshUniforms {
    pub view_proj: [[f32; 4]; 4],
    pub model: [[f32; 4]; 4],
    pub camera_pos: [f32; 4],
    pub light_dir: [f32; 4],
    pub base_color: [f32; 4],
}

impl MeshUniforms {
    #[must_use]
    pub fn from_camera(camera_pos: Vec3, view_proj: Mat4) -> Self {
        Self {
            view_proj: view_proj.to_cols_array_2d(),
            model: Mat4::IDENTITY.to_cols_array_2d(),
            camera_pos: [camera_pos.x, camera_pos.y, camera_pos.z, 1.0],
            // Light comes from upper-front-right, matches the Bambu Studio
            // default preview convention closely enough for shape reading
            light_dir: [0.4, -0.5, 0.8, 0.0],
            base_color: [0.82, 0.82, 0.87, 1.0],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

pub struct MeshPipeline {
    pub render_pipeline: RenderPipeline,
    pub uniform_buffer: Buffer,
    pub bind_group: BindGroup,
    /// Current uploaded mesh — `None` when nothing has been loaded yet
    pub mesh: Option<UploadedMesh>,
}

pub struct UploadedMesh {
    pub vertex_buffer: Buffer,
    pub index_buffer: Buffer,
    pub index_count: u32,
}

impl MeshPipeline {
    pub fn new(device: &Device, format: TextureFormat) -> Self {
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("mesh_view shader"),
            source: ShaderSource::Wgsl(MESH_SHADER.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("mesh_view bgl"),
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
            label: Some("mesh_view uniform"),
            size: std::mem::size_of::<MeshUniforms>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("mesh_view bg"),
            layout: &bind_group_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("mesh_view pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let vertex_layout = VertexBufferLayout {
            array_stride: std::mem::size_of::<MeshVertex>() as u64,
            step_mode: VertexStepMode::Vertex,
            attributes: &[
                VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: VertexFormat::Float32x3,
                },
                VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: VertexFormat::Float32x3,
                },
            ],
        };

        let render_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("mesh_view pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[vertex_layout],
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
                // Mesh from marching cubes / dual contouring can flip
                // winding on internal cavity surfaces; disable culling so
                // the whole shape is always visible regardless of normal
                cull_mode: None,
                polygon_mode: PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            // egui's paint callback runs inside egui's own render pass
            // which has no depth attachment, so a depth-stencil target here
            // would fail validation Preview shows slight front/back sort
            // artifacts as a result — acceptable trade-off for the ability
            // to plug straight into egui without an offscreen texture pass
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            render_pipeline,
            uniform_buffer,
            bind_group,
            mesh: None,
        }
    }

    /// Upload a mesh's vertex / index data to GPU buffers Replaces any
    /// existing uploaded mesh
    pub fn upload_mesh(&mut self, device: &Device, mesh: &alice_sdf::mesh::Mesh) {
        let verts: Vec<MeshVertex> = mesh
            .vertices
            .iter()
            .map(|v| MeshVertex {
                position: v.position.into(),
                normal: v.normal.into(),
            })
            .collect();

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh_view vertex"),
            contents: bytemuck::cast_slice(&verts),
            usage: BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh_view index"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: BufferUsages::INDEX,
        });
        self.mesh = Some(UploadedMesh {
            vertex_buffer,
            index_buffer,
            index_count: mesh.indices.len() as u32,
        });
    }
}
