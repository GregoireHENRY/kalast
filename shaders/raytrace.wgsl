// Shadows from the Sun traced with rays: the GPU's ray queries against one
// acceleration structure per body, built from its full-resolution mesh
// (`app::raytrace`). Only compiled on a device that has them.
enable wgpu_ray_query;

// Linear limb darkening, as the shadow maps' disc has it
// (`LIMB_DARKENING` in `mesh_shadow.wgsl`): I(mu) = 1 - u (1 - mu).
const LIMB_DARKENING: f32 = 0.56;
const GOLDEN_ANGLE: f32 = 2.3999632;
const TAU: f32 = 6.2831855;

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
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};
@group(0) @binding(1) var<uniform> q: FacetRays;
@group(0) @binding(2) var<storage, read> geometry: array<f32>;
@group(0) @binding(3) var<storage, read> indices: array<u32>;
// Per facet, the share of the Sun's light its points do not see.
@group(0) @binding(4) var<storage, read_write> out: array<f32>;

/// The squared radius, on the disc of radius 1, inside which a share `t` of
/// the limb-darkened disc's light falls: the radial CDF inverted by Newton.
/// With x = r^2 and mu = sqrt(1 - x), the light inside is
/// (1 - u) x + u 2/3 (1 - mu^3), of 1 - u/3 in all.
fn limb_radius2(t: f32) -> f32 {
    let u = LIMB_DARKENING;
    let total = 1.0 - u / 3.0;
    var x = t;
    for (var k = 0; k < 6; k++) {
        let mu = sqrt(max(1.0 - x, 0.0));
        let f = ((1.0 - u) * x + u * (2.0 / 3.0) * (1.0 - mu * mu * mu)) / total - t;
        let df = ((1.0 - u) + u * mu) / total;
        x = clamp(x - f / max(df, 1.0e-6), 0.0, 1.0);
    }
    return x;
}

/// Sample `i` of `n` on the disc of radius 1, each carrying the same share
/// of the limb-darkened disc's light: radii at equal steps of its CDF, and
/// angles a golden-angle spiral turned by `turn`. A fixed pattern, so the
/// same scene gives the same answer.
fn disc_sample(i: u32, n: u32, turn: f32) -> vec2<f32> {
    let r = sqrt(limb_radius2((f32(i) + 0.5) / f32(n)));
    let phi = f32(i) * GOLDEN_ANGLE + turn;
    return r * vec2<f32>(cos(phi), sin(phi));
}

/// The ray's origin off the surface at `p`, facing `n`, by a few units in
/// the last place of its coordinates: the surface it starts on is not hit
/// again, and relief a millimetre high still is (Waechter & Binder, Ray
/// Tracing Gems ch. 6). A fixed offset of 1 cm passed over bumps of 1-2 cm
/// (`notes/2026-10-09_disc_gaps_hidden_relief/`).
fn offset_origin(p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    let ulps = vec3<i32>(16.0 * n);
    var o = p;
    for (var k = 0; k < 3; k++) {
        let step = select(ulps[k], -ulps[k], p[k] < 0.0);
        o[k] = bitcast<f32>(bitcast<i32>(p[k]) + step);
        // Near zero a unit in the last place is no distance at all.
        if abs(p[k]) < 1.0 / 32.0 {
            o[k] = p[k] + n[k] * (1.0 / 65536.0) * (1.0 / 32.0);
        }
    }
    return o;
}

/// Whether anything lies on the way from `origin` along `dir` for `len`.
fn blocked(origin: vec3<f32>, dir: vec3<f32>, len: f32) -> bool {
    var rq: ray_query;
    rayQueryInitialize(&rq, tlas, RayDesc(RAY_FLAG_TERMINATE_ON_FIRST_HIT | RAY_FLAG_FORCE_OPAQUE, 0xffu, 0.0, len, origin, dir));
    rayQueryProceed(&rq);
    return rayQueryGetCommittedIntersection(&rq).kind != RAY_QUERY_INTERSECTION_NONE;
}

