// Horizon maps (`app::horizon`): for every facet of a body whose surface
// each direction from its centre crosses once -- a planet, most small
// bodies -- the highest its terrain rises in each of `AZIMUTHS` directions,
// as the sine of the elevation over the plane square to the radius.
//
// 1. `raster`: the mesh into radius grids, longitude by latitude -- a coarse
//    one over the whole body and a fine one over where its facets are
//    small. Each facet writes the cells whose direction from the centre
//    meets it, at the radius it meets it.
// 2. `fill`: a cell no facet claimed -- a direction through a shared edge
//    missed by both, in floats -- takes its neighbours' mean.
// 3. `march`: from each facet's centre, along the great circle each way,
//    the terrain's elevation from the grids, the fine one near and the
//    coarse one far, in steps growing with the distance; until the highest
//    the body rises could no longer be seen above what was. Passing over
//    stretches a pyramid of highest radii said were lower than what was
//    seen, tried, made it slower: near a facet the terrain is seldom that
//    much lower, and the test cost what the steps it saved did.

const AZIMUTHS: u32 = 32u;
const PI: f32 = 3.14159265358979;
// Not claimed by any facet: what the grids are cleared to.
const EMPTY: f32 = 0.0;

struct Params {
    n_facets: u32,
    // Invocations per row: a large mesh is dispatched in rows.
    stride: u32,
    // 0 the coarse grid, 1 the fine one, for `raster` and `fill`.
    grid: u32,
    _pad: u32,
    coarse_size: vec2<u32>,
    fine_size: vec2<u32>,
    // The fine grid's corner, longitude and latitude, and its cells' size,
    // radians; its longitudes wrap.
    fine_lo: vec2<f32>,
    fine_cell: vec2<f32>,
    // How far the fine grid is used along the surface, scene units.
    fine_reach: f32,
    // The first step along the surface, how much each grows (a fraction of
    // the distance), and the largest.
    first_step: f32,
    growth: f32,
    max_step: f32,
    _pad2: vec4<u32>,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> positions: array<f32>;
@group(0) @binding(2) var<storage, read> indices: array<u32>;
@group(0) @binding(3) var<storage, read_write> coarse: array<f32>;
@group(0) @binding(4) var<storage, read_write> fine: array<f32>;
// Two azimuths a word, the sine of each one's elevation as a snorm16, the
// first in the low half.
@group(0) @binding(5) var<storage, read_write> horizons: array<u32>;
// Per coarse cell of 4 x 4 degrees, the highest radius within reach of it.
@group(0) @binding(6) var<storage, read> highest: array<f32>;
// The facets in an order that keeps neighbours together, a Morton curve
// through their centres: what the march takes them in.
@group(0) @binding(7) var<storage, read> order: array<u32>;
// The highest horizon of all, as its snorm16, then per degree of longitude
// and latitude, `TOP_CELLS` of them from -180 and from the north pole, the
// highest of the facets whose centres are in it.
@group(0) @binding(8) var<storage, read_write> top_horizon: array<atomic<i32>>;

const TOP_CELLS: vec2<u32> = vec2<u32>(1440u, 720u);

fn vertex(i: u32) -> vec3<f32> {
    return vec3<f32>(positions[3u * i], positions[3u * i + 1u], positions[3u * i + 2u]);
}

fn lon_lat(d: vec3<f32>) -> vec2<f32> {
    return vec2<f32>(atan2(d.y, d.x), asin(clamp(d.z / length(d), -1.0, 1.0)));
}

fn direction(lon: f32, lat: f32) -> vec3<f32> {
    return vec3<f32>(cos(lat) * cos(lon), cos(lat) * sin(lon), sin(lat));
}

/// The coarse grid's cell centres: longitude from -pi, latitude from the
/// north pole down.
fn coarse_cell(i: i32, j: i32) -> vec2<f32> {
    let n = vec2<f32>(params.coarse_size);
    return vec2<f32>(-PI + (f32(i) + 0.5) * 2.0 * PI / n.x, 0.5 * PI - (f32(j) + 0.5) * PI / n.y);
}

/// The fine grid's: from its corner, longitude east, latitude north.
fn fine_cell(i: i32, j: i32) -> vec2<f32> {
    return params.fine_lo + (vec2<f32>(f32(i), f32(j)) + 0.5) * params.fine_cell;
}

fn wrap(a: f32) -> f32 {
    return a - 2.0 * PI * floor(a / (2.0 * PI));
}

fn facet_id(id: vec3<u32>) -> u32 {
    return id.y * params.stride + id.x;
}

/// A facet `a, b, c` as seen from the centre: the planes through the
/// centre and each edge, and the way it faces.
struct Seen {
    ab: vec3<f32>,
    bc: vec3<f32>,
    ca: vec3<f32>,
    mid: vec3<f32>,
};

fn seen(a: vec3<f32>, b: vec3<f32>, c: vec3<f32>) -> Seen {
    return Seen(normalize(cross(a, b)), normalize(cross(b, c)), normalize(cross(c, a)), a + b + c);
}

/// Whether direction `d` from the centre meets the facet, to a few metres
/// either side of its edges.
fn meets(d: vec3<f32>, f: Seen) -> bool {
    let e = vec3<f32>(dot(d, f.ab), dot(d, f.bc), dot(d, f.ca));
    let tol = 1.0e-6;
    return dot(d, f.mid) > 0.0 && (all(e >= vec3<f32>(-tol)) || all(e <= vec3<f32>(tol)));
}

@compute @workgroup_size(64)
fn raster(@builtin(global_invocation_id) id: vec3<u32>) {
    let f = facet_id(id);
    if f >= params.n_facets {
        return;
    }
    let a = vertex(indices[3u * f]);
    let b = vertex(indices[3u * f + 1u]);
    let c = vertex(indices[3u * f + 2u]);
    let n = cross(b - a, c - a);
    let facet = seen(a, b, c);
    // A plane facet rises no higher than its highest corner: where a
    // direction just past a steep facet's edge -- taken in by the tolerance
    // -- meets its plane far off, that is where it stops.
    let r_lo = min(length(a), min(length(b), length(c)));
    let r_hi = max(length(a), max(length(b), length(c)));
    let la = lon_lat(a);
    let lb = lon_lat(b);
    let lc = lon_lat(c);
    // Longitudes about the first corner's, so a facet across the
    // antimeridian is one interval.
    var lon = vec3<f32>(la.x, la.x + atan2(sin(lb.x - la.x), cos(lb.x - la.x)), la.x + atan2(sin(lc.x - la.x), cos(lc.x - la.x)));
    var lo = vec2<f32>(min(lon.x, min(lon.y, lon.z)), min(la.y, min(lb.y, lc.y)));
    var hi = vec2<f32>(max(lon.x, max(lon.y, lon.z)), max(la.y, max(lb.y, lc.y)));
    // Round a pole, every longitude.
    if meets(vec3<f32>(0.0, 0.0, 1.0), facet) {
        lo.x = -PI;
        hi = vec2<f32>(PI, 0.5 * PI);
    }
    if meets(vec3<f32>(0.0, 0.0, -1.0), facet) {
        lo = vec2<f32>(-PI, -0.5 * PI);
        hi.x = PI;
    }
    if params.grid == 0u {
        let size = vec2<i32>(params.coarse_size);
        let cell = vec2<f32>(2.0 * PI, PI) / vec2<f32>(params.coarse_size);
        let i0 = i32(floor((lo.x + PI) / cell.x)) - 1;
        let i1 = i32(floor((hi.x + PI) / cell.x)) + 1;
        let j0 = max(i32(floor((0.5 * PI - hi.y) / cell.y)) - 1, 0);
        let j1 = min(i32(floor((0.5 * PI - lo.y) / cell.y)) + 1, size.y - 1);
        for (var j = j0; j <= j1; j++) {
            for (var i = i0; i <= min(i1, i0 + size.x - 1); i++) {
                let ii = ((i % size.x) + size.x) % size.x;
                let ll = coarse_cell(ii, j);
                let d = direction(ll.x, ll.y);
                if meets(d, facet) {
                    coarse[u32(j * size.x + ii)] = clamp(dot(n, a) / dot(n, d), 0.9 * r_lo, r_hi);
                }
            }
        }
    } else {
        let size = vec2<i32>(params.fine_size);
        // The facet's longitudes from the grid's corner, wrapped about the
        // middle of the longitudes the grid leaves out: a facet across its
        // west edge starts before it.
        let gap = 0.5 * max(2.0 * PI - f32(size.x) * params.fine_cell.x, 0.0);
        let start = wrap(lo.x - params.fine_lo.x + gap) - gap;
        let i0 = i32(floor(start / params.fine_cell.x)) - 1;
        let i1 = i32(floor((start + hi.x - lo.x) / params.fine_cell.x)) + 1;
        let j0 = max(i32(floor((lo.y - params.fine_lo.y) / params.fine_cell.y)) - 1, 0);
        let j1 = min(i32(floor((hi.y - params.fine_lo.y) / params.fine_cell.y)) + 1, size.y - 1);
        if i0 >= size.x || i1 < 0 || j0 > j1 {
            return;
        }
        for (var j = j0; j <= j1; j++) {
            for (var i = max(i0, 0); i <= min(i1, size.x - 1); i++) {
                let ll = fine_cell(i, j);
                let d = direction(ll.x, ll.y);
                if meets(d, facet) {
                    fine[u32(j * size.x + i)] = clamp(dot(n, a) / dot(n, d), 0.9 * r_lo, r_hi);
                }
            }
        }
    }
}

/// An unclaimed cell, the mean of its claimed neighbours; one pass a call.
@compute @workgroup_size(8, 8)
fn fill(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = select(params.coarse_size, params.fine_size, params.grid == 1u);
    if any(id.xy >= size) {
        return;
    }
    let s = vec2<i32>(size);
    let p = vec2<i32>(id.xy);
    let at = u32(p.y * s.x + p.x);
    var here: f32;
    if params.grid == 0u { here = coarse[at]; } else { here = fine[at]; }
    if here != EMPTY {
        return;
    }
    var sum = 0.0;
    var n = 0.0;
    for (var k = 0; k < 9; k++) {
        var q = p + vec2<i32>(k % 3 - 1, k / 3 - 1);
        if params.grid == 0u {
            q.x = (q.x + s.x) % s.x;
        }
        if any(q < vec2<i32>(0)) || any(q >= s) {
            continue;
        }
        var v: f32;
        if params.grid == 0u { v = coarse[u32(q.y * s.x + q.x)]; } else { v = fine[u32(q.y * s.x + q.x)]; }
        if v != EMPTY {
            sum += v;
            n += 1.0;
        }
    }
    if n > 0.0 {
        if params.grid == 0u { coarse[at] = sum / n; } else { fine[at] = sum / n; }
    }
}

/// The coarse grid at `ll`, bilinear, longitudes wrapping.
fn coarse_at(ll: vec2<f32>) -> f32 {
    let size = vec2<i32>(params.coarse_size);
    let x = (ll.x + PI) / (2.0 * PI) * f32(size.x) - 0.5;
    let y = (0.5 * PI - ll.y) / PI * f32(size.y) - 0.5;
    let i = i32(floor(x));
    let j = i32(floor(y));
    let fx = x - f32(i);
    let fy = y - f32(j);
    let i0 = ((i % size.x) + size.x) % size.x;
    let i1 = (i0 + 1) % size.x;
    let j0 = clamp(j, 0, size.y - 1);
    let j1 = clamp(j + 1, 0, size.y - 1);
    let r00 = coarse[u32(j0 * size.x + i0)];
    let r10 = coarse[u32(j0 * size.x + i1)];
    let r01 = coarse[u32(j1 * size.x + i0)];
    let r11 = coarse[u32(j1 * size.x + i1)];
    return mix(mix(r00, r10, fx), mix(r01, r11, fx), fy);
}

/// The fine grid at `ll`, bilinear, or `EMPTY` off it.
fn fine_at(ll: vec2<f32>) -> f32 {
    let size = vec2<i32>(params.fine_size);
    let x = wrap(ll.x - params.fine_lo.x) / params.fine_cell.x - 0.5;
    let y = (ll.y - params.fine_lo.y) / params.fine_cell.y - 0.5;
    if x < 0.0 || y < 0.0 || x > f32(size.x - 1) || y > f32(size.y - 1) {
        return EMPTY;
    }
    let i = min(i32(floor(x)), size.x - 2);
    let j = min(i32(floor(y)), size.y - 2);
    let fx = x - f32(i);
    let fy = y - f32(j);
    let r00 = fine[u32(j * size.x + i)];
    let r10 = fine[u32(j * size.x + i + 1)];
    let r01 = fine[u32((j + 1) * size.x + i)];
    let r11 = fine[u32((j + 1) * size.x + i + 1)];
    // A cell no facet claimed and the fill did not reach: the coarse grid's.
    if min(min(r00, r10), min(r01, r11)) == EMPTY {
        return EMPTY;
    }
    return mix(mix(r00, r10, fx), mix(r01, r11, fx), fy);
}

/// The surface's radius in direction `d`: the fine grid while `near` and
/// on it, the coarse one else.
fn radius(d: vec3<f32>, near: bool) -> f32 {
    let ll = lon_lat(d);
    if near && params.fine_size.x > 1u {
        let r = fine_at(ll);
        if r != EMPTY {
            return r;
        }
    }
    return coarse_at(ll);
}

/// A facet's local frame, as `horizon_seen` in `mesh_shadow.wgsl` builds it
/// in the world: east along the spin axis cross the radius, north up it.
fn east_of(up: vec3<f32>) -> vec3<f32> {
    let e = cross(vec3<f32>(0.0, 0.0, 1.0), up);
    if dot(e, e) < 1.0e-12 {
        return vec3<f32>(0.0, 1.0, 0.0);
    }
    return normalize(e);
}

/// Sine of the elevation of terrain at radius `r`, `theta` along the
/// surface, from an eye at radius `r0`: written with the half angle, as a
/// metre over thousands of kilometres would cancel away otherwise.
fn sin_elevation(r: f32, r0: f32, theta: f32) -> f32 {
    let h = sin(0.5 * theta);
    let rise = (r - r0) - 2.0 * r * h * h;
    return rise / sqrt((r - r0) * (r - r0) + 4.0 * r * r0 * h * h);
}

fn snorm16(x: f32) -> u32 {
    return u32(i32(round(clamp(x, -1.0, 1.0) * 32767.0)) & 0xffff);
}

/// One facet, two of its azimuths: `k = 2 m` and `2 m + 1`. Invocations
/// side by side take neighbouring facets in the same azimuths (`order`), so
/// a SIMD group marches one way over one stretch of the grids and reads
/// them together: by facet, its 32 lanes went 16 ways, and the march took
/// 55 s on Mars's 12.9M facets; in the mesh's own order of facets, which
/// is not where they are, 139 s.
@compute @workgroup_size(64)
fn march(@builtin(global_invocation_id) id: vec3<u32>) {
    let n = facet_id(id);
    if n >= params.n_facets * (AZIMUTHS / 2u) {
        return;
    }
    let f = order[n % params.n_facets];
    let m = n / params.n_facets;
    let w = f * (AZIMUTHS / 2u) + m;
    let a = vertex(indices[3u * f]);
    let b = vertex(indices[3u * f + 1u]);
    let c = vertex(indices[3u * f + 2u]);
    let up = normalize(a + b + c);
    let east = east_of(up);
    let north = cross(up, east);
    // The eye on the grids' surface, as the terrain it is measured against.
    let r0 = radius(up, true);
    let ll = lon_lat(up);
    let cell = vec2<i32>(i32((ll.x + PI) / (2.0 * PI) * 90.0) % 90, clamp(i32((0.5 * PI - ll.y) / PI * 45.0), 0, 44));
    let top = highest[u32(cell.y * 90 + cell.x)];
    var out = 0u;
    for (var half = 0u; half < 2u; half++) {
        let k = 2u * m + half;
        let az = (f32(k) + 0.5) * 2.0 * PI / f32(AZIMUTHS);
        let t = cos(az) * north + sin(az) * east;
        var best = -1.0;
        var s = params.first_step;
        let coarse_angle = 2.0 * PI / f32(params.coarse_size.x);
        for (var guard = 0; guard < 4096; guard++) {
            let theta = s / r0;
            if theta > 0.5 * PI {
                break;
            }
            let q = cos(theta) * up + sin(theta) * t;
            let near = s < params.fine_reach;
            best = max(best, sin_elevation(radius(q, near), r0, theta));
            // Nothing farther can rise above that: the highest the terrain
            // goes, this far, is already below it, and farther is lower.
            if sin_elevation(top, r0, theta) < best {
                break;
            }
            // Steps of half a cell of the grid read, at least.
            let least = select(0.5 * coarse_angle * r0, params.first_step, near);
            s += clamp(s * params.growth, least, params.max_step);
        }
        out |= snorm16(best) << (16u * half);
        let q = i32(round(clamp(best, -1.0, 1.0) * 32767.0));
        atomicMax(&top_horizon[0], q);
        let i = min(u32((ll.x + PI) / (2.0 * PI) * f32(TOP_CELLS.x)), TOP_CELLS.x - 1u);
        let j = min(u32((0.5 * PI - ll.y) / PI * f32(TOP_CELLS.y)), TOP_CELLS.y - 1u);
        atomicMax(&top_horizon[1u + j * TOP_CELLS.x + i], q);
    }
    horizons[w] = out;
}
