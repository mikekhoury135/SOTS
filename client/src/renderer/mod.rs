use std::sync::Arc;

use anyhow::Result;
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use winit::dpi::PhysicalSize;
use winit::window::Window;

use crate::state::{DebugSettings, GameView};
use shared::physics;

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

const MAX_VERTICES: usize = 16384;
const VERTEX_SIZE: usize = std::mem::size_of::<Vertex>();

const MAP_HALF: f32 = 100.0;
const TILE_SIZE: f32 = 10.0;
const TILES: i32 = (MAP_HALF * 2.0 / TILE_SIZE) as i32; // 20×20

// ── 3D constants ─────────────────────────────────────────────────────────────

const FOV_Y: f32 = std::f32::consts::FRAC_PI_2; // 90° vertical FOV
const NEAR: f32 = 0.1;
const FAR: f32 = 250.0;

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
    depth_texture: wgpu::TextureView,
    size: PhysicalSize<u32>,
}

impl Renderer {
    pub async fn new(window: Arc<Window>) -> Result<Self> {
        let size = window.inner_size();

        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window)?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await?;

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

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                cull_mode: Some(wgpu::Face::Back),
                front_face: wgpu::FrontFace::Ccw,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let depth_texture = create_depth_texture(&device, size.width.max(1), size.height.max(1));

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            vertex_buf,
            uniform_buf,
            bind_group,
            depth_texture,
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
        self.depth_texture = create_depth_texture(&self.device, new_size.width, new_size.height);
    }

    pub fn reconfigure(&mut self) {
        self.surface.configure(&self.device, &self.config);
        self.depth_texture = create_depth_texture(
            &self.device,
            self.size.width.max(1),
            self.size.height.max(1),
        );
    }

    /// Render one frame. Returns `true` if the surface needs reconfiguring.
    pub fn render(&mut self, game: &GameView, debug: &DebugSettings) -> bool {
        let aspect = self.size.width as f32 / self.size.height.max(1) as f32;

        // Camera from predicted position + yaw
        let eye = Vec3::new(
            game.predicted_pos.x,
            physics::EYE_HEIGHT,
            game.predicted_pos.z,
        );
        let (sin_y, cos_y) = game.predicted_yaw.sin_cos();
        let forward = Vec3::new(sin_y, 0.0, -cos_y); // matches physics convention
        let right = Vec3::new(cos_y, 0.0, sin_y);
        let target = eye + forward;

        let vp = build_view_proj(eye, target, aspect);

        // Build vertex data
        let mut verts: Vec<Vertex> = Vec::with_capacity(6000);
        build_floor(&mut verts);
        build_walls(&mut verts);
        build_players(&mut verts, game);
        build_crosshair(&mut verts, eye, forward, right);

        if debug.show_overlay {
            build_debug_overlay(&mut verts, game, debug, eye, forward, right);
        }

        verts.truncate(MAX_VERTICES);

        self.queue.write_buffer(
            &self.uniform_buf,
            0,
            bytemuck::cast_slice(&vp.to_cols_array()),
        );
        self.queue
            .write_buffer(&self.vertex_buf, 0, bytemuck::cast_slice(&verts));

        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(st)
            | wgpu::CurrentSurfaceTexture::Suboptimal(st) => st,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return false;
            }
            wgpu::CurrentSurfaceTexture::Outdated
            | wgpu::CurrentSurfaceTexture::Lost
            | wgpu::CurrentSurfaceTexture::Validation => return true,
        };

        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Main Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.4,
                            g: 0.6,
                            b: 0.9,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
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

// ── Depth texture ────────────────────────────────────────────────────────────

fn create_depth_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Depth Texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

// ── Camera ────────────────────────────────────────────────────────────────────

fn build_view_proj(eye: Vec3, target: Vec3, aspect: f32) -> Mat4 {
    let view = Mat4::look_at_rh(eye, target, Vec3::Y);
    let proj = Mat4::perspective_rh(FOV_Y, aspect, NEAR, FAR);
    proj * view
}

// ── Geometry helpers ──────────────────────────────────────────────────────────

