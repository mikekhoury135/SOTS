use std::sync::Arc;
use std::time::Duration;

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

const MAX_VERTICES: usize = 32768;
const VERTEX_SIZE: usize = std::mem::size_of::<Vertex>();

const MAP_HALF: f32 = 100.0;
const TILE_SIZE: f32 = 10.0;
const TILES: i32 = (MAP_HALF * 2.0 / TILE_SIZE) as i32; // 20×20

// ── 3D constants ─────────────────────────────────────────────────────────────

const FOV_Y: f32 = std::f32::consts::FRAC_PI_2; // 90° vertical FOV
const NEAR: f32 = 0.05;
const FAR: f32 = 250.0;

/// How long to show the shot flash indicator.
const SHOT_FLASH_DURATION: Duration = Duration::from_millis(500);

// Wire thickness for debug hitbox outlines.
const WIRE_T: f32 = 0.04;

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

        let eye = Vec3::new(
            game.predicted_pos.x,
            game.predicted_pos.y + physics::EYE_HEIGHT,
            game.predicted_pos.z,
        );
        let (sin_y, cos_y) = game.predicted_yaw.sin_cos();
        let forward = Vec3::new(sin_y, 0.0, -cos_y);
        let right = Vec3::new(cos_y, 0.0, sin_y);
        let target = eye + forward;

        let vp = build_view_proj(eye, target, aspect);

        let just_fired = game
            .last_shot_time
            .is_some_and(|t| t.elapsed() < SHOT_FLASH_DURATION);

        let mut verts: Vec<Vertex> = Vec::with_capacity(8000);
        build_floor(&mut verts);
        build_ceiling(&mut verts);
        build_walls(&mut verts);
        build_players(&mut verts, game);
        build_crosshair(&mut verts, eye, forward, right, just_fired);

        if debug.show_overlay {
            build_debug_overlay(&mut verts, game, debug, eye, forward, right, just_fired);
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

// ── Floor / Ceiling ───────────────────────────────────────────────────────────

fn build_floor(verts: &mut Vec<Vertex>) {
    for row in 0..TILES {
        for col in 0..TILES {
            let x0 = -MAP_HALF + col as f32 * TILE_SIZE;
            let z0 = -MAP_HALF + row as f32 * TILE_SIZE;
            let x1 = x0 + TILE_SIZE;
            let z1 = z0 + TILE_SIZE;
            let (r, g, b) = if (row + col) % 2 == 0 {
                (0.28_f32, 0.34, 0.28)
            } else {
                (0.18_f32, 0.22, 0.18)
            };
            // Floor faces +Y. CCW from above: (x0,z0)→(x0,z1)→(x1,z1)→(x1,z0)
            push_face(
                verts,
                Vertex::new(x0, 0.0, z0, r, g, b),
                Vertex::new(x0, 0.0, z1, r, g, b),
                Vertex::new(x1, 0.0, z1, r, g, b),
                Vertex::new(x1, 0.0, z0, r, g, b),
            );
        }
    }
}

fn build_ceiling(verts: &mut Vec<Vertex>) {
    // Ceiling at CEILING_HEIGHT, faces -Y (visible from below).
    // CCW from below = CW from above: (x0,z0)→(x1,z0)→(x1,z1)→(x0,z1)
    let h = physics::CEILING_HEIGHT;
    let half = MAP_HALF;
    let (r, g, b) = (0.18_f32, 0.20, 0.22); // slightly cool dark tone

    // One large quad — no tiling needed since it's above the player
    push_face(
        verts,
        Vertex::new(-half, h, -half, r, g, b),
        Vertex::new(half, h, -half, r, g, b),
        Vertex::new(half, h, half, r, g, b),
        Vertex::new(-half, h, half, r, g, b),
    );
}

// ── Walls ─────────────────────────────────────────────────────────────────────

fn build_walls(verts: &mut Vec<Vertex>) {
    let h = physics::WALL_HEIGHT;
    for wall in physics::WALLS {
        push_box(
            verts, wall.x_min, 0.0, wall.z_min, wall.x_max, h, wall.z_max, 0.45, 0.35, 0.25,
        );
    }
}

// ── Player models ─────────────────────────────────────────────────────────────

fn build_players(verts: &mut Vec<Vertex>, game: &GameView) {
    for p in &game.players {
        if game.player_id == Some(p.id) {
            continue; // don't render the local player (first-person)
        }

        let base = p.position.to_vec3();
        // Decode player yaw so we can orient the face direction indicator
        let player_yaw = p.yaw as f32 / 65536.0 * std::f32::consts::TAU;
        let (sin_y, cos_y) = player_yaw.sin_cos();
        let face_dir = Vec3::new(sin_y, 0.0, -cos_y); // forward facing direction

        let is_dead = p.health == 0;
        let (body_r, body_g, body_b): (f32, f32, f32) = if is_dead {
            (0.3, 0.3, 0.3) // grey for dead
        } else {
            (1.0, 0.5, 0.1) // orange for alive
        };

        let x = base.x;
        let y = base.y; // base Y from server — non-zero when jumping
        let z = base.z;

        // ── Legs (y+0.0 → y+0.85) ─────────────────────────────────────────
        let leg_half = 0.28_f32;
        let leg_top = 0.85_f32;
        push_box(
            verts,
            x - leg_half,
            y,
            z - leg_half,
            x + leg_half,
            y + leg_top,
            z + leg_half,
            body_r * 0.7,
            body_g * 0.7,
            body_b * 0.7,
        );

        // ── Torso (y+0.85 → y+1.65) ───────────────────────────────────────
        let torso_half = 0.38_f32;
        let torso_bot = leg_top;
        let torso_top = 1.65_f32;
        push_box(
            verts,
            x - torso_half,
            y + torso_bot,
            z - torso_half,
            x + torso_half,
            y + torso_top,
            z + torso_half,
            body_r,
            body_g,
            body_b,
        );

        // ── Head (y+1.65 → y+2.05) ────────────────────────────────────────
        let head_half = 0.25_f32;
        let head_bot = torso_top;
        let head_top = 2.05_f32;
        push_box(
            verts,
            x - head_half,
            y + head_bot,
            z - head_half,
            x + head_half,
            y + head_top,
            z + head_half,
            body_r * 0.9 + 0.1,
            body_g * 0.85 + 0.05,
            body_b * 0.6 + 0.3, // skin-ish tint
        );

        // ── Face direction indicator (dark band on front of head) ─────────
        let right_dir = Vec3::new(cos_y, 0.0, sin_y);
        let face_center =
            Vec3::new(x, y + (head_bot + head_top) * 0.5, z) + face_dir * (head_half + 0.005);
        push_flat_quad(
            verts,
            face_center - right_dir * head_half * 0.8 - Vec3::Y * (head_half * 0.8),
            right_dir * head_half * 1.6,
            Vec3::Y * head_half * 1.6,
            0.1,
            0.1,
            0.1,
        );
    }
}

// ── Crosshair ─────────────────────────────────────────────────────────────────

fn build_crosshair(
    verts: &mut Vec<Vertex>,
    eye: Vec3,
    forward: Vec3,
    right: Vec3,
    just_fired: bool,
) {
    let center = eye + forward * 2.0;
    let up = Vec3::Y;
    let (arm, thick) = if just_fired {
        (0.05_f32, 0.008_f32) // bigger crosshair on fire
    } else {
        (0.025_f32, 0.004_f32)
    };
    let (r, g, b) = if just_fired {
        (1.0_f32, 1.0, 0.0) // yellow flash when shot
    } else {
        (1.0_f32, 1.0, 1.0) // white normally
    };

    // Horizontal bar
    push_flat_quad(
        verts,
        center - right * arm - up * thick,
        right * arm * 2.0,
        up * thick * 2.0,
        r,
        g,
        b,
    );
    // Vertical bar
    push_flat_quad(
        verts,
        center - right * thick - up * arm,
        right * thick * 2.0,
        up * arm * 2.0,
        r,
        g,
        b,
    );
}

// ── Debug overlay ─────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn build_debug_overlay(
    verts: &mut Vec<Vertex>,
    game: &GameView,
    debug: &DebugSettings,
    eye: Vec3,
    forward: Vec3,
    right: Vec3,
    just_fired: bool,
) {
    let up = Vec3::Y;

    // ── Server ghost: red wire box at server-confirmed position ───────────
    let s = game.server_pos;
    let gh = physics::PLAYER_HEIGHT;
    let gh_half = physics::PLAYER_HALF;
    push_wire_box(
        verts,
        s.x - gh_half,
        s.y,
        s.z - gh_half,
        s.x + gh_half,
        s.y + gh,
        s.z + gh_half,
        0.9,
        0.2,
        0.2,
    );

    // ── Player hitbox wireframes (cyan) ───────────────────────────────────
    for p in &game.players {
        if game.player_id == Some(p.id) {
            continue;
        }
        let w = p.position.to_vec3();
        let ph = physics::PLAYER_HALF;
        push_wire_box(
            verts,
            w.x - ph,
            w.y,
            w.z - ph,
            w.x + ph,
            w.y + physics::PLAYER_HEIGHT,
            w.z + ph,
            0.0,
            0.9,
            0.9,
        );
    }

    // ── Wall hitbox wireframes (green) ────────────────────────────────────
    for wall in physics::WALLS {
        push_wire_box(
            verts,
            wall.x_min,
            0.0,
            wall.z_min,
            wall.x_max,
            physics::WALL_HEIGHT,
            wall.z_max,
            0.2,
            0.9,
            0.3,
        );
    }

    // ── Shot ray indicator ────────────────────────────────────────────────
    if just_fired {
        // Start slightly in front of eye to avoid near-plane clipping.
        let ray_start = eye + forward * 0.15;
        let ray_end = eye + forward * shared::combat::HITSCAN_RANGE;
        let perp_r = right * 0.06;
        let perp_u = up * 0.06;
        // Horizontal slab
        push_face(
            verts,
            vert(ray_start + perp_r, 1.0, 1.0, 0.0),
            vert(ray_end + perp_r, 1.0, 1.0, 0.0),
            vert(ray_end - perp_r, 1.0, 1.0, 0.0),
            vert(ray_start - perp_r, 1.0, 1.0, 0.0),
        );
        // Vertical slab (cross-shape makes it visible from any angle)
        push_face(
            verts,
            vert(ray_start + perp_u, 1.0, 1.0, 0.0),
            vert(ray_start - perp_u, 1.0, 1.0, 0.0),
            vert(ray_end - perp_u, 1.0, 1.0, 0.0),
            vert(ray_end + perp_u, 1.0, 1.0, 0.0),
        );
        // Bright dot at the end of the ray to mark where the bullet terminated
        push_box(
            verts,
            ray_end.x - 0.12,
            ray_end.y - 0.12,
            ray_end.z - 0.12,
            ray_end.x + 0.12,
            ray_end.y + 0.12,
            ray_end.z + 0.12,
            1.0,
            0.3,
            0.0,
        );
    }

    // ── HUD bars (bottom-left of view) ────────────────────────────────────
    let hud_base = eye + forward * 1.5 - right * 0.65 - up * 0.42;

    let bar_w = (game.rtt_ms / 200.0).min(1.0) * 0.28;
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
        right * bar_w.max(0.02),
        up * 0.014,
        r,
        g,
        b,
    );

    let pending = game.pending_inputs.min(20) as f32;
    push_flat_quad(
        verts,
        hud_base - up * 0.023,
        right * (pending * 0.014).max(0.005),
        up * 0.010,
        0.5,
        0.5,
        0.9,
    );

    if debug.simulated_latency_ms > 0 {
        let lat_w = debug.simulated_latency_ms as f32 / 200.0 * 0.28;
        push_flat_quad(
            verts,
            hud_base - up * 0.042,
            right * lat_w,
            up * 0.010,
            0.9,
            0.4,
            0.9,
        );
    }
}

