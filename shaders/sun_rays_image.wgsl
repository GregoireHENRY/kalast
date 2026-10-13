// Light bounced off the surfaces (`reference.bounces`): the main pass's own
// ray code, put in at its `//@rays_image` line by `gpu::shader_for` on a
// device with ray queries. The bodies' surfaces, as `raytrace::Geometry`
// lays them out: every body's vertices in one buffer, its facets' indices
// into it, where each body's facets begin, how each body reflects -- three
// `vec4` a body, its facets' mean colour and its law's number, then the
// law's parameters, as `law_factor` takes them.
@group(7) @binding(1) var<storage, read> ray_positions: array<f32>;
@group(7) @binding(2) var<storage, read> ray_indices: array<u32>;
@group(7) @binding(3) var<storage, read> ray_index_base: array<u32>;
@group(7) @binding(4) var<storage, read> ray_surface: array<vec4<f32>>;

// Halton's bases for the bounces, four a bounce -- the direction's two and
// the disc's two at the surface met -- after the pixel's 2 and 3 and the
// disc's 5 and 7.
const RAY_BOUNCE_BASES = array<u32, 16>(11u, 13u, 17u, 19u, 23u, 29u, 31u, 37u, 41u, 43u, 47u, 53u, 59u, 61u, 67u, 71u);

fn ray_vertex(i: u32) -> vec3<f32> {
    return vec3<f32>(ray_positions[3u * i], ray_positions[3u * i + 1u], ray_positions[3u * i + 2u]);
}

/// Two numbers in [0, 1): Halton's `b0` and `b1` at `i`, shifted by `key`'s
/// fixed amount (Cranley-Patterson).
fn ray_pair(i: u32, b0: u32, b1: u32, key: u32) -> vec2<f32> {
    return fract(vec2<f32>(ray_radical_inverse(i, b0), ray_radical_inverse(i, b1))
        + vec2<f32>(ray_unit(key), ray_unit(key ^ 0x9e3779b9u)));
}

/// The I/F at `p`, facing `n` and seen along `v`, of the sunlight bounced
/// `bounces` times off the surfaces on its way there, per unit of the colour
/// at `p`, whose law is `law` with parameters `la` and `lb`; for reference
/// sample `k` of `pixel` (`ray_reference`).
///
/// A path: from each point a direction drawn as a Lambert surface scatters
/// -- cosine-weighted -- to the surface it meets, whose sunlight (one ray to
/// a point of the disc) it sends back along the path, and on from there.
/// Every surface on the way reflects by its own law at the angles the path
/// makes there (`law_factor`): the point seen by its body's, a surface met
/// by its body's and the mean of its facets' colours (`raytrace::Geometry`),
/// from either side. Drawn as they are, a direction's share is the law's
/// I/F over the cosine of the light's incidence: the colour itself for a
/// Lambert surface. A ray that leaves for space brings nothing: there is no
/// sky.
fn ray_bounced(p: vec3<f32>, n: vec3<f32>, v: vec3<f32>, law: u32, la: vec4<f32>, lb: vec4<f32>, sun: vec3<f32>, radius: f32, k: u32, pixel: vec2<u32>, bounces: u32) -> vec3<f32> {
    let replica = k % 4u;
    let i = k / 4u + 1u;
    let base_key = (pixel.x * 73856093u) ^ (pixel.y * 19349663u) ^ (replica * 83492791u);
    // The point the light reaches, which way it leaves from there -- to the
    // camera, then back along the path -- and how that surface reflects.
    var at = p;
    var normal = n;
    var out = v;
    var here_law = law;
    var here_a = la;
    var here_b = lb;
    var here_colour = vec3<f32>(1.0);
    var carried = vec3<f32>(1.0);
    var light = vec3<f32>(0.0);
    // Declared once, outside the loop: inside it naga resets it on each pass
    // by assigning a fresh query, and Metal's has no assignment -- the main
    // pass did not build on the M1 Pro.
    var rq: ray_query;
    for (var b = 0u; b < min(bounces, 4u); b++) {
        let key = base_key ^ ((b + 1u) * 0x632be5abu);
        let ud = ray_pair(i, RAY_BOUNCE_BASES[4u * b], RAY_BOUNCE_BASES[4u * b + 1u], key);
        let us = ray_pair(i, RAY_BOUNCE_BASES[4u * b + 2u], RAY_BOUNCE_BASES[4u * b + 3u], key ^ 0x51ed270bu);
        // Cosine-weighted about the normal.
        let side = select(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(1.0, 0.0, 0.0), abs(normal.z) > 0.9);
        let t1 = normalize(cross(normal, side));
        let t2 = cross(normal, t1);
        let r = sqrt(ud.x);
        let phi = RAY_TAU * ud.y;
        let dir = normalize(r * cos(phi) * t1 + r * sin(phi) * t2 + sqrt(max(1.0 - ud.x, 0.0)) * normal);
        // The surface here, lit from along `dir` and seen from along `out`.
        let mu_in = dot(normal, dir);
        let alpha = acos(clamp(dot(dir, out), -1.0, 1.0));
        carried *= here_colour * law_factor(here_law, here_a, here_b, mu_in, dot(normal, out), alpha) / max(mu_in, 1.0e-6);
        let origin = ray_origin(at, normal);
        rayQueryInitialize(&rq, tlas, RayDesc(RAY_FLAG_FORCE_OPAQUE, 0xffu, 0.0, 1.0e30, origin, dir));
        rayQueryProceed(&rq);
        let hit = rayQueryGetCommittedIntersection(&rq);
        if hit.kind == RAY_QUERY_INTERSECTION_NONE {
            break;
        }
        let body = hit.instance_custom_data;
        let f = ray_index_base[body] + 3u * hit.primitive_index;
        let a = ray_vertex(ray_indices[f]);
        let e1 = ray_vertex(ray_indices[f + 1u]) - a;
        let e2 = ray_vertex(ray_indices[f + 2u]) - a;
        var nq = normalize((hit.object_to_world * vec4<f32>(cross(e1, e2), 0.0)).xyz);
        // Met from behind, it is lit and seen from that side.
        if dot(nq, dir) > 0.0 {
            nq = -nq;
        }
        let q = origin + dir * hit.t;
        let surface = ray_surface[3u * body];
        here_law = u32(surface.w);
        here_a = ray_surface[3u * body + 1u];
        here_b = ray_surface[3u * body + 2u];
        here_colour = surface.rgb;
        // The Sun's light on the surface met, through one ray to the disc,
        // sent back along the path by its law.
        let to_sun = normalize(sun - q);
        let mu = dot(nq, to_sun);
        if mu > 0.0 {
            let rs = sqrt(ray_limb_radius2(us.x));
            let sphi = RAY_TAU * us.y;
            let sside = select(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(1.0, 0.0, 0.0), abs(to_sun.z) > 0.9);
            let s1 = normalize(cross(to_sun, sside));
            let s2 = cross(to_sun, s1);
            let disc = select(rs * vec2<f32>(cos(sphi), sin(sphi)), vec2<f32>(0.0), radius <= 0.0);
            if !ray_to_disc(ray_origin(q, nq), sun, radius, s1, s2, disc) {
                let back = acos(clamp(dot(to_sun, -dir), -1.0, 1.0));
                light += carried * here_colour * law_factor(here_law, here_a, here_b, mu, dot(nq, -dir), back);
            }
        }
        at = q;
        normal = nq;
        out = -dir;
    }
    return light;
}
