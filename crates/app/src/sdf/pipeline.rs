use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;
use wgpu::*;

/// Depth format for the offscreen preview render pass Must match the
/// depth attachment created by MeshResources::ensure_textures
pub const DEPTH_FORMAT: TextureFormat = TextureFormat::Depth32Float;

/// Color format for the offscreen preview render target We use
/// Rgba8UnormSrgb so that egui's default Image widget displays the
/// texture with correct gamma matching the rest of the UI
pub const COLOR_FORMAT: TextureFormat = TextureFormat::Rgba8UnormSrgb;

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
    /// `format` is ignored — the pipeline always targets [`COLOR_FORMAT`]
    /// since we render to an offscreen texture Kept in the signature for
    /// backward source compat with callers that previously passed
    /// `render_state.target_format`
    pub fn new(device: &Device, _format: TextureFormat) -> Self {
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
                    format: COLOR_FORMAT,
                    blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                // Back-face culling is safe now that we render to an
                // offscreen texture with proper depth testing (the old
                // egui pass had no depth attachment, so we had to disable
                // culling to keep cavity walls visible under blob-out)
                cull_mode: Some(Face::Back),
                polygon_mode: PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            // We now render to an offscreen color+depth texture (see
            // MeshResources::render_frame), so depth testing is available
            // and hollow shapes render with correct front/back occlusion
            depth_stencil: Some(DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: CompareFunction::Less,
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
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

    /// Encode a full offscreen render pass that draws the uploaded mesh
    /// with depth testing to the given color + depth attachments Uploads
    /// current camera uniforms as a side effect
    ///
    /// No-op when no mesh has been uploaded yet
    pub fn render(
        &self,
        device: &Device,
        queue: &Queue,
        color_view: &TextureView,
        depth_view: &TextureView,
        uniforms: MeshUniforms,
    ) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let Some(mesh) = &self.mesh else {
            // Even without a mesh, clear the texture so previous frame
            // doesn't ghost through Callers that need a fresh background
            // rely on this
            let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
                label: Some("mesh_view clear only"),
            });
            {
                let _pass = encoder.begin_render_pass(&RenderPassDescriptor {
                    label: Some("mesh_view clear pass"),
                    color_attachments: &[Some(RenderPassColorAttachment {
                        view: color_view,
                        resolve_target: None,
                        ops: Operations {
                            load: LoadOp::Clear(Color {
                                r: 0.11,
                                g: 0.11,
                                b: 0.13,
                                a: 1.0,
                            }),
                            store: StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                        view: depth_view,
                        depth_ops: Some(Operations {
                            load: LoadOp::Clear(1.0),
                            store: StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
            }
            queue.submit(std::iter::once(encoder.finish()));
            return;
        };

        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("mesh_view offscreen encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("mesh_view offscreen pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: color_view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color {
                            r: 0.11,
                            g: 0.11,
                            b: 0.13,
                            a: 1.0,
                        }),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(Operations {
                        load: LoadOp::Clear(1.0),
                        store: StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.render_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
            pass.set_index_buffer(mesh.index_buffer.slice(..), IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.index_count, 0, 0..1);
        }
        queue.submit(std::iter::once(encoder.finish()));
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
