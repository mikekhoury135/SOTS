// ── Uniforms ──────────────────────────────────────────────────────────────────

struct Uniforms {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

// ── Vertex ────────────────────────────────────────────────────────────────────

struct VertexIn {
    @location(0) position : vec3<f32>,
    @location(1) color    : vec3<f32>,
};

struct VertexOut {
    @builtin(position) clip_pos : vec4<f32>,
    @location(0)       color    : vec3<f32>,
};

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip_pos = uniforms.view_proj * vec4<f32>(in.position, 1.0);
    out.color    = in.color;
    return out;
}

// ── Fragment ──────────────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
