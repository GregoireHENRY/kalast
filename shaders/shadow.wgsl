struct Camera {
    view_proj: mat4x4<f32>,
};

struct Light {
    // Scratch the shadow pass used to draw with, when it was rewritten per
    // layer. Nothing reads it now; kept so the struct still matches the
    // buffer the other shaders share.
    view_proj: mat4x4<f32>,
    // One per body: aimed at it and sized to it, with depth spanning the
    // scene so occluders still cast into it.
    view_proj_layers: array<mat4x4<f32>, 8>,
    // Per layer: (normal_offset_scale, bias_scale, bias_minimum, unused).
    layer_bias: array<vec4<f32>, 8>,
    pos: vec3<f32>,
    n_layers: u32,
    color: vec3<f32>,
};

// Which layer this pass is drawing into. A dynamic offset picks the entry,
// so every layer can share one encoder; the matrices themselves are already
// uploaded once per frame in `view_proj_layers`.
struct ShadowLayer {
    index: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};
@group(2) @binding(0)
var<uniform> shadow_layer: ShadowLayer;

struct View {
    camera: Camera,
    light: Light,
};
@group(1) @binding(0)
var<uniform> view: View;

struct InstanceInput {
    @location(8) mat_row_0: vec4<f32>,
    @location(9) mat_row_1: vec4<f32>,
    @location(10) mat_row_2: vec4<f32>,
    @location(11) mat_row_3: vec4<f32>,
};

struct VertexInput {
    @location(0) pos: vec3<f32>,
};

@vertex
fn vs_main(
    vertex: VertexInput,
    instance: InstanceInput,
) -> @builtin(position) vec4<f32> {
    let model_matrix = mat4x4<f32>(
        instance.mat_row_0,
        instance.mat_row_1,
        instance.mat_row_2,
        instance.mat_row_3,
    );

    return view.light.view_proj_layers[shadow_layer.index]
        * model_matrix * vec4<f32>(vertex.pos, 1.0);
}
