// Reference axes: world-space line segments, coloured per vertex.
//
// No lighting and no shadow: these are annotation, not geometry, and a tick
// that dims as the Sun moves would be unreadable half the time.

struct Camera {
    view_proj: mat4x4<f32>,
};
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
    @location(1) color: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_main(vertex: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = view.camera.view_proj * vec4<f32>(vertex.pos, 1.0);
    out.color = vertex.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
