// The thermophysical model's shadows traced with rays: each facet's corners
// and centre against one acceleration structure per body, built from its
// full-resolution mesh (`app::raytrace`). Only compiled on a device that has
// ray queries; the tracing itself is `sun_rays.wgsl`'s, put in below.
enable wgpu_ray_query;

// Linear limb darkening, as the shadow maps' disc has it
// (`LIMB_DARKENING` in `mesh_shadow.wgsl`): I(mu) = 1 - u (1 - mu).
const LIMB_DARKENING: f32 = 0.56;

@group(0) @binding(0) var tlas: acceleration_structure;

struct FacetRays {
    // The body's matrix, its mesh's corners into the scene.
    mat: mat4x4<f32>,
    sun: vec3<f32>,
    // 0 for a point Sun.
    sun_radius: f32,
    n_facets: u32,
    // Invocations per row of a dispatch split in two (`dispatch_2d`).
    stride: u32,
    // 1 when facet `i` is corners `3i..3i+2`, 0 when `indices` say.
    is_flat: u32,
    // Rays per point across the disc.
    samples: u32,
    // Rays tried first, the rest only where they disagree; 0: all of them
    // everywhere.
    probe: u32,
    // 1: `guide` says which facets the disc is traced for (`shadows.ray_guide`).
    guided: u32,
    // The first facet of this dispatch: a query traced a range of facets at
    // a time (`raytrace::FacetPass`), up to `n_facets`.
    first: u32,
    _pad2: u32,
};
@group(0) @binding(1) var<uniform> q: FacetRays;
@group(0) @binding(2) var<storage, read> geometry: array<f32>;
@group(0) @binding(3) var<storage, read> indices: array<u32>;
// Per facet, the share of the Sun's light its points do not see.
@group(0) @binding(4) var<storage, read_write> out: array<f32>;
// Per facet, 1 where the shadow maps see a penumbra can reach it, with
// `guided` (`cs_facets_guide` in `mesh_shadow.wgsl`).
@group(0) @binding(5) var<storage, read> guide: array<u32>;

fn vertex(i: u32) -> vec3<f32> {
    let p = vec3<f32>(geometry[3u * i], geometry[3u * i + 1u], geometry[3u * i + 2u]);
    return (q.mat * vec4<f32>(p, 1.0)).xyz;
}

/// Each facet's shadowed share: its corners and centre, as the shadow maps'
/// query takes them (`cs_facets` in `mesh_shadow.wgsl`), each seeing what
/// its rays reach of the Sun.
@compute @workgroup_size(64)
fn cs_facets(@builtin(global_invocation_id) id: vec3<u32>) {
    let f = q.first + id.y * q.stride + id.x;
    if f >= q.n_facets {
        return;
    }
    var i = vec3<u32>(3u * f, 3u * f + 1u, 3u * f + 2u);
    if q.is_flat == 0u {
        i = vec3<u32>(indices[3u * f], indices[3u * f + 1u], indices[3u * f + 2u]);
    }
    let a = vertex(i.x);
    let b = vertex(i.y);
    let c = vertex(i.z);
    let normal = cross(b - a, c - a);
    if dot(normal, normal) == 0.0 {
        // No surface to light.
        out[f] = 1.0;
        return;
    }
    // The facet as a flat plate, as the thermophysical model has it.
    let n = normalize(normal);
    let centre = (a + b + c) / 3.0;
    // Guided, one ray a point where the maps see no penumbra can be: all of
    // the disc or, in the umbra, none of it.
    let samples = select(q.samples, 1u, q.guided != 0u && guide[f] == 0u);
    var lit = 0.0;
    for (var k = 0u; k < 4u; k++) {
        let p = select(select(select(centre, c, k == 2u), b, k == 1u), a, k == 0u);
        lit += ray_sun_seen(p, n, q.sun, q.sun_radius, samples, q.probe, ray_turn(4u * f + k));
    }
    out[f] = 1.0 - lit / 4.0;
}

//@rays
