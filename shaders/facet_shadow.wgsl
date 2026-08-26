// Per-facet shadow query.
//
// Answers, for every facet of one body, what fraction of its sample points
// are occluded from the light -- reading the same shadow map the render pass
// samples, with the same projection and the same depth bias, so that what
// this reports and what you see rendered cannot disagree.
//
// Sampling is the facet's 3 vertices plus its centroid, giving a coverage
// fraction in {0, 0.25, 0.5, 0.75, 1}. 0 is fully lit, 1 fully shadowed,
// anything between is a facet straddling a shadow boundary.
//
// Geometry is read straight out of the render pass's vertex buffer rather
// than a second upload, so the positions tested are exactly the positions
// drawn. GeometryVertex is 14 consecutive f32 (pos.3, tex.2, normal.3,
// tangent.3, bitangent.3), so vertex i starts at i * 14 -- see
// `GeometryVertex` in src/app/gpu.rs; the two must stay in step.

const VERTEX_STRIDE: u32 = 14u;

struct Params {
    // Body model matrix: geometry is stored in model space.
    model: mat4x4<f32>,
    light_view_proj: mat4x4<f32>,
    light_pos: vec3<f32>,
    n_facets: u32,

    shadow_bias_scale: f32,
    shadow_bias_minimum: f32,
    shadow_normal_offset_scale: f32,
    // 1 when the mesh is flat (vertices are already triangle-major, so facet
    // i is vertices 3i..3i+2); 0 when it is indexed and `indices` applies.
    is_flat: u32,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var t_shadow: texture_depth_2d;
@group(0) @binding(2) var<storage, read> geometry: array<f32>;
@group(0) @binding(3) var<storage, read> indices: array<u32>;
@group(0) @binding(4) var<storage, read_write> out_fraction: array<f32>;

fn vertex_pos(i: u32) -> vec3<f32> {
    let b = i * VERTEX_STRIDE;
    return vec3<f32>(geometry[b], geometry[b + 1u], geometry[b + 2u]);
}

/// Occlusion of one world-space point, mirroring the fragment shader's
/// shadow lookup exactly (normal offset, y flip, bias) but with an explicit
/// textureLoad rather than a comparison sampler, which compute cannot use.
/// Returns 1.0 when occluded.
fn occluded(world_pos: vec3<f32>, world_normal: vec3<f32>) -> f32 {
    let light_dir = normalize(params.light_pos - world_pos);
    let ndotl = max(dot(world_normal, light_dir), 0.0);
    let k = 1.0 - ndotl;
    let k2 = k * k;

    let normal_offset = params.shadow_normal_offset_scale * k;
    let offset_pos = world_pos + world_normal * normal_offset;

    let light_space = params.light_view_proj * vec4<f32>(offset_pos, 1.0);
    if light_space.w <= 0.0 {
        return 0.0;
    }
    var proj = light_space.xyz / light_space.w;
    proj.y = -proj.y;
    let uv = proj.xy * 0.5 + 0.5;

    // Outside the light frustum there is no occluder information. Treat as
    // lit: the fitted frustum covers the whole scene, so this only happens
    // for geometry the light cannot reach anyway.
    if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
        return 0.0;
    }

    let dims = textureDimensions(t_shadow);
    let texel = vec2<i32>(uv * vec2<f32>(dims));
    let stored = textureLoad(t_shadow, texel, 0);

    let bias = max(params.shadow_bias_scale * k2, params.shadow_bias_minimum);

    // The render pass uses CompareFunction::LessEqual and reads 1.0 = lit,
    // so the occluded case is the strict complement.
    if proj.z - bias > stored {
        return 1.0;
    }
    return 0.0;
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let f = gid.x;
    if f >= params.n_facets {
        return;
    }

    var i0 = f * 3u;
    var i1 = f * 3u + 1u;
    var i2 = f * 3u + 2u;
    if params.is_flat == 0u {
        i0 = indices[f * 3u];
        i1 = indices[f * 3u + 1u];
        i2 = indices[f * 3u + 2u];
    }

    let a = (params.model * vec4<f32>(vertex_pos(i0), 1.0)).xyz;
    let b = (params.model * vec4<f32>(vertex_pos(i1), 1.0)).xyz;
    let c = (params.model * vec4<f32>(vertex_pos(i2), 1.0)).xyz;

    // Geometric facet normal, not the interpolated vertex normal: the shadow
    // test is about the facet as a flat plate, which is also what the
    // thermophysical boundary condition assumes.
    let n = normalize(cross(b - a, c - a));

    let centroid = (a + b + c) / 3.0;

    let occ = occluded(a, n)
        + occluded(b, n)
        + occluded(c, n)
        + occluded(centroid, n);

    out_fraction[f] = occ * 0.25;
}
