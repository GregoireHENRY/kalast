// Facet-index render target.
//
// Writes, per pixel, `1 + offset + facet` for the facet that covers it, and
// 0 where nothing does. That turns "which facet does the instrument see here"
// into an array lookup on the CPU, so per-facet quantities -- radiance in a
// given filter, temperature, incidence angle -- can be mapped to pixels
// exactly, at full float precision, instead of being encoded into a colour
// and read back quantised.
//
// `offset` is per body, so several bodies share one global index space and a
// single readback resolves both which body and which of its facets.
//
// Only meaningful for flattened meshes, where each facet owns three vertices
// and `vertex_index / 3` is therefore the facet index. `FacetIdPass::render`
// skips any mesh that is not flat rather than writing indices that would be
// silently wrong.

// Same declaration as mesh_shadow.wgsl, so this pass can reuse the renderer's
// existing view bind group rather than maintaining a second camera uniform --
// which would be one more thing that can silently disagree with what was
// actually drawn.
struct Camera {
    view_proj: mat4x4<f32>,
};

struct Light {
    view_proj: mat4x4<f32>,
    pos: vec3<f32>,
    color: vec3<f32>,
};

struct View {
    camera: Camera,
    light: Light,
};
@group(0) @binding(0)
var<uniform> view: View;

struct IdParams {
    offset: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};
@group(1) @binding(0)
var<uniform> params: IdParams;

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
    // Flat: an index must not be interpolated across the triangle.
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
    out.clip_position = view.camera.view_proj * m * vec4<f32>(model.pos, 1.0);
    // +1 so that 0 stays reserved for "no facet here".
    out.id = params.offset + vertex_index / 3u + 1u;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) u32 {
    return in.id;
}