fn build_floor(verts: &mut Vec<Vertex>) {
    for row in 0..TILES {
        for col in 0..TILES {
            let x0 = -MAP_HALF + col as f32 * TILE_SIZE;
            let z0 = -MAP_HALF + row as f32 * TILE_SIZE;
            let (r, g, b) = if (row + col) % 2 == 0 {
                (0.22_f32, 0.28, 0.22)
            } else {
                (0.16_f32, 0.20, 0.16)
            };
            // Floor quad at y=0, facing up (+Y) — CCW winding from above
            let tl = Vertex::new(x0, 0.0, z0, r, g, b);
            let tr = Vertex::new(x0 + TILE_SIZE, 0.0, z0, r, g, b);
            let bl = Vertex::new(x0, 0.0, z0 + TILE_SIZE, r, g, b);
            let br = Vertex::new(x0 + TILE_SIZE, 0.0, z0 + TILE_SIZE, r, g, b);
            // Two triangles: CCW when viewed from +Y
            verts.extend_from_slice(&[tl, tr, bl, tr, br, bl]);
        }
    }
}

fn build_walls(verts: &mut Vec<Vertex>) {
    let h = physics::WALL_HEIGHT;
    for wall in physics::WALLS {
        let x0 = wall.x_min;
        let x1 = wall.x_max;
        let z0 = wall.z_min;
        let z1 = wall.z_max;
        // Slightly vary color per face for visual depth
        push_box(verts, x0, 0.0, z0, x1, h, z1, 0.45, 0.35, 0.25);
    }
}

fn build_players(verts: &mut Vec<Vertex>, game: &GameView) {
    let half = physics::PLAYER_HALF;
    let ph = physics::PLAYER_HEIGHT;
    for p in &game.players {
        let is_local = game.player_id == Some(p.id);
        if is_local {
            continue; // Don't render the local player's body in first person
        }
        let w = p.position.to_vec3();
        // Orange box for remote players
        push_box(
            verts,
            w.x - half,
            0.0,
            w.z - half,
            w.x + half,
            ph,
            w.z + half,
            1.0,
            0.5,
            0.0,
        );
    }
}

fn build_crosshair(verts: &mut Vec<Vertex>, eye: Vec3, forward: Vec3, right: Vec3) {
    // Small white cross placed 2 units in front of the camera
    let center = eye + forward * 2.0;
    let up = Vec3::Y;
    let arm = 0.02_f32; // half-size of each arm
    let thick = 0.003_f32; // half-thickness

    // Horizontal bar
    push_flat_quad(
        verts,
        center - right * arm - up * thick,
        right * arm * 2.0,
        up * thick * 2.0,
        1.0,
        1.0,
        1.0,
    );

    // Vertical bar
    push_flat_quad(
        verts,
        center - right * thick - up * arm,
        right * thick * 2.0,
        up * arm * 2.0,
        1.0,
        1.0,
        1.0,
    );
}

#[allow(clippy::too_many_arguments)]
fn build_debug_overlay(
    verts: &mut Vec<Vertex>,
    game: &GameView,
    debug: &DebugSettings,
    eye: Vec3,
    forward: Vec3,
    right: Vec3,
) {
    let up = Vec3::Y;

    // Server ghost: red box at server-confirmed position
    let s = game.server_pos;
    let half = physics::PLAYER_HALF * 0.8;
    let ph = physics::PLAYER_HEIGHT;
    push_box(
        verts,
        s.x - half,
        0.0,
        s.z - half,
        s.x + half,
        ph,
        s.z + half,
        0.8,
        0.2,
        0.2,
    );

    // HUD bars placed in front of camera, offset to the bottom-left of view
    let hud_base = eye + forward * 1.5 - right * 0.6 - up * 0.4;

    // RTT bar
    let bar_width = (game.rtt_ms / 200.0).min(1.0) * 0.3;
    let (r, g, b) = if game.rtt_ms < 30.0 {
        (0.0_f32, 0.8, 0.2)
    } else if game.rtt_ms < 100.0 {
        (0.9_f32, 0.8, 0.0)
    } else {
        (0.9_f32, 0.2, 0.1)
    };
    push_flat_quad(
        verts,
        hud_base,
        right * bar_width.max(0.02),
        up * 0.015,
        r,
        g,
        b,
    );

    // Pending inputs bar (below RTT bar)
    let pending = game.pending_inputs.min(20) as f32;
    let pending_w = pending * 0.015;
    push_flat_quad(
        verts,
        hud_base - up * 0.025,
        right * pending_w.max(0.005),
        up * 0.01,
        0.5,
        0.5,
        0.9,
    );

    // Simulated latency bar
    if debug.simulated_latency_ms > 0 {
        let lat_w = debug.simulated_latency_ms as f32 / 200.0 * 0.3;
        push_flat_quad(
            verts,
            hud_base - up * 0.045,
            right * lat_w,
            up * 0.01,
            0.9,
            0.4,
            0.9,
        );
    }
}