// ── Wire box ──────────────────────────────────────────────────────────────────

/// Draw a 3D box outline using thin axis-aligned stick boxes for each of the 12 edges.
#[allow(clippy::too_many_arguments)]
fn push_wire_box(
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
    let t = WIRE_T;
    // Bottom 4 edges (y=y0)
    push_box(
        verts,
        x0,
        y0 - t / 2.0,
        z0 - t / 2.0,
        x1,
        y0 + t / 2.0,
        z0 + t / 2.0,
        r,
        g,
        b,
    );
    push_box(
        verts,
        x0,
        y0 - t / 2.0,
        z1 - t / 2.0,
        x1,
        y0 + t / 2.0,
        z1 + t / 2.0,
        r,
        g,
        b,
    );
    push_box(
        verts,
        x0 - t / 2.0,
        y0 - t / 2.0,
        z0,
        x0 + t / 2.0,
        y0 + t / 2.0,
        z1,
        r,
        g,
        b,
    );
    push_box(
        verts,
        x1 - t / 2.0,
        y0 - t / 2.0,
        z0,
        x1 + t / 2.0,
        y0 + t / 2.0,
        z1,
        r,
        g,
        b,
    );
    // Top 4 edges (y=y1)
    push_box(
        verts,
        x0,
        y1 - t / 2.0,
        z0 - t / 2.0,
        x1,
        y1 + t / 2.0,
        z0 + t / 2.0,
        r,
        g,
        b,
    );
    push_box(
        verts,
        x0,
        y1 - t / 2.0,
        z1 - t / 2.0,
        x1,
        y1 + t / 2.0,
        z1 + t / 2.0,
        r,
        g,
        b,
    );
    push_box(
        verts,
        x0 - t / 2.0,
        y1 - t / 2.0,
        z0,
        x0 + t / 2.0,
        y1 + t / 2.0,
        z1,
        r,
        g,
        b,
    );
    push_box(
        verts,
        x1 - t / 2.0,
        y1 - t / 2.0,
        z0,
        x1 + t / 2.0,
        y1 + t / 2.0,
        z1,
        r,
        g,
        b,
    );
    // 4 vertical corner pillars
    push_box(
        verts,
        x0 - t / 2.0,
        y0,
        z0 - t / 2.0,
        x0 + t / 2.0,
        y1,
        z0 + t / 2.0,
        r,
        g,
        b,
    );
    push_box(
        verts,
        x1 - t / 2.0,
        y0,
        z0 - t / 2.0,
        x1 + t / 2.0,
        y1,
        z0 + t / 2.0,
        r,
        g,
        b,
    );
    push_box(
        verts,
        x0 - t / 2.0,
        y0,
        z1 - t / 2.0,
        x0 + t / 2.0,
        y1,
        z1 + t / 2.0,
        r,
        g,
        b,
    );
    push_box(
        verts,
        x1 - t / 2.0,
        y0,
        z1 - t / 2.0,
        x1 + t / 2.0,
        y1,
        z1 + t / 2.0,
        r,
        g,
        b,
    );
}

