struct VertexInput {
    @location(0) pos: vec3<f32>,
    @location(1) tex: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex: vec2<f32>,
};

@vertex
fn vs_main(
    vertex: VertexInput
) -> VertexOutput {
    var out: VertexOutput;
    out.tex = vertex.tex;
    out.clip_position = vec4<f32>(vertex.pos, 1.0);
    return out;
}

@group(0) @binding(0)
var t_shadow: texture_2d<f32>;
@group(0) @binding(1)
var s_shadow: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Reversed-Z: the camera's buffer holds 1 at the near plane and 0 at the
    // far one, so undo that before the linearisation, which is written for the
    // other sense. A cleared texel is 0 and still comes out white, as before.
    //
    // These planes are a fixed guess rather than the camera's own, so the ramp
    // is indicative and not a distance. That was already true.
    let near = 0.1;
    let far = 100.0;
    let depth = 1.0 - textureSample(t_shadow, s_shadow, in.tex).x;
    let r = (2.0 * near) / (far + near - depth * (far - near));
    return vec4<f32>(vec3<f32>(r), 1.0);
}