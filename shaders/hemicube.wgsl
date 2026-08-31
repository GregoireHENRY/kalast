// Hemicube view factors: render facet indices, then accumulate weighted.
//
// The render half writes `1 + facet` into an integer atlas holding the five
// faces of one hemicube side by side. The compute half walks that atlas and
// adds each pixel's delta form factor to whichever facet the pixel shows.
//
// The accumulation half lives in `hemicube_accumulate.wgsl`.

struct Params {
    view_proj: mat4x4<f32>,
    facet_offset: u32,
    // Three scalars, not a vec3<u32>: WGSL aligns a vec3 to 16 bytes, which
    // would pad the struct to 96 and silently disagree with the 80-byte Rust
    // side. The validator catches it, but only at pipeline creation.
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};
@group(0) @binding(0)
var<uniform> params: Params;

struct VertexInput {
    @location(0) pos: vec3<f32>,
    @location(1) tex: vec2<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) tangent: vec3<f32>,
    @location(4) bitangent: vec3<f32>,
};

struct InstanceInput {
    @location(8) mat_row_0: vec4<f32>,
    @location(9) mat_row_1: vec4<f32>,
    @location(10) mat_row_2: vec4<f32>,
    @location(11) mat_row_3: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) @interpolate(flat) id: u32,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    model: VertexInput,
    instance: InstanceInput,
) -> VertexOutput {
    let m = mat4x4<f32>(
        instance.mat_row_0,
        instance.mat_row_1,
        instance.mat_row_2,
        instance.mat_row_3,
    );
    var out: VertexOutput;
    out.clip_position = params.view_proj * m * vec4<f32>(model.pos, 1.0);
    out.id = params.facet_offset + vertex_index / 3u + 1u;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) u32 {
    return in.id;
}
