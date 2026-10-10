// The Sun's light traced with rays, shared by the shaders that do: put in at
// their `//@rays` line by `gpu::shader_for`, on a device with ray queries
// alone. They declare `tlas`, the bodies' acceleration structure, and
// `LIMB_DARKENING`; everything here is `ray_`-named to keep clear of theirs.

const RAY_GOLDEN_ANGLE: f32 = 2.3999632;
const RAY_TAU: f32 = 6.2831855;

/// The squared radius, on the disc of radius 1, inside which a share `t` of
/// the limb-darkened disc's light falls: the radial CDF inverted by Newton.
/// With x = r^2 and mu = sqrt(1 - x), the light inside is
/// (1 - u) x + u 2/3 (1 - mu^3), of 1 - u/3 in all.
fn ray_limb_radius2(t: f32) -> f32 {
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
fn ray_disc_sample(i: u32, n: u32, turn: f32) -> vec2<f32> {
    let r = sqrt(ray_limb_radius2((f32(i) + 0.5) / f32(n)));
    let phi = f32(i) * RAY_GOLDEN_ANGLE + turn;
    return r * vec2<f32>(cos(phi), sin(phi));
}

/// The ray's origin off the surface at `p`, facing `n`, by a few units in
/// the last place of its coordinates: the surface it starts on is not hit
/// again, and relief a millimetre high still is (Waechter & Binder, Ray
/// Tracing Gems ch. 6). A fixed offset of 1 cm passed over bumps of 1-2 cm
/// (`notes/2026-10-09_disc_gaps_hidden_relief/`).
fn ray_origin(p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
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
fn ray_blocked(origin: vec3<f32>, dir: vec3<f32>, len: f32) -> bool {
    var rq: ray_query;
    rayQueryInitialize(&rq, tlas, RayDesc(RAY_FLAG_TERMINATE_ON_FIRST_HIT | RAY_FLAG_FORCE_OPAQUE, 0xffu, 0.0, len, origin, dir));
    rayQueryProceed(&rq);
    return rayQueryGetCommittedIntersection(&rq).kind != RAY_QUERY_INTERSECTION_NONE;
}

/// Whether the ray from `origin` to the point `s` of the disc -- of radius 1,
/// across `t1` and `t2` -- of the Sun at `sun`, of radius `radius`, is
/// blocked.
fn ray_to_disc(origin: vec3<f32>, sun: vec3<f32>, radius: f32, t1: vec3<f32>, t2: vec3<f32>, s: vec2<f32>) -> bool {
    let v = sun + radius * (s.x * t1 + s.y * t2) - origin;
    let len = length(v);
    return ray_blocked(origin, v / len, len);
}

/// The point of the meshes themselves the camera at `camera` sees through
/// `p`, a fragment's position. That position is not on the triangle the rays
/// are tested against -- rasterised, interpolated, or a level of detail's
/// coarser cut -- and rays from it hit their own surface: sunlit ground
/// speckled black at full resolution, 61,816 pixels of Didymos darker with
/// the cut, which passes under the relief. `p` itself where the camera's ray
/// misses them.
///
/// The ray starts `back` in front of `p` along the camera's line, not at the
/// camera: a point found at the camera's distance carries an error of that
/// distance's last place, far more than a point's own near zero -- a plate
/// at z = 0 seen from 45 above was ringed with black. From close by, the
/// point is as exact as its own coordinates.
fn ray_seen_point(camera: vec3<f32>, p: vec3<f32>, back: f32) -> vec3<f32> {
    let v = p - camera;
    let len = length(v);
    let dir = v / len;
    let step = min(back, 0.5 * len);
    let start = p - dir * step;
    var rq: ray_query;
    rayQueryInitialize(&rq, tlas, RayDesc(RAY_FLAG_FORCE_OPAQUE, 0xffu, 0.0, 2.0 * step, start, dir));
    rayQueryProceed(&rq);
    let hit = rayQueryGetCommittedIntersection(&rq);
    if hit.kind == RAY_QUERY_INTERSECTION_NONE {
        return p;
    }
    return start + dir * hit.t;
}

/// The share of the Sun's light a point `p` of a surface facing `n` sees:
/// none when the surface faces away from the Sun's centre, as the shadow
/// maps have it; one ray for a point Sun (`radius` 0 or `samples` 1),
/// `samples` across its disc. The rays start off the surface toward `away`,
/// its normal.
///
/// `probe` of them first, half on the rim and half inside: where they all
/// agree the point is wholly lit or wholly hidden, as nearly every point is,
/// and the rest are not traced. An occluder's edge comes onto the disc across
/// the rim, so a sliver a few rim rays apart is all that is missed; spread by
/// light, as the samples are, they kept off the dim rim, and a penumbra's
/// edge passed between them -- 4.8 % of the disc at the edge of a wall's
/// shadow. 0 traces all of `samples`.
fn ray_sun_seen(p: vec3<f32>, n: vec3<f32>, sun: vec3<f32>, radius: f32, samples: u32, probe: u32, turn: f32) -> f32 {
    let to_sun = sun - p;
    let dist = length(to_sun);
    let d = to_sun / dist;
    if dot(n, d) <= 0.0 {
        return 0.0;
    }
    let origin = ray_origin(p, n);
    if radius <= 0.0 || samples <= 1u {
        return select(1.0, 0.0, ray_blocked(origin, d, dist));
    }
    // Two directions across the disc as the point sees it.
    let side = select(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(1.0, 0.0, 0.0), abs(d.z) > 0.9);
    let t1 = normalize(cross(d, side));
    let t2 = cross(d, t1);
    if probe > 0u && samples > probe {
        let rim = probe / 2u;
        var hidden = 0u;
        for (var i = 0u; i < probe; i++) {
            var s: vec2<f32>;
            if i < rim {
                let phi = (f32(i) + 0.5) * (RAY_TAU / f32(rim)) + turn;
                s = 0.999 * vec2<f32>(cos(phi), sin(phi));
            } else {
                s = ray_disc_sample(i - rim, probe - rim, turn);
            }
            if ray_to_disc(origin, sun, radius, t1, t2, s) {
                hidden += 1u;
            }
        }
        if hidden == 0u {
            return 1.0;
        }
        if hidden == probe {
            return 0.0;
        }
    }
    var seen = 0u;
    for (var i = 0u; i < samples; i++) {
        if !ray_to_disc(origin, sun, radius, t1, t2, ray_disc_sample(i, samples, turn)) {
            seen += 1u;
        }
    }
    return f32(seen) / f32(samples);
}

/// A turn of the disc's pattern for each point, so neighbouring points do
/// not all miss the same part of a penumbra. Fixed, as the pattern is.
fn ray_turn(key: u32) -> f32 {
    var h = key * 747796405u + 2891336453u;
    h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
    h = (h >> 22u) ^ h;
    return f32(h) * (RAY_TAU / 4294967296.0);
}

/// A number in [0, 1) from `key`, fixed: the random shift of a reference
/// image's sequences for one pixel and one replica (`ray_reference`).
fn ray_unit(key: u32) -> f32 {
    return ray_turn(key) / RAY_TAU;
}

/// The radical inverse of `i` in `base`: the Halton sequence's coordinate,
/// low discrepancy, the same on every run.
fn ray_radical_inverse(i: u32, base: u32) -> f32 {
    var n = i;
    var inv = 1.0 / f32(base);
    var f = inv;
    var r = 0.0;
    while n > 0u {
        r += f32(n % base) * f;
        n /= base;
        f *= inv;
    }
    return r;
}

/// A reference image's share of the Sun a point sees, from one ray: to the
/// point of the limb-darkened disc that sample `k` of the progressive sum
/// stands for (`reference`), every ray the same share of its light. Sample
/// `k` is replica `k % 4`'s `k / 4`-th: Halton's bases 5 and 7 -- 2 and 3
/// jitter the pixel, on the CPU -- shifted by a fixed random amount per
/// pixel and replica (Cranley-Patterson), so the four replicas are
/// independent estimates and their spread an honest error. Radius and angle
/// both drawn anew each sample: a fixed spiral's radii, only turned, would
/// leave its quadrature error in the mean however long it ran.
fn ray_reference(p: vec3<f32>, n: vec3<f32>, sun: vec3<f32>, radius: f32, k: u32, pixel: vec2<u32>) -> f32 {
    let to_sun = sun - p;
    let dist = length(to_sun);
    let d = to_sun / dist;
    if dot(n, d) <= 0.0 {
        return 0.0;
    }
    let origin = ray_origin(p, n);
    if radius <= 0.0 {
        return select(1.0, 0.0, ray_blocked(origin, d, dist));
    }
    let replica = k % 4u;
    let i = k / 4u + 1u;
    let key = (pixel.x * 73856093u) ^ (pixel.y * 19349663u) ^ (replica * 83492791u);
    let u = fract(vec2<f32>(ray_radical_inverse(i, 5u), ray_radical_inverse(i, 7u))
        + vec2<f32>(ray_unit(key), ray_unit(key ^ 0x9e3779b9u)));
    let r = sqrt(ray_limb_radius2(u.x));
    let phi = RAY_TAU * u.y;
    let side = select(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(1.0, 0.0, 0.0), abs(d.z) > 0.9);
    let t1 = normalize(cross(d, side));
    let t2 = cross(d, t1);
    return select(1.0, 0.0, ray_to_disc(origin, sun, radius, t1, t2, r * vec2<f32>(cos(phi), sin(phi))));
}