// ── Box builder ───────────────────────────────────────────────────────────────

/// Push a solid 3D box (5 faces — no floor) with correct CCW winding for each face.
/// Face normals point outward so back-face culling works correctly.
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
    let d = 0.78_f32; // side face darkening
    let d2 = 0.60_f32; // left/right even darker

    // Top face (+Y normal): CCW from above — (x0,z0)→(x0,z1)→(x1,z1)→(x1,z0)
    push_face(
        verts,
        Vertex::new(x0, y1, z0, r, g, b),
        Vertex::new(x0, y1, z1, r, g, b),
        Vertex::new(x1, y1, z1, r, g, b),
        Vertex::new(x1, y1, z0, r, g, b),
    );

    // Front face (-Z normal): CCW from -Z — (x0,y0)→(x0,y1)→(x1,y1)→(x1,y0)
    push_face(
        verts,
        Vertex::new(x0, y0, z0, r * d, g * d, b * d),
        Vertex::new(x0, y1, z0, r * d, g * d, b * d),
        Vertex::new(x1, y1, z0, r * d, g * d, b * d),
        Vertex::new(x1, y0, z0, r * d, g * d, b * d),
    );

    // Back face (+Z normal): CCW from +Z — (x1,y0)→(x1,y1)→(x0,y1)→(x0,y0)
    push_face(
        verts,
        Vertex::new(x1, y0, z1, r * d, g * d, b * d),
        Vertex::new(x1, y1, z1, r * d, g * d, b * d),
        Vertex::new(x0, y1, z1, r * d, g * d, b * d),
        Vertex::new(x0, y0, z1, r * d, g * d, b * d),
    );

    // Left face (-X normal): CCW from -X — (x0,y0,z1)→(x0,y1,z1)→(x0,y1,z0)→(x0,y0,z0)
    push_face(
        verts,
        Vertex::new(x0, y0, z1, r * d2, g * d2, b * d2),
        Vertex::new(x0, y1, z1, r * d2, g * d2, b * d2),
        Vertex::new(x0, y1, z0, r * d2, g * d2, b * d2),
        Vertex::new(x0, y0, z0, r * d2, g * d2, b * d2),
    );

    // Right face (+X normal): CCW from +X — (x1,y0,z0)→(x1,y1,z0)→(x1,y1,z1)→(x1,y0,z1)
    push_face(
        verts,
        Vertex::new(x1, y0, z0, r * d2, g * d2, b * d2),
        Vertex::new(x1, y1, z0, r * d2, g * d2, b * d2),
        Vertex::new(x1, y1, z1, r * d2, g * d2, b * d2),
        Vertex::new(x1, y0, z1, r * d2, g * d2, b * d2),
    );
}

// ── Primitive helpers ─────────────────────────────────────────────────────────

/// Push one quad as two CCW triangles. Winding: v0→v1→v2 and v0→v2→v3.
fn push_face(verts: &mut Vec<Vertex>, v0: Vertex, v1: Vertex, v2: Vertex, v3: Vertex) {
    verts.extend_from_slice(&[v0, v1, v2, v0, v2, v3]);
}

/// Convenience constructor for a colored vertex.
fn vert(p: Vec3, r: f32, g: f32, b: f32) -> Vertex {
    Vertex::new(p.x, p.y, p.z, r, g, b)
}

/// Push a parallelogram quad defined by origin + two edge vectors.
/// Normal = width × height (faces toward the camera when width=right, height=up).
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