/// The share of the Sun's light a point `p` of a surface facing `n` sees:
/// none when the surface faces away from the Sun's centre, as the shadow
/// maps have it; one ray for a point Sun, `q.samples` across its disc.
fn sun_seen(p: vec3<f32>, n: vec3<f32>, turn: f32) -> f32 {
    let to_sun = q.sun - p;
    let dist = length(to_sun);
    let d = to_sun / dist;
    if dot(n, d) <= 0.0 {
        return 0.0;
    }
    let origin = offset_origin(p, n);
    if q.sun_radius <= 0.0 || q.samples <= 1u {
        return select(1.0, 0.0, blocked(origin, d, dist));
    }
    // Two directions across the disc as the point sees it.
    let side = select(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(1.0, 0.0, 0.0), abs(d.z) > 0.9);
    let t1 = normalize(cross(d, side));
    let t2 = cross(d, t1);
    // A few rays first: where they all agree the point is wholly lit or
    // wholly hidden, as nearly every point is, and the rest are not traced.
    // Only a point a penumbra crosses takes all of them -- 25,000 facets of
    // Didymos's 3.1 million, with Dimorphos's shadow on it.
    //
    // Half of them on the rim: an occluder's edge comes onto the disc across
    // it, so a sliver a few rim rays apart is seen. Spread by light alone,
    // they kept off the dim rim, and a penumbra's edge passed between them
    // -- 4.8 % of the disc missed at the edge of a wall's shadow. The other
    // half inside, for an occluder smaller than the disc.
    if q.probe > 0u && q.samples > q.probe {
        var hidden = 0u;
        let rim = q.probe / 2u;
        for (var i = 0u; i < q.probe; i++) {
            var s = disc_sample(i - rim, q.probe - rim, turn);
            if i < rim {
                let phi = (f32(i) + 0.5) * (TAU / f32(rim)) + turn;
                s = 0.999 * vec2<f32>(cos(phi), sin(phi));
            }
            let v = q.sun + q.sun_radius * (s.x * t1 + s.y * t2) - origin;
            let len = length(v);
            if blocked(origin, v / len, len) {
                hidden += 1u;
            }
        }
        if hidden == 0u {
            return 1.0;
        }
        if hidden == q.probe {
            return 0.0;
        }
    }
    var seen = 0u;
    for (var i = 0u; i < q.samples; i++) {
        let s = disc_sample(i, q.samples, turn);
        let v = q.sun + q.sun_radius * (s.x * t1 + s.y * t2) - origin;
        let len = length(v);
        if !blocked(origin, v / len, len) {
            seen += 1u;
        }
    }
    return f32(seen) / f32(q.samples);
}

/// A turn of the disc's pattern for each point, so neighbouring points do
/// not all miss the same part of a penumbra. Fixed, as the pattern is.
fn turn_of(key: u32) -> f32 {
    var h = key * 747796405u + 2891336453u;
    h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
    h = (h >> 22u) ^ h;
    return f32(h) * (TAU / 4294967296.0);
}

fn vertex(i: u32) -> vec3<f32> {
    let p = vec3<f32>(geometry[3u * i], geometry[3u * i + 1u], geometry[3u * i + 2u]);
    return (q.mat * vec4<f32>(p, 1.0)).xyz;
}

/// Each facet's shadowed share: its corners and centre, as the shadow maps'
/// query takes them (`cs_facets` in `mesh_shadow.wgsl`), each seeing what
/// its rays reach of the Sun.
@compute @workgroup_size(64)
fn cs_facets(@builtin(global_invocation_id) id: vec3<u32>) {
    let f = id.y * q.stride + id.x;
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
    var lit = 0.0;
    for (var k = 0u; k < 4u; k++) {
        let p = select(select(select(centre, c, k == 2u), b, k == 1u), a, k == 0u);
        lit += sun_seen(p, n, turn_of(4u * f + k));
    }
    out[f] = 1.0 - lit / 4.0;
}
