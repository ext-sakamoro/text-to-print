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

/// WGSL shader for slicer-style flat-shaded mesh preview
///
/// - Vertex: MVP transform, pass world position (normal input kept for
///   layout compatibility but unused — we re-derive face normal in the
///   fragment shader for flat shading)
/// - Fragment: flat normal via `cross(dpdx(world_pos), dpdy(world_pos))`
///   with `faceforward`-style viewer alignment (Y-flipped projection の
///   cross 符号逆転を defensive fix、2026-09-06)、Half-Lambert diffuse で
///   dark side が真っ黒にならず shape 読み取り可、specular は subtle に
///   Bambu Studio 標準の matte gray aesthetic に合わせて base_color 高輝度 +
///   rim light の合成で 1.0 超え white 飽和していた旧 formula から改修
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
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    let world = u.model * vec4<f32>(in.position, 1.0);
    var out: VsOut;
    out.clip_pos = u.view_proj * world;
    out.world_pos = world.xyz;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Flat normal from screen-space derivatives (per-vertex normal 噪音回避)
    var n = normalize(cross(dpdx(in.world_pos), dpdy(in.world_pos)));
    let v = normalize(u.camera_pos.xyz - in.world_pos);
    // Defensive: Y-flipped projection や CW/CCW winding 差異で cross の
    // 符号が反転する case を吸収 常に viewer 向きの normal に揃える
    if (dot(n, v) < 0.0) {
        n = -n;
    }

    let l = normalize(u.light_dir.xyz);
    let h = normalize(l + v);

    // Half-Lambert diffuse: 0.5 + 0.5*dot → 常に [0, 1] range
    // dark side が真っ黒にならず shape 読み取り可 (Valve / Bambu 系)
    let diffuse = 0.5 + 0.5 * dot(n, l);
    // Subtle specular (滑らかな面のみ highlight、over-bright しない)
    let specular = pow(max(dot(n, h), 0.0), 24.0) * 0.15;

    // Max: base_color * 1.0 + 0.15 = ~0.97 (white 飽和なし)
    // Min: base_color * 0.0 + 0.0 = 黒 (実際は Half-Lambert で 0 未到達)
    let color = u.base_color.rgb * diffuse + vec3<f32>(specular);
    return vec4<f32>(color, u.base_color.a);
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
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
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
                buffers: &[Some(vertex_layout)],
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
                depth_write_enabled: Some(true),
                depth_compare: Some(CompareFunction::Less),
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState::default(),
            multiview_mask: None,
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
                        depth_slice: None,
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
                    multiview_mask: None,
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
                    depth_slice: None,
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
                multiview_mask: None,
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

/// 3D 表示の pipeline を実際の GPU で 1 frame 描き、読み戻して確かめる
///
/// GPU の adapter が要るので既定では走らせない (`--ignored`)
/// `T2P_GPU_REQUIRE_VENDOR` (例: `0x10de`) を渡すと、adapter の vendor がそれでなければ
/// 失敗する (software の描画 (llvmpipe など) で通ったことにしない)
#[cfg(test)]
mod gpu_tests {
    use super::{COLOR_FORMAT, DEPTH_FORMAT, MeshPipeline, MeshUniforms};
    use glam::{Mat4, Vec3};

    const SIZE: u32 = 64;

    fn device() -> (wgpu::Device, wgpu::Queue, wgpu::AdapterInfo) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
        }))
        .expect("no GPU adapter");
        let info = adapter.get_info();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .expect("request_device");
        (device, queue, info)
    }

    fn triangle() -> alice_sdf::mesh::Mesh {
        let n = Vec3::Z;
        let mut mesh = alice_sdf::mesh::Mesh::new();
        // counter-clockwise seen from +z (the camera), so back-face culling keeps it
        for p in [
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ] {
            mesh.vertices.push(alice_sdf::mesh::Vertex::new(p, n));
        }
        mesh.indices = vec![0, 1, 2];
        mesh
    }

    fn render(with_mesh: bool) -> (Vec<u8>, wgpu::AdapterInfo) {
        let (device, queue, info) = device();
        let mut pipeline = MeshPipeline::new(&device, COLOR_FORMAT);
        if with_mesh {
            pipeline.upload_mesh(&device, &triangle());
        }
        let extent = wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        };
        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gpu test color"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gpu test depth"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

        let eye = Vec3::new(0.0, 0.0, 3.0);
        let view = Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y);
        let proj = Mat4::perspective_rh(45f32.to_radians(), 1.0, 0.1, 100.0);
        pipeline.render(
            &device,
            &queue,
            &color_view,
            &depth_view,
            MeshUniforms::from_camera(eye, proj * view),
        );

        let row = SIZE * 4; // 256, already a multiple of COPY_BYTES_PER_ROW_ALIGNMENT
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu test readback"),
            size: u64::from(row * SIZE),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_buffer(
            color.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(SIZE),
                },
            },
            extent,
        );
        queue.submit(std::iter::once(encoder.finish()));
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |r| r.expect("map"));
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("poll");
        let bytes = slice.get_mapped_range().expect("mapped range").to_vec();
        (bytes, info)
    }

    fn pixel(bytes: &[u8], x: u32, y: u32) -> [u8; 4] {
        let i = ((y * SIZE + x) * 4) as usize;
        [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]
    }

    fn near(a: [u8; 4], b: [u8; 4]) -> bool {
        a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 2)
    }

    fn check_vendor(info: &wgpu::AdapterInfo) {
        println!(
            "[t2p gpu-test] adapter name={} vendor=0x{:04x} device=0x{:04x} type={:?} backend={:?}",
            info.name, info.vendor, info.device, info.device_type, info.backend
        );
        if let Ok(want) = std::env::var("T2P_GPU_REQUIRE_VENDOR") {
            let want = u32::from_str_radix(want.trim_start_matches("0x"), 16)
                .expect("T2P_GPU_REQUIRE_VENDOR is hex");
            assert_eq!(info.vendor, want, "adapter vendor is not the required one");
        }
    }

    /// 背景は mesh の無い frame から読む (clear の色を sRGB に直す計算を試験に持たない)
    #[test]
    #[ignore = "needs a GPU adapter (run with --ignored on a GPU machine)"]
    fn the_mesh_pipeline_draws_a_triangle_over_the_clear_colour() {
        let (empty, info) = render(false);
        check_vendor(&info);
        let clear = pixel(&empty, SIZE / 2, SIZE / 2);
        assert!(
            (0..SIZE).all(|y| (0..SIZE).all(|x| pixel(&empty, x, y) == clear)),
            "the frame without a mesh is not one colour"
        );
        let (bytes, _) = render(true);
        let centre = pixel(&bytes, SIZE / 2, SIZE / 2);
        let corner = pixel(&bytes, 1, 1);
        let covered = (0..SIZE)
            .flat_map(|y| (0..SIZE).map(move |x| (x, y)))
            .filter(|&(x, y)| !near(pixel(&bytes, x, y), clear))
            .count();
        println!(
            "[t2p gpu-test] clear={clear:?} centre={centre:?} corner={corner:?} covered={covered}/{}",
            SIZE * SIZE
        );
        assert!(
            near(corner, clear),
            "corner {corner:?} is not the clear colour {clear:?}"
        );
        assert!(!near(centre, clear), "centre {centre:?} shows no triangle");
        assert_eq!(centre[3], 0xff);
        // the triangle spans x in [-1, 1], y in [-1, 1] at distance 3 with a 45 degree fov:
        // a quarter of the frame or so, never all of it
        assert!(covered > 200 && covered < 3000, "covered {covered} pixels");
    }
}