// ── Box builder (5 faces — no bottom) ────────────────────────────────────────

/// Push a 3D box from (x0,y0,z0) to (x1,y1,z1). CCW winding facing outward.
/// Slightly darkens side faces for visual depth.
#[allow(clippy::too_many_arguments)]
fn push_box(
    verts: &mut Vec<Vertex>,
    x0: f32,
    y0: f32,
    z0: f32,
    x1: f32,
    y1: f32,
    z1: f32,
    r: f32,
    g: f32,
    b: f32,
) {
    let d = 0.75_f32; // darkening factor for side faces

    // Top face (y = y1, facing +Y) — CCW from above
    push_face(
        verts,
        Vertex::new(x0, y1, z0, r, g, b),
        Vertex::new(x1, y1, z0, r, g, b),
        Vertex::new(x1, y1, z1, r, g, b),
        Vertex::new(x0, y1, z1, r, g, b),
    );

    // Front face (z = z0, facing -Z) — CCW from -Z
    push_face(
        verts,
        Vertex::new(x0, y0, z0, r * d, g * d, b * d),
        Vertex::new(x0, y1, z0, r * d, g * d, b * d),
        Vertex::new(x1, y1, z0, r * d, g * d, b * d),
        Vertex::new(x1, y0, z0, r * d, g * d, b * d),
    );

    // Back face (z = z1, facing +Z) — CCW from +Z
    push_face(
        verts,
        Vertex::new(x1, y0, z1, r * d, g * d, b * d),
        Vertex::new(x1, y1, z1, r * d, g * d, b * d),
        Vertex::new(x0, y1, z1, r * d, g * d, b * d),
        Vertex::new(x0, y0, z1, r * d, g * d, b * d),
    );

    let d2 = 0.6_f32; // even darker for left/right

    // Left face (x = x0, facing -X) — CCW from -X
    push_face(
        verts,
        Vertex::new(x0, y0, z1, r * d2, g * d2, b * d2),
        Vertex::new(x0, y1, z1, r * d2, g * d2, b * d2),
        Vertex::new(x0, y1, z0, r * d2, g * d2, b * d2),
        Vertex::new(x0, y0, z0, r * d2, g * d2, b * d2),
    );

    // Right face (x = x1, facing +X) — CCW from +X
    push_face(
        verts,
        Vertex::new(x1, y0, z0, r * d2, g * d2, b * d2),
        Vertex::new(x1, y1, z0, r * d2, g * d2, b * d2),
        Vertex::new(x1, y1, z1, r * d2, g * d2, b * d2),
        Vertex::new(x1, y0, z1, r * d2, g * d2, b * d2),
    );
}

/// Push one quad face as 2 CCW triangles. Vertices must be in CCW order when
/// viewed from the outside.
fn push_face(verts: &mut Vec<Vertex>, v0: Vertex, v1: Vertex, v2: Vertex, v3: Vertex) {
    verts.extend_from_slice(&[v0, v1, v2, v0, v2, v3]);
}

/// Push a flat quad defined by an origin, width-vector, and height-vector.
/// Faces toward the camera (billboard).
fn push_flat_quad(
    verts: &mut Vec<Vertex>,
    origin: Vec3,
    width: Vec3,
    height: Vec3,
    r: f32,
    g: f32,
    b: f32,
) {
    let v0 = origin;
    let v1 = origin + width;
    let v2 = origin + width + height;
    let v3 = origin + height;
    verts.extend_from_slice(&[
        Vertex::new(v0.x, v0.y, v0.z, r, g, b),
        Vertex::new(v1.x, v1.y, v1.z, r, g, b),
        Vertex::new(v2.x, v2.y, v2.z, r, g, b),
        Vertex::new(v0.x, v0.y, v0.z, r, g, b),
        Vertex::new(v2.x, v2.y, v2.z, r, g, b),
        Vertex::new(v3.x, v3.y, v3.z, r, g, b),
    ]);
}
