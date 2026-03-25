use std::sync::Arc;

use anyhow::Result;
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use winit::dpi::PhysicalSize;
use winit::window::Window;

use crate::state::GameView;

// ── Vertex layout ─────────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
}

impl Vertex {
    fn new(x: f32, y: f32, z: f32, r: f32, g: f32, b: f32) -> Self {
        Self {
            position: [x, y, z],
            color: [r, g, b],
        }
    }
}

const MAX_VERTICES: usize = 8192;
const VERTEX_SIZE: usize = std::mem::size_of::<Vertex>();

const MAP_HALF: f32 = 100.0;
const TILE_SIZE: f32 = 10.0;
const TILES: i32 = (MAP_HALF * 2.0 / TILE_SIZE) as i32; // 20×20

const CAM_HEIGHT: f32 = 50.0;
const VIEW_HALF: f32 = 20.0; // world-units from screen center to edge

// ── Renderer ─────────────────────────────────────────────────────────────────

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    vertex_buf: wgpu::Buffer,
    uniform_buf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    size: PhysicalSize<u32>,
}

impl Renderer {
    pub async fn new(window: Arc<Window>) -> Result<Self> {
        let size = window.inner_size();

        // Instance::default() uses InstanceDescriptor::new_without_display_handle()
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window)?;

        // In wgpu 29, request_adapter returns Result<Adapter, RequestAdapterError>
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await?;

        // In wgpu 29, request_device takes 1 argument (no trace path)
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await?;

        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| anyhow::anyhow!("Surface not supported by this adapter"))?;
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SOTS Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniform Buffer"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Uniform BG"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });

        let vertex_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vertex Buffer"),
            size: (MAX_VERTICES * VERTEX_SIZE) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // wgpu 29: PipelineLayoutDescriptor uses `immediate_size` (not push_constant_ranges)
        //          bind_group_layouts is &[Option<&BindGroupLayout>]
        let pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(&bgl)],
                immediate_size: 0,
            });

        // wgpu 29: RenderPipelineDescriptor uses `multiview_mask` (not `multiview`)
        let pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Main Pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: VERTEX_SIZE as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![
                            0 => Float32x3,
                            1 => Float32x3
                        ],
                    }],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: config.format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            vertex_buf,
            uniform_buf,
            bind_group,
            size,
        })
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.size = new_size;
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn reconfigure(&mut self) {
        self.surface.configure(&self.device, &self.config);
    }

    /// Render one frame.
    ///
    /// Returns `true` if the surface needs reconfiguring (call `reconfigure()`).
    /// Returns `false` on success or a gracefully-skipped frame (timeout/occluded).
    pub fn render(&mut self, game: &GameView) -> bool {
        // Build vertex data
        let mut verts: Vec<Vertex> = Vec::with_capacity(3000);
        build_floor(&mut verts);
        build_players(&mut verts, game);
        verts.truncate(MAX_VERTICES);

        // Update camera uniform
        let cam = local_player_pos(game);
        let aspect = self.size.width as f32 / self.size.height.max(1) as f32;
        let vp = build_view_proj(cam.x, cam.z, aspect);
        self.queue.write_buffer(
            &self.uniform_buf,
            0,
            bytemuck::cast_slice(&vp.to_cols_array()),
        );

        // Upload vertex data
        self.queue
            .write_buffer(&self.vertex_buf, 0, bytemuck::cast_slice(&verts));

        // wgpu 29: get_current_texture() returns CurrentSurfaceTexture (an enum)
        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(st)
            | wgpu::CurrentSurfaceTexture::Suboptimal(st) => st,
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded => return false, // skip frame
            wgpu::CurrentSurfaceTexture::Outdated
            | wgpu::CurrentSurfaceTexture::Lost
            | wgpu::CurrentSurfaceTexture::Validation => return true, // reconfigure
        };

        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            // wgpu 29: RenderPassDescriptor requires `multiview_mask`;
            //          RenderPassColorAttachment requires `depth_slice`
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Main Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.05,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, Some(&self.bind_group), &[]);
            pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
            pass.draw(0..verts.len() as u32, 0..1);
        }

        self.queue.submit([encoder.finish()]);
        surface_texture.present();
        false
    }
}

// ── Geometry helpers ──────────────────────────────────────────────────────────

fn build_floor(verts: &mut Vec<Vertex>) {
    for row in 0..TILES {
        for col in 0..TILES {
            let x0 = -MAP_HALF + col as f32 * TILE_SIZE;
            let z0 = -MAP_HALF + row as f32 * TILE_SIZE;
            let (r, g, b) = if (row + col) % 2 == 0 {
                (0.18_f32, 0.22, 0.18)
            } else {
                (0.13_f32, 0.16, 0.13)
            };
            push_quad(verts, x0, z0, x0 + TILE_SIZE, z0 + TILE_SIZE, 0.0, r, g, b);
        }
    }
}

fn build_players(verts: &mut Vec<Vertex>, game: &GameView) {
    const HALF: f32 = 1.0;
    for p in &game.players {
        let w = p.position.to_vec3();
        let is_local = game.player_id == Some(p.id);
        let (r, g, b) = if is_local {
            (0.0_f32, 0.9, 0.9) // cyan — local player
        } else {
            (1.0_f32, 0.5, 0.0) // orange — remote players
        };
        push_quad(
            verts,
            w.x - HALF,
            w.z - HALF,
            w.x + HALF,
            w.z + HALF,
            0.01, // just above the floor
            r,
            g,
            b,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn push_quad(
    verts: &mut Vec<Vertex>,
    x0: f32, z0: f32, x1: f32, z1: f32,
    y: f32,
    r: f32, g: f32, b: f32,
) {
    let tl = Vertex::new(x0, y, z0, r, g, b);
    let tr = Vertex::new(x1, y, z0, r, g, b);
    let bl = Vertex::new(x0, y, z1, r, g, b);
    let br = Vertex::new(x1, y, z1, r, g, b);
    verts.extend_from_slice(&[tl, bl, tr, tr, bl, br]);
}

// ── Camera ────────────────────────────────────────────────────────────────────

fn build_view_proj(cam_x: f32, cam_z: f32, aspect: f32) -> Mat4 {
    // Top-down orthographic: camera floats directly above the player, looks straight down.
    // Up vector = world -Z so that north (-Z) renders at the top of the screen.
    let view = Mat4::look_at_rh(
        Vec3::new(cam_x, CAM_HEIGHT, cam_z),
        Vec3::new(cam_x, 0.0, cam_z),
        Vec3::NEG_Z,
    );
    let hw = VIEW_HALF * aspect;
    let hh = VIEW_HALF;
    let proj = Mat4::orthographic_rh(-hw, hw, -hh, hh, 1.0, CAM_HEIGHT + 10.0);
    proj * view
}

fn local_player_pos(game: &GameView) -> Vec3 {
    game.player_id
        .and_then(|id| game.players.iter().find(|p| p.id == id))
        .map(|p| p.position.to_vec3())
        .unwrap_or(Vec3::ZERO)
}
