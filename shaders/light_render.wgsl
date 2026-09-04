// MUST match `Globals` in src/app/uniform.rs field for field. This is a
// second declaration of the same buffer, so a field missing here does not
// fail to compile -- it silently shifts every field after it. It was missing
// `srgb_mode` and `gamma`, so `light_cube_scale` read `gamma` (2.2) and the
// debug cube came out ~9x oversized.
struct Globals {
    color: vec3<f32>,
    color_mode: u32,
    srgb_mode: u32,
    gamma: f32,
    ambient_strength: f32,
    light_cube_scale: f32,
    shadow_resolution: u32,
    shadow_bias_scale: f32,
    shadow_bias_minimum: f32,
    shadow_normal_offset_scale: f32,
    shadow_pcf: u32,
    extra: u32,
    wireframe_mode: u32,
    wireframe_width: f32,
    wireframe_color: vec3<f32>,
};@group(0) @binding(0)
var<uniform> globals: Globals;

struct Camera {
    view_proj: mat4x4<f32>,
};

// Same rule as Globals. `view_proj_layers` and `layer_bias` were added with
// per-body shadow layers and not mirrored here, so `pos` was read out of the
// middle of a matrix and the cube was drawn at a garbage position -- covering
// a large part of the screen instead of marking the Sun.
struct Light {
    view_proj: mat4x4<f32>,
    view_proj_layers: array<mat4x4<f32>, 8>,
    layer_bias: array<vec4<f32>, 8>,
    pos: vec3<f32>,
    n_layers: u32,
    color: vec3<f32>,
};
struct View {
    camera: Camera,
    light: Light,
};
@group(1) @binding(0)
var<uniform> view: View;

struct VertexInput {
    @location(0) pos: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_main(
    vertex: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = view.camera.view_proj * vec4<f32>(vertex.pos * globals.light_cube_scale + view.light.pos, 1.0);
    out.color = view.light.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}