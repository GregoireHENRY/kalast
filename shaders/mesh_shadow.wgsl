// Lines tagged `//@prim` are kept only on a device with PRIMITIVE_INDEX,
// lines tagged `//@noprim` only without it -- see `gpu::shader_for`. With
// it, a flat mesh is drawn indexed over its shared vertices and the
// fragment stage finds the facet by the primitive index; without it, the
// old non-indexed draw, where `vertex_index / 3` is the facet.
enable primitive_index; //@prim

struct Globals {
    color: vec3<f32>,
    color_mode: u32,
    srgb_mode: u32,
    gamma: f32,
    ambient_strength: f32,
    light_cube_scale: f32,
    shadow_resolution: u32,
    shadow_bias_scale: f32,
    shadow_bias_minimum: f32,
    shadow_normal_offset_scale: f32,
    shadow_pcf: u32,
    extra: u32,
    // 0: shaded only, 1: wireframe only, 2: wireframe over shaded
    wireframe_mode: u32,
    wireframe_width: f32,
    wireframe_color: vec3<f32>,
    wireframe_fade: u32,
    value_min: f32,
    value_max: f32,
    wireframe_antialias: u32,
    axes_antialias: u32,
    image_size: vec2<f32>,
    // At 112, past the padding: the camera, for the direction each fragment
    // is seen from. See Globals in app/uniform.rs.
    camera_pos: vec3<f32>,
};
@group(0) @binding(0)
var<uniform> globals: Globals;

struct Camera {
    view_proj: mat4x4<f32>,
};

struct Light {
    // Scratch the shadow pass draws with, rewritten per layer. Not used here.
    view_proj: mat4x4<f32>,
    // One per body: aimed at it and sized to it, with depth spanning the
    // scene so occluders still cast into it.
    view_proj_layers: array<mat4x4<f32>, 8>,
    // Per layer: (normal_offset_scale, bias_scale, bias_minimum, texel_depth).
    // Per layer because each covers a different world extent at the same
    // texel count, so one texel is a different distance in each.
    layer_bias: array<vec4<f32>, 8>,
    pos: vec3<f32>,
    n_layers: u32,
    color: vec3<f32>,
    // The Sun's radius, scene units; 0 for a point (`light.sun_as_point`).
    sun_radius: f32,
    // Bodies with an atmosphere, the first two: per one, its centre and
    // equatorial radius, z axis and polar radius, dust optical depth and
    // scale height (`through_air`).
    air_count: u32,
    // 1: their solid shadow is the ellipsoid's too; 0: the shadow map's.
    air_solid: u32,
    // 1: the penumbra pass ran this frame (`fs_penumbra`, `cs_walk`), so
    // the main pass reads what it found (`sun_walked`).
    penumbra: u32,
    // Bit `i`: layer `i`'s body and the other bodies are in slices of
    // their own, the others' after the layers (`others_slice`).
    apart: u32,
    air: array<vec4<f32>, 6>,
    // 1: each layer has a second depth layer, the nearest surface behind its
    // first, after the others' slices (`peel_slice`).
    peeled: u32,
    // Cascaded shadow maps (`shadows.cascades`): the layers over the camera's
    // view before the scene's; 0, a layer per body.
    cascades: u32,
};

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
    @location(12) normal_row_0: vec4<f32>,
    @location(13) normal_row_1: vec4<f32>,
    @location(14) normal_row_2: vec4<f32>,
    @location(15) normal_row_3: vec4<f32>,
    // Bit 0: mesh is flat (non-indexed), so barycentrics can be recovered
    // from vertex_index. See INSTANCE_FLAG_FLAT in app/gpu.rs.
    @location(16) flags: u32,
    // Which shadow layer shades this body; see InstanceInput in app/gpu.rs.
    @location(17) shadow_layer: u32,
    // The body's scattering law and its numbers; see `law_factor`.
    @location(18) law: u32,
    @location(19) law_params: vec4<f32>,
    @location(20) law_params2: vec4<f32>,
};

// Shared with the colorbar, so the surface and the bar cannot disagree about
// what a colour means.
struct Colormap {
    lut: array<vec4<f32>, 256>,
};
@group(3) @binding(0)
var<uniform> colormap: Colormap;

/// Colour for a value, clamped to the ends of the range.
///
/// Clamped rather than wrapped: an outlier should saturate at the top of the
/// scale, not alias back to the bottom and read as a cold facet.
fn colormap_lookup(v: f32) -> vec3<f32> {
    let span = max(globals.value_max - globals.value_min, 1e-20);
    let t = clamp((v - globals.value_min) / span, 0.0, 1.0);
    let x = t * 255.0;
    let i = u32(floor(x));
    let j = min(i + 1u, 255u);
    // Interpolated between entries: a 256-step ramp banded visibly on a
    // smooth field at the sizes these figures get printed at.
    return mix(colormap.lut[i].rgb, colormap.lut[j].rgb, x - f32(i));
}

struct VertexInput {
    @location(0) pos: vec3<f32>,
};

/// Everything a surface has that is not its position: **one entry per facet**
/// for a flat mesh, one per vertex for a smooth one. These were four more
/// vertex attributes, 20 bytes on every corner of every mesh -- and three
/// times over for a flat one, whose three corners carry the same facet
/// normal, colour and value between them. Keep in step with `MeshAttr` in
/// `app/gpu.rs`.
struct MeshAttr {
    normal: vec3<f32>,
    value: f32,
    color: vec3<f32>,
    mode: u32,
};

@group(5) @binding(0) var<storage, read> attrs: array<MeshAttr>;

// A body drawn by its level-of-detail cut (flag bit 3, `gpu::LodBuffers`):
// the cut's triangles are not the mesh's own, and each stands for the facet
// `facet_of` names, whose colour, mode and value it shows. `facet_base` is
// the draw's first triangle, set per draw; `skirt_base` the first of the
// cut's vertices that is a skirt's, which hangs from the vertex of
// `lod_positions` that `skirt_top` names.
@group(5) @binding(1) var<storage, read> facet_of: array<u32>;
@group(5) @binding(3) var<storage, read> lod_positions: array<f32>;
@group(5) @binding(4) var<storage, read> skirt_top: array<u32>;
// A body's horizon map (flag bit 4, `app::horizon`): per facet, the sine of
// how high its terrain rises in each of `HORIZON_AZIMUTHS` directions, a
// snorm16 each, two to a word; then per degree of longitude and latitude
// (`HORIZON_TOPS`), the highest of the horizons of the facets there.
@group(5) @binding(5) var<storage, read> horizons: array<u32>;
struct Chunk {
    facet_base: u32,
    skirt_base: u32,
};
var<immediate> chunk: Chunk; //@imm

// The body's own numbers, for the fragment stage: the instance buffer itself,
// `InstanceInput` in `app/gpu.rs`, read where they are used. They were
// flat outputs of the vertex stage, 80 bytes on every vertex -- which a
// tiled GPU writes out and reads back per vertex, so on a frame of 2.6 M
// triangles they were most of the traffic between the two stages.
struct Body {
    mat: array<vec4<f32>, 4>,
    normal: array<vec4<f32>, 4>,
    flags: u32,
    shadow_layer: u32,
    law: u32,
    // `law_params` then `law_params2`; scalars, as they sit at byte 140.
    law_params: array<f32, 8>,
    // 1 with an atmosphere, then its numbers: tau, scale height, radius,
    // omega, g1, g2, q, the surroundings' albedo (-1: the facet's own), the
    // polar radius, and how bright the surface is under a diffuse sky per
    // unit of colour -- the law's albedo for it, 1 for Lambert.
    atmosphere_on: u32,
    atmosphere: array<f32, 12>,
    // With a horizon map, the sine of the highest any of its horizons rise.
    horizon_top: f32,
    // Seen up close, the layer over where the camera looks closest, u32 max
    // without one; how far into it a point uses it, a fraction of its
    // half-side, and how deep, in its depth (`shadow_layer_at`).
    near_layer: u32,
    near_inner: f32,
    near_depth: f32,
};
@group(5) @binding(2) var<storage, read> body: Body;

// Two builds of this file (`gpu::shader_for`): the full one, and a lean one
// for a flat mesh drawn indexed without the wireframe -- what a large body
// is -- whose surface the fragment stage reads by facet. Lines ending in
// `//@full` are the full build's alone, `//@lean` the lean one's. Lean, a
// vertex carries its clip and world position and nothing else: every byte a
// vertex carries, a tiled GPU writes out and reads back, and at a couple of
// million triangles a frame that traffic was most of the pass.
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(1) color: vec3<f32>, //@full
    @location(2) world_normal: vec3<f32>, //@full
    @location(3) world_pos: vec3<f32>,
    // Barycentric coordinate of this corner, interpolated across the
    // triangle so the fragment shader knows its distance to each edge.
    @location(4) bary: vec3<f32>, //@full
    // Flat: every corner of a facet carries the same value, and interpolating
    // would smear one facet's datum into its neighbour.
    @location(7) @interpolate(flat) value: f32, //@full
    // Per-vertex colour mode, overriding the global one for this facet alone.
    // Flat, like the others: a mode must not be blended across a triangle.
    @location(8) @interpolate(flat) color_mode: u32, //@full
    // The facet, on a device without `primitive_index`: `vertex_index / 3`
    // of a non-indexed flat draw.
    @location(12) @interpolate(flat) facet: u32, //@noprim
};

/// The surface `fs_shaded` shades: from the vertex stage for a smooth mesh,
/// from the facet's attributes for a flat one.
struct Surface {
    color: vec3<f32>,
    world_normal: vec3<f32>,
    world_pos: vec3<f32>,
    value: f32,
    color_mode: u32,
    // The fragment's window position and depth, and how much the depth
    // changes over a pixel: where the penumbra pass's answer for this pixel
    // is, and whether it was for this surface (`sun_walked`).
    frag: vec4<f32>,
    dz: f32,
    // The mesh's facet, for its horizon (`horizon_seen`).
    facet: u32,
};

// sRGB's own curve, decoding: the image is stored through its encoding, which
// then undoes this exactly. `shading.gamma` set (above 0) uses the power law
// instead, which is how this was done before 7 October -- and 2.2 parted from
// the curve in the dark: a lit 0.078 was stored as 0.047.
fn srgb_to_linear(color: vec3<f32>, gamma: f32) -> vec3<f32> {
    if gamma > 0.0 {
        return pow(color, vec3<f32>(gamma));
    }
    let low = color / 12.92;
    let high = pow((max(color, vec3<f32>(0.0)) + 0.055) / 1.055, vec3<f32>(2.4));
    return select(high, low, color <= vec3<f32>(0.04045));
}

@vertex
fn vs_main(
    vertex: VertexInput,
    instance: InstanceInput,
    @builtin(vertex_index) vertex_index: u32,
) -> VertexOutput {
    let model_matrix = mat4x4<f32>(
        instance.mat_row_0,
        instance.mat_row_1,
        instance.mat_row_2,
        instance.mat_row_3,
    );

    let normal_matrix = mat3x3<f32>(
        instance.normal_row_0.xyz,
        instance.normal_row_1.xyz,
        instance.normal_row_2.xyz,
    );

    var out: VertexOutput;

    // A smooth mesh is drawn indexed, `vertex_index` is the vertex's own id
    // and its attributes are its own, read here and interpolated. A flat
    // mesh's attributes are per facet, and a facet is only known in the
    // fragment stage (`fs_main`), so its vertex stage does the position and
    // nothing else: on a 3M-facet model that is 1.6 M invocations of this
    // instead of 9.4 M, drawn indexed over the shared vertices.
    let flat = (instance.flags & 1u) != 0u;
    if !flat {
        let attr = attrs[vertex_index]; //@full
        out.color = attr.color; //@full
        out.color_mode = attr.mode; //@full
        out.world_normal = normalize(normal_matrix * attr.normal); //@full
        out.value = attr.value; //@full
    }
    out.facet = vertex_index / 3u; //@noprim

    var world_pos = model_matrix * vec4<f32>(vertex.pos, 1.0);
    out.clip_position = view.camera.view_proj * world_pos;

    // A skirt is lit as the outline it hangs from (`lod`): where one shows,
    // in the gap between two patches, it stands in for the surface there --
    // and at its own depth, under the surface, every shadow map had it dark.
    if (instance.flags & 8u) != 0u && vertex_index >= chunk.skirt_base { //@imm
        let t = 3u * skirt_top[vertex_index - chunk.skirt_base]; //@imm
        world_pos = model_matrix * vec4<f32>(lod_positions[t], lod_positions[t + 1u], lod_positions[t + 2u], 1.0); //@imm
    } //@imm
    out.world_pos = world_pos.xyz;

    // A flat mesh drawn non-indexed -- one vertex per triangle corner, which
    // is how it is drawn whenever the wireframe is on (flag bit 2) -- has
    // vertex_index modulo 3 *as* the corner index, giving barycentric
    // coordinates for free. Meaningless for a shared-vertex draw, indexed
    // flat or smooth, and the fragment stage does not read it then.
    let corner = vertex_index % 3u;
    out.bary = vec3<f32>(f32(corner == 0u), f32(corner == 1u), f32(corner == 2u)); //@full

    return out;
}

/// A facet smaller than this many pixels shows no wireframe at all; one at
/// least this large shows it in full, with a smoothstep between.
///
/// Deliberately close to the pixel. The wash this exists to stop only happens
/// when a facet's own edges cover its interior, which is a facet of one or two
/// pixels; anything wider than about four draws a line that is genuinely a
/// line. A 3-12 px window looked reasonable and was not: a 5120-facet sphere
/// six units away has ~5 px facets, so the whole body sat inside the ramp and
/// the wireframe was being faded at conversational distances.
const WIRE_FADE_MIN_PX: f32 = 1.0;
const WIRE_FADE_FULL_PX: f32 = 4.0;

/// How much wireframe to draw here, by how well the facet is resolved.
///
/// A facet only a few pixels across has all three of its edges within a line
/// width of every interior pixel, so the "wireframe" covers the whole triangle
/// and the body reads as a sheet of wireframe colour -- a shadowed sphere at
/// distance comes out grey rather than black, which is a lie about the
/// shading and not a drawing of the mesh.
///
/// Same problem the ground grid has, and nearly the same answer: there, a
/// level fades out before its cells stop being resolvable and a coarser one
/// takes over. A mesh has no coarser wireframe to cross-fade to, so this
/// fades to nothing instead.
///
/// `1 / fwidth(bary.i)` is the triangle's **height from vertex i**, in pixels,
/// so the three of them are its three heights and the question is which to
/// measure it by.
///
/// The **largest**, which is `min` over the derivatives. Foreshortening
/// squashes a facet along one screen direction and leaves the perpendicular
/// one alone, so the smallest height collapses as soon as a surface tilts
/// away -- measuring by that faded the wireframe around a sphere's limb at any
/// distance, which is angle doing the work that distance should. The largest
/// height is what survives tilt and shrinks only as the body recedes.
fn wireframe_resolution_fade(bary: vec3<f32>) -> f32 {
    let d = fwidth(bary);
    let size_px = 1.0 / max(min(d.x, min(d.y, d.z)), 1e-8);
    return smoothstep(WIRE_FADE_MIN_PX, WIRE_FADE_FULL_PX, size_px);
}

/// Coverage of the wireframe at this fragment, 0 (interior) to 1 (on an edge).
///
/// Dividing the barycentric by its screen-space derivative converts it to an
/// approximate distance in pixels, so a given width looks the same however
/// far away or however large the triangle is. smoothstep then antialiases the
/// line for free.
fn wireframe_edge(bary: vec3<f32>) -> f32 {
    let d = fwidth(bary);
    let px = bary / max(d, vec3<f32>(1e-8));
    let nearest = min(min(px.x, px.y), px.z);

    // `wireframe.antialias` off: the same line cut where the blend is half,
    // so it keeps its width.
    if globals.wireframe_antialias == 0u {
        return select(0.0, 1.0, nearest < globals.wireframe_width - 0.5);
    }
    return 1.0 - smoothstep(globals.wireframe_width - 1.0, globals.wireframe_width, nearest);
}


/// Depth gradient of the receiver in shadow-map UV space, from its own plane.
///
/// Every PCF tap compares against the *centre* fragment's depth, but a
/// receiver tilted in light space sits at a different depth a few texels
/// away. On the crater's lit wall that darkened 39,219 px at `shadow_pcf = 4`
/// which `shadow_pcf = 0` renders clean -- acne produced by the filter, not
/// by the geometry.
///
/// Derived from the facet normal rather than `dpdx`/`dpdy`. Screen-space
/// derivatives are meaningless across a facet boundary, and on a flat-shaded
/// mesh every pixel is near one: a `dpdx` version of this made the facet-edge
/// leak 6x worse (1,279 px against 215). The normal is constant per facet, so
/// this has no discontinuity to blow up on.
///
/// The light projection is orthographic, hence affine, so differencing two
/// tangent steps is exact and the step length cancels in the solve.
fn receiver_plane_grad(m: mat4x4<f32>, pos: vec3<f32>, n: vec3<f32>) -> vec2<f32> {
    // Any two directions spanning the facet plane.
    var a = vec3<f32>(1.0, 0.0, 0.0);
    if abs(n.x) > 0.9 {
        a = vec3<f32>(0.0, 1.0, 0.0);
    }
    let t1 = normalize(cross(n, a));
    let t2 = cross(n, t1);

    let p0 = project_light(m, pos);
    let d1 = project_light(m, pos + t1) - p0;
    let d2 = project_light(m, pos + t2) - p0;

    let det = d1.x * d2.y - d1.y * d2.x;
    if abs(det) < 1.0e-20 {
        return vec2<f32>(0.0, 0.0);
    }
    return vec2<f32>(d2.y * d1.z - d1.y * d2.z,
                     d1.x * d2.z - d2.x * d1.z) / det;
}

/// World position -> (shadow uv, depth), matching the lookup below exactly.
fn project_light(m: mat4x4<f32>, pos: vec3<f32>) -> vec3<f32> {
    let ls = m * vec4<f32>(pos, 1.0);
    var pr = ls.xyz / ls.w;
    pr.y = -pr.y;
    return vec3<f32>(pr.xy * 0.5 + 0.5, pr.z);
}

@group(2) @binding(0)
var t_shadow: texture_depth_2d_array;
@group(2) @binding(1)
var s_shadow: sampler_comparison;

// The layers' depth pyramid (`gpu::DepthPyramid`): mip `m` holds the depths
// nearest and farthest from the Sun over each block of `4 * 2^m` texels.
@group(2) @binding(2)
var t_shadow_min: texture_2d_array<f32>;

// The Sun as a disc (`light.sun_as_point` off): what each receiver sees of
// it, from the shadow map.
//
// A ray from the receiver toward a point of the disc tilted from the Sun's
// centre by `t` in direction `a` moves `t` across for every unit of depth it
// climbs toward the Sun, along `a`. A texel `u` away along `a` and `dz` in
// front of the receiver is a thin sheet at its depth: the ray crosses that
// depth `t dz` across, and the sheet stops it if that is within the texel,
// so the texel hides the tilts from `(u - w/2) / dz` to `(u + w/2) / dz`, `w`
// its width. Along each of `SUN_AZIMUTHS` directions the receiver walks the
// map and adds up what its texels hide: a band of the disc each, which over
// a continuous surface -- the body's own relief -- join into a horizon, and
// round a body floating in front -- a moon, Dimorphos -- leave the rays that
// pass beside it. The disc is cut, along each direction, into 32 rings of a
// 32nd of its light each (the Sun limb-darkened), and a ring is hidden when
// its middle is: a bit each, so any overlap of shadows, the body's own and
// another's, hides what all of them hide and no more.
//
// A map holds only what the Sun sees first. Where another body stood in
// front of the receiver's own relief, the relief was not in the map, and
// the part of the disc the body leaves and the relief hides came through:
// a lit line along Dimorphos's shadow where it crosses Didymos's own. So
// with the disc, a layer with its body and others in it keeps them apart,
// a slice each (`others_slice`), and the walk takes both.
//
// Replaced percentage-closer soft shadows (one occluder depth for the whole
// disc, the receiver's plane extrapolated to every tap), which lit the umbra
// of Dimorphos's shadow on Didymos at grazing light, leaked light behind
// ridges near the terminator, and so had to keep a body's own shadows hard.
const SUN_AZIMUTHS: u32 = 32u;
// The Sun's limb darkening, linear in mu: 0.56 in the visible.
const LIMB_DARKENING: f32 = 0.56;

const GOLDEN_ANGLE: f32 = 2.39996323;

/// Point `i` of `n` spread evenly over the unit disc, a sunflower's.
fn vogel(i: u32, n: u32) -> vec2<f32> {
    let r = sqrt((f32(i) + 0.5) / f32(n));
    let a = f32(i) * GOLDEN_ANGLE;
    return r * vec2<f32>(cos(a), sin(a));
}

// The shadow of a body with an atmosphere on another: Mars's on its moons.
// Not from the shadow map -- such a body casts into no other body's layer
// (`Window::update`) -- but from its ellipsoid and its air, which is what
// that shadow is: past Mars's limb its dust is opaque some 30 km up, and
// Phobos leaving Mars's shadow came out of it a minute after the solid planet
// let it. For each atmosphere but the receiver's own, each ray toward the
// Sun -- `AIR_RAYS` points of its limb-darkened disc, or its centre when it
// is a point -- passes the body at a height `z` over its ellipsoid: under
// it, the ray is stopped; over it, it crosses the slant optical depth
// `tau exp(-z / H) sqrt(2 pi r / H)`, the column of a grazing ray through an
// exponential atmosphere (Chapman's function at 90 deg). The mean, weighted
// by the limb darkening, is the light that gets through: the umbra, the
// penumbra and the air's own shadow at once, and smooth across a moon a
// fraction of the penumbra's width, where the shadow map, widened to hold
// a penumbra that wide, had halved its texels on the moon and doubled its
// bias -- 5 % too much light on Phobos's crescent.
const AIR_RAYS: u32 = 16u;

fn through_air(p: vec3<f32>, to_sun: vec3<f32>) -> f32 {
    var lit = 1.0;
    let tan_t = view.light.sun_radius / max(length(view.light.pos - p), 1.0e-30);
    for (var i = 0u; i < min(view.light.air_count, 2u); i++) {
        let c = view.light.air[3u * i];
        let axis = view.light.air[3u * i + 1u];
        let dust = view.light.air[3u * i + 2u];
        // Its own air is the shading's (`air`).
        if distance(c.xyz, body.mat[3].xyz) <= 1.0e-4 * c.w {
            continue;
        }
        let to_c = c.xyz - p;
        let ahead = dot(to_c, to_sun);
        // Behind the receiver, or passed far above its air by every ray.
        if ahead <= 0.0 || length(to_c - ahead * to_sun) - c.w > 12.0 * dust.y + 2.0 * ahead * tan_t {
            continue;
        }
        var e1 = cross(to_sun, axis.xyz);
        if dot(e1, e1) < 1.0e-12 {
            e1 = cross(to_sun, vec3<f32>(1.0, 0.0, 0.0));
        }
        e1 = normalize(e1);
        let e2 = cross(to_sun, e1);
        let n = select(1u, AIR_RAYS, tan_t > 0.0);
        var through = 0.0;
        var weight = 0.0;
        for (var k = 0u; k < n; k++) {
            let s = select(vec2<f32>(0.0), vogel(k, n), n > 1u);
            let w = 1.0 - LIMB_DARKENING * (1.0 - sqrt(max(1.0 - dot(s, s), 0.0)));
            let d = normalize(to_sun + tan_t * (s.x * e1 + s.y * e2));
            // From the body's centre to where the ray passes nearest it.
            let q = dot(to_c, d) * d - to_c;
            let r = length(q);
            let sin_lat = dot(q, axis.xyz) / max(r, 1.0e-30);
            let level = c.w * axis.w / sqrt(axis.w * axis.w * max(1.0 - sin_lat * sin_lat, 0.0) + c.w * c.w * sin_lat * sin_lat);
            let z = r - level;
            if z > 0.0 {
                through += w * exp(-dust.x * exp(-z / dust.y) * sqrt(2.0 * PI * r / dust.y));
                weight += w;
            } else if view.light.air_solid != 0u {
                weight += w;
            }
        }
        // With one layer for the scene, the map stops the rays under the
        // limb, and this is the mean over the others.
        if weight > 0.0 {
            lit *= through / weight;
        }
    }
    return lit;
}

/// The limb-darkened disc's light within `rho` of its centre, `rho` in the
/// disc's radii, as a fraction of all of it.
fn disc_within(rho: f32) -> f32 {
    let r2 = min(rho * rho, 1.0);
    let u = LIMB_DARKENING;
    let rim = 1.0 - (1.0 - r2) * sqrt(1.0 - r2);
    return ((1.0 - u) * r2 + u * (2.0 / 3.0) * rim) / (1.0 - u / 3.0);
}

/// The ring boundary nearest `rho`, of the 33 between the disc's 32 rings.
fn ring(rho: f32) -> u32 {
    return u32(round(disc_within(rho) * 32.0));
}

/// Rings `a` up to `b`, as bits.
fn rings(a: u32, b: u32) -> u32 {
    if a >= b {
        return 0u;
    }
    let below_b = select((1u << b) - 1u, 0xffffffffu, b >= 32u);
    return below_b & ~((1u << a) - 1u);
}

/// The rings a band of the disc, `(from, to, ahead)`, hides, `from` and `to`
/// in the disc's radii -- with the last band's between them when both are
/// of one surface.
fn hide(band: vec3<f32>, last: vec3<f32>) -> u32 {
    var hides = band.xy;
    if last.z > 0.0 && max(band.z, last.z) <= 2.0 * min(band.z, last.z) {
        hides = vec2<f32>(min(hides.x, last.x), max(hides.y, last.y));
    }
    if hides.x >= 1.0 {
        return 0u;
    }
    return rings(ring(hides.x), ring(hides.y));
}

/// How far from `q` along `d` the box `size` square from `lo` ends; `q` in it.
fn leave(q: vec2<f32>, d: vec2<f32>, lo: vec2<f32>, size: f32) -> f32 {
    let edge = select(lo, lo + size, d > vec2<f32>(0.0));
    let t = select(vec2<f32>(1.0e9), (edge - q) / d, abs(d) > vec2<f32>(1.0e-6));
    return max(min(t.x, t.y), 0.0) + 1.0e-3;
}

// The narrowest penumbra, in texels, the walk is trusted with. Narrower, it
// is drawn as with a point. Each texel then stands for a quarter of the
// disc's radius or more, and which texel a junction of two occluders falls
// in decides the answer: on Dimorphos from 37 m, its map's texels 3 cm and
// its boulders' penumbrae 1-2 texels, the walk lit gaps between shadows that
// meet, 14 % too bright where rays to the disc give 0-5 %, no closer to the
// rays than the hard lookup (rms 0.17-0.18 against 0.14-0.18); near its
// terminator, penumbrae 4-4.4 texels wide, still lit slivers in the dark
// (0.10 against 0.06). See `notes/2026-10-09_disc_shadows_apart/` and
// `notes/2026-10-09_near_shadow_layer/`.
const WALK_MIN_REACH: f32 = 6.0;

/// How far, in texels, a penumbra can reach a receiver at `uv`, `depth` in
/// `layer` from, and what is there: `(reach, 0)` nothing within it, or none
/// `WALK_MIN_REACH` wide -- the hard lookup's to answer -- `(reach, 1)` the
/// umbra, `(reach, 2)` a penumbra to walk. `tan_uv` is the Sun's angular
/// radius, uv per unit of depth.
fn sun_reach(layer: u32, uv: vec2<f32>, depth: f32, bias: f32, tan_uv: f32) -> vec2<f32> {
    return sun_reach_about(layer, uv, depth, bias, tan_uv, 0.0);
}

/// `sun_reach` for any receiver within `spread` texels of `uv`, none
/// farther from the Sun than `depth`: what it says of nothing in reach holds
/// for each of them (`cs_facets`, a facet's corners at once).
fn sun_reach_about(layer: u32, uv: vec2<f32>, depth: f32, bias: f32, tan_uv: f32, spread: f32) -> vec2<f32> {
    let res = f32(globals.shadow_resolution);
    let top = i32(textureNumLevels(t_shadow_min)) - 1;
    // How far, in texels, a penumbra can reach the receiver from: the depth
    // nearest the Sun in the whole layer; then, twice, from the pyramid's
    // blocks about the receiver at least that wide, three by three, the
    // farthest any of them reaches -- a block's from its nearest depth, and
    // only if the receiver is within it; and last, a penumbra still in
    // reach, from blocks half as wide, five by five, which hold no more than
    // the reach does. Nothing within reach in front of the receiver: no
    // penumbra. Everything within reach in front, one surface: the umbra.
    // Only between does the receiver walk the map, and with three by three
    // alone it walked at twice the width the reach needs, about Dimorphos's
    // shadow on Didymos half of them to find the umbra.
    let front = textureLoad(t_shadow_min, vec2<i32>(0), i32(layer), top).r;
    var reach = (depth - front) * tan_uv * res;
    var umbra = false;
    let p = uv * res;
    for (var tries = 0; tries < 3; tries++) {
        if reach < WALK_MIN_REACH {
            return vec2<f32>(reach, 0.0);
        }
        if tries == 2 && umbra {
            break;
        }
        let span = select(1, 2, tries == 2);
        let n = 2 * span + 1;
        let m = clamp(i32(ceil(log2(reach / (4.0 * f32(span))))), 0, top);
        let block = f32(4u << u32(m));
        let blocks = vec2<i32>(textureDimensions(t_shadow_min, m));
        let centre = vec2<i32>(floor(p / block));
        var farthest = 0.0;
        var all_in_front = true;
        var near = 1.0;
        var far = 0.0;
        for (var k = 0; k < n * n; k++) {
            let b = centre + vec2<i32>(k % n - span, k / n - span);
            let lo = vec2<f32>(b) * block;
            let apart = max(length(max(max(lo - p, p - lo - block), vec2<f32>(0.0))) - spread, 0.0);
            if apart >= reach {
                continue;
            }
            if any(b < vec2<i32>(0)) || any(b >= blocks) {
                all_in_front = false;
                continue;
            }
            let z = textureLoad(t_shadow_min, b, i32(layer), m).rg;
            let ahead = (depth - z.x) * tan_uv * res;
            if apart < ahead {
                farthest = max(farthest, ahead);
            }
            all_in_front = all_in_front && z.y < depth - bias;
            near = min(near, z.x);
            far = max(far, z.y);
        }
        reach = min(reach, farthest);
        umbra = all_in_front && depth - near <= 2.0 * (depth - far);
    }
    if reach < WALK_MIN_REACH {
        return vec2<f32>(reach, 0.0);
    }
    return vec2<f32>(reach, select(2.0, 1.0, umbra));

}

/// The rings of the disc a receiver at `uv`, `depth` in `layer` does not
/// see along directions `first`, `first + every`, ... of `SUN_AZIMUTHS`:
/// its layer's texels out to `reach.x` texels, and where its body and the
/// others are apart, the others' slice out to `reach.y`; 0, a slice not
/// walked. `grad` is the receiver's plane, depth per uv.
fn sun_hidden(layer: u32, uv: vec2<f32>, depth: f32, grad: vec2<f32>, bias: f32, tan_uv: f32,
              reach: vec2<f32>, first: u32, every: u32) -> u32 {
    let others = others_slice(layer);
    var hidden = 0u;
    for (var j = first; j < SUN_AZIMUTHS; j += every) {
        let a = (f32(j) + 0.5) * (2.0 * PI / f32(SUN_AZIMUTHS));
        let along = vec2<f32>(cos(a), sin(a));
        // The receiver's own plane hides what is below its horizon: past
        // a tilt of one over its rise toward the Sun this way.
        var mask = 0u;
        let rise = -dot(grad, along) * tan_uv;
        if rise > 1.0 {
            mask = rings(ring(1.0 / rise), 32u);
        }
        // A ring hidden in any slice is hidden: the body's relief and
        // another body in front of it, each whole in its own slice; and in
        // the layer's second depth layer, relief the first holds no trace of
        // -- a rock in the shadow of a bigger one, as the Sun sees them.
        if reach.x > 0.0 {
            mask = sun_walk(layer, uv, depth, grad, bias, tan_uv, reach.x, along, mask);
            if view.light.peeled != 0u {
                mask = sun_walk(peel_slice(layer), uv, depth, grad, bias, tan_uv, reach.x, along, mask);
            }
        }
        if reach.y > 0.0 {
            mask = sun_walk(others, uv, depth, grad, bias, tan_uv, reach.y, along, mask);
        }
        hidden += countOneBits(mask);
    }
    return hidden;
}

/// The rings hidden along `along`, `so_far` and those the texels of `slice`
/// out to `reach` texels hide.
fn sun_walk(slice: u32, uv: vec2<f32>, depth: f32, grad: vec2<f32>, bias: f32, tan_uv: f32,
            reach: f32, along: vec2<f32>, so_far: u32) -> u32 {
    let res = f32(globals.shadow_resolution);
    let top = i32(textureNumLevels(t_shadow_min)) - 1;
    let p = uv * res;
    // The walk, in texels, through the pyramid: a block with nothing in front
    // of the receiver passed over; one all in front, of one surface, hiding
    // at once the band from where the walk enters it to where it leaves;
    // one with an edge in it looked into, a level down, and at the bottom,
    // texel by texel. The blocks grow with the way out, to a quarter of it:
    // a texel's band of the disc is its width over its distance, so the far
    // map is wanted no finer than that, while next to the receiver every
    // texel counts -- a step of the reach over 24, as this first was, passed
    // over Phobos's own relief when Mars, thousands of km in front, made the
    // reach hundreds of texels, and lit Phobos up as it left Mars's shadow.
    let size = vec2<i32>(textureDimensions(t_shadow));
    var mask = so_far;
    // The last band hidden and how far in front it was, 0 if nothing
    // was: a surface runs on between two samples of it, and a ray
    // between their bands would pass through it. Two are of one surface
    // when one is no more than twice as far in front as the other -- a
    // silhouette, Dimorphos's edge over Didymos, is far more.
    var last = vec3<f32>(0.0);
    // The receiver's own texel first, from the receiver to where the walk
    // leaves it, in front or not as the hard lookup has it. The steps
    // below start half a texel out, and from near the texel's edge their
    // first falls in the next: a rock 7 cm in front, its penumbra under a
    // tenth of a texel, was in the receiver's texel and no other, and the
    // whole disc came through it that way -- lit streaks in the dark on
    // Dimorphos at a grazing Sun.
    let own = vec2<f32>(floor(p));
    let z_own = textureLoad(t_shadow, clamp(vec2<i32>(own), vec2<i32>(0), size - 1), i32(slice), 0);
    if z_own < depth - bias {
        let ahead = (depth - z_own) * tan_uv * res;
        let band = vec3<f32>(0.0, leave(p, along, own, 1.0) / ahead, ahead);
        mask |= hide(band, last);
        last = band;
    }
    var t = 0.0;
    var m = 0;
    for (var guard = 0; guard < 128; guard++) {
        if t >= reach || mask == 0xffffffffu {
            break;
        }
        let q = p + t * along;
        if any(q < vec2<f32>(0.0)) || any(q >= vec2<f32>(res)) {
            break;
        }
        m = min(m + 1, clamp(i32(floor(log2(max(t * 0.25, 4.0) / 4.0))), 0, top));
        let block = f32(4u << u32(m));
        let b = vec2<i32>(floor(q / block));
        let lo = vec2<f32>(b) * block;
        let exit = t + leave(q, along, lo, block);
        let z = textureLoad(t_shadow_min, b, i32(slice), m).rg;
        // The receiver's plane at the block's corners nearest and
        // farthest from the Sun: a texel hides something only if in
        // front of the plane where it is, so a block hides nothing if
        // even its nearest depth is behind the plane's farthest corner,
        // and hides its whole band only if even its farthest depth is in
        // front of the plane's nearest.
        let up = grad < vec2<f32>(0.0);
        let near_corner = (lo + select(vec2<f32>(0.0), vec2<f32>(block), up)) / res;
        let far_corner = (lo + select(vec2<f32>(block), vec2<f32>(0.0), up)) / res;
        let plane_near = depth + dot(near_corner - uv, grad);
        let plane_far = depth + dot(far_corner - uv, grad);
        let most = (depth - z.x) * tan_uv * res;
        if z.x >= min(depth, plane_far) - bias || t >= most + 0.5 {
            last = vec3<f32>(0.0);
            t = exit;
            continue;
        }
        let least = (depth - z.y) * tan_uv * res;
        if z.y < min(depth, plane_near) - bias && most <= 2.0 * least {
            let band = vec3<f32>(t / most, exit / least, most);
            mask |= hide(band, last);
            last = band;
            t = exit;
            continue;
        }
        if m > 0 {
            m -= 2;
            continue;
        }
        for (var u = t + 0.5; u < exit; u += 1.0) {
            let texel = clamp(vec2<i32>(p + u * along), vec2<i32>(0), size - 1);
            let zt = textureLoad(t_shadow, texel, i32(slice), 0);
            // In front of the receiver, and above its own plane there:
            // the receiver's own surface hides nothing above its horizon.
            let at = depth + dot((vec2<f32>(texel) + 0.5) / res - uv, grad);
            var band = vec3<f32>(0.0);
            if zt < min(depth, at) - bias {
                let ahead = (depth - zt) * tan_uv * res;
                band = vec3<f32>(max(u - 0.5, 0.0) / ahead, (u + 0.5) / ahead, ahead);
                mask |= hide(band, last);
            }
            last = band;
        }
        t = exit;
    }
    return mask;
}

/// What a receiver at `uv`, `depth` in `layer` sees of the Sun's disc: `x`
/// the fraction its walked slices leave, limb-darkened, -1 where no
/// penumbra reaching it from either is `WALK_MIN_REACH` texels wide, for the
/// hard lookups to answer; `y` and `z` 1 where its layer and the other bodies'
/// slice were walked -- a slice not walked answers by its hard lookup.
/// `grad` is the receiver's plane, depth per uv; `tan_uv` the Sun's angular
/// radius, uv per unit of depth.
fn sun_seen(layer: u32, uv: vec2<f32>, depth: f32, grad: vec2<f32>, bias: f32, tan_uv: f32) -> vec3<f32> {
    let reach = sun_reaches(layer, uv, depth, bias, tan_uv);
    if reach.x < 0.0 {
        return vec3<f32>(0.0, 1.0, 1.0);
    }
    if all(reach == vec2<f32>(0.0)) {
        return vec3<f32>(-1.0, 0.0, 0.0);
    }
    let hidden = sun_hidden(layer, uv, depth, grad, bias, tan_uv, reach, 0u, 1u);
    return vec3<f32>(1.0 - f32(hidden) / f32(32u * SUN_AZIMUTHS), select(vec2<f32>(0.0), vec2<f32>(1.0), reach > vec2<f32>(0.0)));
}

/// How far to walk each slice of a receiver in `layer` (`sun_reach`): its
/// layer's and the other bodies', 0 for one with no penumbra in reach --
/// none apart, the other's -- and `x` -1 for the umbra of either.
fn sun_reaches(layer: u32, uv: vec2<f32>, depth: f32, bias: f32, tan_uv: f32) -> vec2<f32> {
    let own = sun_reach(layer, uv, depth, bias, tan_uv);
    var other = vec2<f32>(0.0);
    let others = others_slice(layer);
    if others != layer {
        other = sun_reach(others, uv, depth, bias, tan_uv);
    }
    if own.y == 1.0 || other.y == 1.0 {
        return vec2<f32>(-1.0, 0.0);
    }
    return vec2<f32>(select(0.0, own.x, own.y == 2.0), select(0.0, other.x, other.y == 2.0));
}

// The disc walked in passes of its own (`pass::penumbra`). A prepass draws
// the bodies once more, single-sampled, writing each pixel's surface --
// its normal and shadow layer -- and its depth (`fs_penumbra`); a compute
// pass goes over the pixels, does what decides whether a receiver needs a
// walk and queues those that do (`cs_scan`); another walks the queue a lane
// per direction (`cs_walk`), the 32 directions of one receiver side by side
// on one SIMD group; and the main pass reads its pixel's answer
// (`sun_walked`). Walked in the main pass's fragments, a receiver took one
// lane of 32 and waited on each of its reads in turn, the lanes beside it
// idle -- 25 ms of a 31 ms frame on Didymos from 1 km with Dimorphos's
// shadow across it. The prepass writes nothing else: a fragment stage
// writing to memory is run for every fragment, hidden ones too, and one
// that queued its own walk queued 270,000 on that frame for the 50,000
// pixels that needed one.
struct Walk {
    uv: vec2<f32>,
    grad: vec2<f32>,
    // How far to walk its layer and the other bodies' slice (`sun_reaches`).
    reach: vec2<f32>,
    depth: f32,
    bias: f32,
    tan_uv: f32,
    layer: u32,
    // Rings hidden, summed over the directions.
    hidden: atomic<u32>,
};

struct Walks {
    count: atomic<u32>,
    entries: array<Walk>,
};

// The same, read by the main pass once the walks are done.
struct Walked {
    uv: vec2<f32>,
    grad: vec2<f32>,
    reach: vec2<f32>,
    depth: f32,
    bias: f32,
    tan_uv: f32,
    layer: u32,
    hidden: u32,
};

struct WalksDone {
    count: u32,
    entries: array<Walked>,
};

// The camera's inverse, for the scan to find a pixel's surface from its
// depth, the camera itself, to find a point's pixel, and the image's size.
struct Scan {
    inverse: mat4x4<f32>,
    view_proj: mat4x4<f32>,
    size: vec2<u32>,
};

// The scan and the walks.
@group(6) @binding(0) var<storage, read_write> walks: Walks;
@group(6) @binding(3) var t_surface: texture_2d<u32>;
@group(6) @binding(4) var t_surface_depth: texture_depth_2d;
@group(6) @binding(5) var t_codes: texture_storage_2d<rg32uint, write>;
@group(6) @binding(6) var<uniform> scan: Scan;
// The main pass: the walks, and the scan's code and depth per pixel --
// 0 nothing drawn, 1 no penumbra, 2 the umbra, 3 a walk with no room in the
// queue, 4 + i walk `i`.
@group(6) @binding(1) var<storage, read> walked: WalksDone;
@group(6) @binding(2) var t_walk: texture_2d<u32>;

/// Where a receiver in shadow layer `layer` looks the Sun up: its place
/// there and depth, lifted off the surface as the hard lookup is, its bias,
/// its plane -- depth per uv, unclamped -- and the Sun's angular radius in
/// uv per unit of depth. `ndotl` is the Sun's cosine on it, above 0.
struct SunAt {
    uv: vec2<f32>,
    depth: f32,
    bias: f32,
    tan_uv: f32,
};

/// The PCF kernel's steps for a receiver (`sun_lookup`): a texel's width
/// along its own plane, across the light and up the plane's slope toward
/// it, as a change of (uv, depth); and how much nearer the Sun than the
/// plane a tap may find the surface, per texel out, and still take it for
/// the receiver's own ground. Apart from `SunAt`, which the scan and the
/// per-facet query take too, without a kernel.
struct SunKernel {
    across: vec3<f32>,
    up: vec3<f32>,
    relief: f32,
};

fn sun_at(pos: vec3<f32>, normal: vec3<f32>, ndotl: f32, layer: u32) -> SunAt {
    var at: SunAt;
    let lb = view.light.layer_bias[layer];
    let k = 1.0 - ndotl;
    // One texel diagonal, whatever the kernel. This used to scale with the
    // PCF radius to keep far taps from self-shadowing a tilted receiver, but
    // lifting the lookup N texels off the surface moves the shadow's edge --
    // at grazing incidence by far more than N texels along the surface --
    // and Dimorphos's shadow on Didymos shrank from 78,042 to 8,539 px
    // between pcf 0 and 16 at 512. The far taps are the receiver-plane
    // term's job below; the offset only has to clear the texel it is in.
    let offset_pos = pos + normal * (lb.x * k);
    let m = view.light.view_proj_layers[layer];
    let p = project_light(m, offset_pos);
    at.uv = p.xy;
    at.depth = p.z;
    at.bias = max(lb.y * k * k, lb.z);
    // `lb.w` is one texel's width in depth units.
    let tan_theta = view.light.sun_radius / max(length(view.light.pos - pos), 1.0e-30);
    at.tan_uv = tan_theta / max(lb.w * f32(globals.shadow_resolution), 1.0e-30);
    return at;
}

/// The receiver's plane at `pos`, facing `normal`, as `sun_at` moves it:
/// depth per uv, for the walk (`sun_hidden`). Worked out where a walk is,
/// not for every lookup: three projections a lookup that only the few
/// receivers in a penumbra use.
fn sun_grad(pos: vec3<f32>, normal: vec3<f32>, ndotl: f32, layer: u32) -> vec2<f32> {
    let lb = view.light.layer_bias[layer];
    let offset_pos = pos + normal * (lb.x * (1.0 - ndotl));
    return receiver_plane_grad(view.light.view_proj_layers[layer], offset_pos, normal);
}

/// `SunKernel` for a receiver at `pos` facing `normal` in `layer`.
fn sun_kernel(pos: vec3<f32>, normal: vec3<f32>, ndotl: f32, layer: u32) -> SunKernel {
    var kernel: SunKernel;
    let lb = view.light.layer_bias[layer];
    let m = view.light.view_proj_layers[layer];
    let texel = 2.0 / (length(vec3<f32>(m[0][0], m[1][0], m[2][0])) * f32(globals.shadow_resolution));
    var across = cross(normal, normalize(view.light.pos - pos));
    if dot(across, across) < 1.0e-12 {
        across = cross(normal, select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), abs(normal.x) > 0.9));
    }
    across = normalize(across);
    kernel.across = light_step(m, across * texel);
    kernel.up = light_step(m, cross(across, normal) * texel);
    kernel.relief = PCF_RELIEF * lb.w / max(ndotl, PCF_NDOTL_MIN);
    return kernel;
}

/// The change of (uv, depth) a displacement `v` makes in a layer's view,
/// which is orthographic: without subtracting two projected positions,
/// whose digits a body far from the origin takes.
fn light_step(m: mat4x4<f32>, v: vec3<f32>) -> vec3<f32> {
    let d = m * vec4<f32>(v, 0.0);
    return vec3<f32>(d.x * 0.5, -d.y * 0.5, d.z);
}

/// A body's shadow layer, the last when there are more bodies than layers.
fn body_layer() -> u32 {
    return min(body.shadow_layer, max(view.light.n_layers, 1u) - 1u);
}

/// The layer a point `pos` of this body looks the Sun up in: its near layer,
/// finer, where it has one and the point is well inside it -- not in the
/// margin round it, nor farther from the Sun than what it was fitted to --
/// else its own. The penumbra prepass and the main pass both choose by this,
/// so a walk is in the layer its pixel reads.
fn shadow_layer_at(pos: vec3<f32>) -> u32 {
    // Cascades: the finest that holds the point well inside, else the
    // scene's, after them.
    if view.light.cascades > 0u {
        for (var k = 0u; k < view.light.cascades; k++) {
            let p = project_light(view.light.view_proj_layers[k], pos);
            if all(abs(p.xy - vec2<f32>(0.5)) <= vec2<f32>(0.5 * CASCADE_INNER)) && p.z >= 0.0 && p.z <= 1.0 {
                return k;
            }
        }
        return view.light.cascades;
    }
    let own = body_layer();
    if body.near_layer >= view.light.n_layers {
        return own;
    }
    let p = project_light(view.light.view_proj_layers[body.near_layer], pos);
    if all(abs(p.xy - vec2<f32>(0.5)) <= vec2<f32>(0.5 * body.near_inner)) && p.z <= body.near_depth {
        return body.near_layer;
    }
    return own;
}

/// How far into a cascade, as a fraction of its width about its centre, a
/// point uses it: its rim is left to the next, whose PCF taps and walks
/// would read past the edge.
const CASCADE_INNER: f32 = 0.95;

/// The slice the other bodies of `layer` are in: their own, after the
/// layers, where its body and they are apart (`light.apart`), or `layer`.
fn others_slice(layer: u32) -> u32 {
    let bit = 1u << layer;
    if (view.light.apart & bit) == 0u {
        return layer;
    }
    return view.light.n_layers + countOneBits(view.light.apart & (bit - 1u));
}

/// Layer `layer`'s second depth layer, where the layers have them: the
/// nearest surface behind what the Sun sees first, after the others' slices.
fn peel_slice(layer: u32) -> u32 {
    return view.light.n_layers + countOneBits(view.light.apart) + layer;
}

/// The hard lookup in `slice`: whether `uv` at `depth`, less `bias`, is
/// lit, filtered over a texel, or with `shadow_pcf` over its kernel, each
/// tap against the receiver's plane `grad` there.
/// The hard lookup: lit or not, as a point Sun has it, filtered over a
/// kernel `2 pcf + 1` texels wide where `shadows.pcf` asks for one.
///
/// The taps lie on the receiver's own plane, a texel apart along it, across
/// the light and up the plane's slope (`SunAt::across`, `up`). They used to
/// be a square of texels in the layer's view, each compared against the
/// receiver's plane extended to it. At a low Sun a texel across the layer's
/// view is many along the ground -- sixteen were over a metre on Dimorphos
/// at 16384 -- and the ground there rose above the plane: lit ground came out
/// darkened as if by ambient occlusion, 45 % of the lit pixels from 68 m by
/// more than 5 % at pcf 16, 8 % at pcf 4; a slope ceiling on the plane
/// (`tan 85 deg`) had kept the worst of it, a terminator's plane rising a
/// kilometre, from going lit instead. On the ground a kernel reaches as far
/// whatever the Sun's height, and blurs a shadow's edge as wide there.
///
/// A tap still takes the surface it finds for the receiver's own ground if
/// that is no nearer the Sun than ground rising `PCF_RELIEF` per unit out
/// from the receiver would be: facets meet at a few degrees, and with the
/// plane alone even the taps on the ground took its every hollow for an
/// occluder (1 % of the lit pixels at pcf 16, 7 % darkened by over 5 %).
fn sun_lookup(slice: u32, at: SunAt, kernel: SunKernel) -> f32 {
    if globals.shadow_pcf == 0u {
        return textureSampleCompareLevel(t_shadow, s_shadow, at.uv, slice, at.depth - at.bias);
    }
    // The corners and the middle first: where those agree, so would the
    // rest, and most of an image is wholly lit or wholly in shadow.
    let n = i32(globals.shadow_pcf);
    let e = f32(n);
    let first = sun_tap(slice, at, kernel, vec2<f32>(-e, -e)) + sun_tap(slice, at, kernel, vec2<f32>(e, -e))
        + sun_tap(slice, at, kernel, vec2<f32>(-e, e)) + sun_tap(slice, at, kernel, vec2<f32>(e, e))
        + sun_tap(slice, at, kernel, vec2<f32>(0.0));
    if first == 0.0 || first == 5.0 {
        return first / 5.0;
    }
    // A texel apart, each tap the hardware's 2x2 comparison. Two apart, a
    // third of the taps, the taps off the texels' grid left ripples across
    // a shadow's edge, streaks along the light. Accumulated into a sum of
    // its own: onto the `shadow` of 1.0 it biased every result brighter by
    // 1/(2*pcf+1)^2 -- ~+11% at pcf=1.
    var sum = 0.0;
    for (var x = -n; x <= n; x++) {
        for (var y = -n; y <= n; y++) {
            sum += sun_tap(slice, at, kernel, vec2<f32>(f32(x), f32(y)));
        }
    }
    let taps = f32(2 * n + 1);
    return sum / (taps * taps);
}

/// One tap of `sun_lookup`'s kernel, `k` texels out along the receiver's
/// plane, across the light and up its slope.
fn sun_tap(slice: u32, at: SunAt, kernel: SunKernel, k: vec2<f32>) -> f32 {
    let o = kernel.across * k.x + kernel.up * k.y;
    return textureSampleCompareLevel(t_shadow, s_shadow, at.uv + o.xy, slice, at.depth + o.z - at.bias - kernel.relief * length(k));
}

/// The rise per unit out, as a slope, that a tap's ground may have over the
/// receiver's plane (`sun_lookup`): 0.25, 14 deg. At 0.1, ground rising at
/// 4096 still darkened 5 % of the lit pixels at pcf 16; at 0.25 1.4 %, with
/// 1 % of the shadowed lit through it, each beyond the blur's own reach.
const PCF_RELIEF: f32 = 0.25;
/// The receiver's height over the Sun below which `PCF_RELIEF` stops
/// growing, as the cosine of its incidence.
const PCF_NDOTL_MIN: f32 = 0.05;

@fragment
fn fs_penumbra(vertex: VertexOutput, @builtin(primitive_index) prim: u32) -> @location(0) vec4<u32> { //@prim
fn fs_penumbra(vertex: VertexOutput) -> @location(0) vec4<u32> { //@noprim
    let prim = vertex.facet; //@noprim
    let in = surface(vertex, prim);
    return vec4<u32>(bitcast<vec3<u32>>(in.world_normal), shadow_layer_at(in.world_pos));
}

var<workgroup> scan_walks: atomic<u32>;
var<workgroup> scan_first: u32;

/// Each pixel's code (`t_walk`): its surface from the prepass, the place
/// from its depth through the camera's inverse; what is in reach of it from
/// the pyramid (`sun_reach`); and a walk queued where there is a penumbra
/// -- a place in the queue per workgroup, so that 50,000 walks do not wait
/// on one counter in turn.
@compute @workgroup_size(8, 8)
fn cs_scan(@builtin(global_invocation_id) id: vec3<u32>, @builtin(local_invocation_index) local: u32) {
    let inside = all(id.xy < scan.size);
    let pixel = vec2<i32>(min(id.xy, scan.size - 1u));
    let z = textureLoad(t_surface_depth, pixel, 0);
    var code = 0u;
    var at: SunAt;
    var grad = vec2<f32>(0.0);
    var layer = 0u;
    var reach = vec2<f32>(0.0);
    if inside && z != DEPTH_CLEAR {
        let surface = textureLoad(t_surface, pixel, 0);
        let normal = bitcast<vec3<f32>>(surface.xyz);
        layer = surface.w;
        let ndc = vec2<f32>(
            (f32(pixel.x) + 0.5) / f32(scan.size.x) * 2.0 - 1.0,
            1.0 - (f32(pixel.y) + 0.5) / f32(scan.size.y) * 2.0,
        );
        let h = scan.inverse * vec4<f32>(ndc, z, 1.0);
        let pos = h.xyz / h.w;
        let ndotl = dot(normal, normalize(view.light.pos - pos));
        code = 1u;
        if ndotl > 0.0 {
            at = sun_at(pos, normal, ndotl, layer);
            reach = sun_reaches(layer, at.uv, at.depth, at.bias, at.tan_uv);
            code = select(select(1u, 3u, any(reach > vec2<f32>(0.0))), 2u, reach.x < 0.0);
            if code == 3u && near_blocked(pos, normal, layer) {
                code = 2u;
            }
            if code == 3u {
                grad = sun_grad(pos, normal, ndotl, layer);
            }
        }
    }
    var mine = 0u;
    if code == 3u {
        mine = atomicAdd(&scan_walks, 1u);
    }
    workgroupBarrier();
    if local == 0u {
        scan_first = atomicAdd(&walks.count, atomicLoad(&scan_walks));
    }
    workgroupBarrier();
    if code == 3u {
        let i = scan_first + mine;
        if i < arrayLength(&walks.entries) {
            walks.entries[i].uv = at.uv;
            walks.entries[i].grad = grad;
            walks.entries[i].depth = at.depth;
            walks.entries[i].bias = at.bias;
            walks.entries[i].tan_uv = at.tan_uv;
            walks.entries[i].reach = reach;
            walks.entries[i].layer = layer;
            atomicStore(&walks.entries[i].hidden, 0u);
            code = 4u + i;
        }
    }
    if inside {
        textureStore(t_codes, pixel, vec4<u32>(code, bitcast<u32>(z), 0u, 0u));
    }
}

/// Whether relief the camera sees stops the ray from the surface at `pos`
/// toward the Sun's centre close to it -- nearer than its penumbra could be
/// two texels of `layer` wide, so that the receiver is in its umbra, as a
/// point Sun would have it. A map holds what the Sun sees first: bumps a
/// centimetre or two above the ray half a metre to two metres from
/// receivers at a grazing Sun on Dimorphos, each behind two far surfaces
/// along the ray, were in neither depth layer, and the disc came through
/// them in streaks; the camera saw them.
///
/// The ray is stepped out, closer together near the receiver, each point
/// found in the image. Where a point goes behind the surface the camera
/// sees, the stretch since the last step is gone over again a pixel at a
/// time to the first point behind, and the ray stopped if that surface is
/// not in front of the pixel's stretch of ray by more than `NEAR_THICK`
/// pixels: the ray went into it. A surface well in front is something the
/// ray passed behind, a rock between the camera and the ground. Steps far
/// apart in the image went from in front of a slope to inside it by six to
/// sixteen pixels, and missed it.
fn near_blocked(pos: vec3<f32>, normal: vec3<f32>, layer: u32) -> bool {
    let to_sun = normalize(view.light.pos - pos);
    let lm = view.light.view_proj_layers[layer];
    let texel = 2.0 / (length(vec3<f32>(lm[0][0], lm[1][0], lm[2][0])) * f32(globals.shadow_resolution));
    let tan_theta = view.light.sun_radius / max(length(view.light.pos - pos), 1.0e-30);
    let reach = 2.0 * texel / max(tan_theta, 1.0e-30);
    let start = pos + normal * (0.5 * texel);
    // The ray in clip space, a straight line there too.
    let c0 = scan.view_proj * vec4<f32>(start, 1.0);
    let cd = scan.view_proj * vec4<f32>(to_sun, 0.0);
    var t0 = 0.0;
    var in_front = true;
    for (var k = 1u; k <= NEAR_STEPS; k++) {
        let f = f32(k) / f32(NEAR_STEPS);
        let t1 = reach * f * f;
        let c = c0 + cd * t1;
        let seen = near_seen(c);
        if seen.x < 0.0 {
            return false;
        }
        let behind = seen.y > 0.0;
        // Gone behind the surface the camera sees since the last step, or
        // passed within `NEAR_CLOSE` pixels in front of it: a ray that only
        // grazes the ground dips under it between two steps 7 pixels apart
        // and is in front at both, 0.1 pixel at one (Dimorphos, 2.3 m out).
        var close = false;
        if !behind && in_front {
            let ndc = c.xyz / c.w;
            let z = textureLoad(t_surface_depth, vec2<i32>(seen.zw), 0);
            let h = scan.inverse * vec4<f32>(ndc.xy, z, 1.0);
            let p = start + to_sun * t1;
            let gap = length(h.xyz / h.w - globals.camera_pos) - length(p - globals.camera_pos);
            close = gap < NEAR_CLOSE * near_footprint(ndc, p);
        }
        if in_front && (behind || close) && near_refine(c0, cd, start, to_sun, t0, t1) {
            return true;
        }
        in_front = !behind;
        t0 = t1;
    }
    return false;
}

/// Whether the ray from `start` along `to_sun`, `c0 + cd t` in clip space,
/// goes into the surface the camera sees between `t0` and `t1`: gone over a
/// pixel at a time to the first point behind that surface, and stopped if
/// the surface is not in front of the pixel's stretch of ray by more than
/// `NEAR_THICK` pixels.
fn near_refine(c0: vec4<f32>, cd: vec4<f32>, start: vec3<f32>, to_sun: vec3<f32>, t0: f32, t1: f32) -> bool {
    let eye = globals.camera_pos;
    let span = u32(ceil(distance(near_pixel(c0 + cd * t0), near_pixel(c0 + cd * t1))));
    let n = clamp(span, 1u, NEAR_REFINE);
    var t = t0;
    for (var j = 1u; j <= n; j++) {
        let tj = mix(t0, t1, f32(j) / f32(n));
        let c = c0 + cd * tj;
        let s = near_seen(c);
        if s.y > 0.0 {
            let ndc = c.xyz / c.w;
            let z = textureLoad(t_surface_depth, vec2<i32>(s.zw), 0);
            let h = scan.inverse * vec4<f32>(ndc.xy, z, 1.0);
            let p = start + to_sun * tj;
            let nearest = min(length(start + to_sun * t - eye), length(p - eye));
            return length(h.xyz / h.w - eye) > nearest - NEAR_THICK * near_footprint(ndc, p);
        }
        t = tj;
    }
    return false;
}

/// A pixel's width at the point `p`, `ndc` its place in the image.
fn near_footprint(ndc: vec3<f32>, p: vec3<f32>) -> f32 {
    let side = scan.inverse * vec4<f32>(ndc.xy + vec2<f32>(2.0 / f32(scan.size.x), 0.0), ndc.z, 1.0);
    return length(side.xyz / side.w - p);
}

/// The pixel of a point given in clip space.
fn near_pixel(c: vec4<f32>) -> vec2<f32> {
    let ndc = c.xy / c.w;
    return vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5) * vec2<f32>(scan.size);
}

/// For a point in clip space: -1 in x if it is off the image or behind the
/// camera; in y, positive if the surface the camera sees at its pixel is in
/// front of it; its pixel in zw.
fn near_seen(c: vec4<f32>) -> vec4<f32> {
    if c.w <= 0.0 {
        return vec4<f32>(-1.0);
    }
    let ndc = c.xyz / c.w;
    if any(abs(ndc.xy) >= vec2<f32>(1.0)) {
        return vec4<f32>(-1.0);
    }
    let pixel = near_pixel(c);
    let z = textureLoad(t_surface_depth, vec2<i32>(pixel), 0);
    // Reversed depth: nearer is larger.
    return vec4<f32>(1.0, z - ndc.z, pixel);
}

/// Steps of `near_blocked`'s ray, and the most pixels a stretch between two
/// is gone over again in.
const NEAR_STEPS: u32 = 32u;
const NEAR_REFINE: u32 = 24u;
/// How far in front of a pixel's stretch of ray, in the image's pixels, the
/// surface seen there can be and still be where the ray went in: the depth
/// buffer holds a pixel's middle, the point is anywhere in it.
const NEAR_THICK: f32 = 2.0;
/// How near in front of the surface the camera sees, in its pixels, a step
/// of the ray has the stretch since the last gone over again too.
const NEAR_CLOSE: f32 = 1.0;

// The per-facet shadow query (`sim.facet_shadow`, the thermophysical
// model's shadows): what each facet's corners and centre see of the Sun, as
// the image has it -- the same layer, slices, lookups and kernel
// (`shadows.pcf`), and with the Sun a disc the same pyramid and walk -- and
// with a horizon map, over its horizon. Group 5 is the body's own (`Body`,
// its horizons), 6 the query's: the walks queued as the image's are
// (`walks`, `cs_walk`), what each is for (`facet_walks`).
//
// Walked where each point was found to need it, a lane for the 32 of its
// directions in turn, the query took 19 ms a step of the Didymos pair's
// 33 with Dimorphos's shadow across Didymos: a SIMD group held by its
// longest walk, as the image's fragments were. So the points queue their
// walks (`cs_facets`), the image's walk takes them a lane per direction
// (`cs_walk`), and each walked point's share is added to its facet after
// (`cs_facets_walked`), in fixed point.
struct FacetQuery {
    n_facets: u32,
    // Invocations per row of a dispatch split in two (`dispatch_2d`).
    stride: u32,
    // 1 when facet `i` is vertices `3i..3i+2`, 0 when `facet_indices` say.
    is_flat: u32,
    _pad: u32,
};
@group(6) @binding(10) var<uniform> facet_query: FacetQuery;
@group(6) @binding(11) var<storage, read> facet_geometry: array<f32>;
@group(6) @binding(12) var<storage, read> facet_indices: array<u32>;
// What each facet's points see of the disc, summed, `FACET_FIXED` a whole
// disc's worth per point.
@group(6) @binding(13) var<storage, read_write> facet_out: array<atomic<u32>>;
// For each queued walk: its facet, its hard lookups (its layer's, the other
// bodies') and its facet's horizon, as bits.
@group(6) @binding(14) var<storage, read_write> facet_walks: array<vec4<u32>>;

const FACET_FIXED: f32 = 4194304.0;

fn facet_vertex(i: u32) -> vec3<f32> {
    let p = vec3<f32>(facet_geometry[3u * i], facet_geometry[3u * i + 1u], facet_geometry[3u * i + 2u]);
    let m = mat4x4<f32>(body.mat[0], body.mat[1], body.mat[2], body.mat[3]);
    return (m * vec4<f32>(p, 1.0)).xyz;
}

/// What a point `pos` of facet `f`, facing `n`, sees of the Sun in `layer`,
/// over its facet's `horizon`, as the image has it: its hard lookups, with
/// the image's PCF kernel; with the Sun a disc, the walk where a penumbra
/// reaching it is `WALK_MIN_REACH` texels wide (`near`: the facet may be in
/// one), -1 where that walk is queued, its share added once walked.
fn facet_sun(f: u32, pos: vec3<f32>, n: vec3<f32>, layer: u32, horizon: f32, near: bool, kernel: SunKernel) -> f32 {
    let ndotl = dot(n, normalize(view.light.pos - pos));
    if ndotl <= 0.0 {
        return 0.0;
    }
    let at = sun_at(pos, n, ndotl, layer);
    var reach = vec2<f32>(0.0);
    if near {
        reach = sun_reaches(layer, at.uv, at.depth, at.bias, at.tan_uv);
    }
    if reach.x < 0.0 {
        return 0.0;
    }
    let own = sun_lookup(layer, at, kernel);
    var theirs = 1.0;
    let others = others_slice(layer);
    if others != layer {
        theirs = sun_lookup(others, at, kernel);
    }
    if all(reach == vec2<f32>(0.0)) {
        return own * theirs * horizon;
    }
    let i = atomicAdd(&walks.count, 1u);
    if i < arrayLength(&walks.entries) {
        walks.entries[i].uv = at.uv;
        walks.entries[i].grad = sun_grad(pos, n, ndotl, layer);
        walks.entries[i].reach = reach;
        walks.entries[i].depth = at.depth;
        walks.entries[i].bias = at.bias;
        walks.entries[i].tan_uv = at.tan_uv;
        walks.entries[i].layer = layer;
        atomicStore(&walks.entries[i].hidden, 0u);
        facet_walks[i] = vec4<u32>(f, bitcast<u32>(own), bitcast<u32>(theirs), bitcast<u32>(horizon));
        return -1.0;
    }
    // No room in the queue: walked here.
    let hidden = sun_hidden(layer, at.uv, at.depth, sun_grad(pos, n, ndotl, layer), at.bias, at.tan_uv, reach, 0u, 1u);
    let seen = 1.0 - f32(hidden) / f32(32u * SUN_AZIMUTHS);
    return seen * select(own, 1.0, reach.x > 0.0) * select(theirs, 1.0, reach.y > 0.0) * horizon;
}

/// Whether a penumbra may reach any point of the facet `a`, `b`, `c` facing
/// `n` in `layer` or the other bodies' slice: the pyramid looked round once
/// for the facet, about its centre, as far again as its corners are and as
/// far from the Sun as the farthest. Most facets are far from any penumbra,
/// and four looks a facet were 8 ms of a step of the Didymos pair.
fn facet_near_penumbra(a: vec3<f32>, b: vec3<f32>, c: vec3<f32>, n: vec3<f32>, layer: u32) -> bool {
    let m = view.light.view_proj_layers[layer];
    let res = f32(globals.shadow_resolution);
    let pa = project_light(m, a);
    let pb = project_light(m, b);
    let pc = project_light(m, c);
    let uv = (pa.xy + pb.xy + pc.xy) / 3.0;
    let spread = max(max(length(pa.xy - uv), length(pb.xy - uv)), length(pc.xy - uv)) * res + 2.0;
    let centre = (a + b + c) / 3.0;
    let ndotl = dot(n, normalize(view.light.pos - centre));
    if ndotl <= 0.0 {
        return false;
    }
    let at = sun_at(centre, n, ndotl, layer);
    let depth = max(max(pa.z, pb.z), max(pc.z, at.depth));
    let own = sun_reach_about(layer, uv, depth, at.bias, at.tan_uv, spread);
    if own.y != 0.0 {
        return true;
    }
    let others = others_slice(layer);
    return others != layer && sun_reach_about(others, uv, depth, at.bias, at.tan_uv, spread).y != 0.0;
}

/// Each facet's points that need no walk, into `facet_out`; those that do,
/// queued.
@compute @workgroup_size(64)
fn cs_facets(@builtin(global_invocation_id) id: vec3<u32>) {
    let f = id.y * facet_query.stride + id.x;
    if f >= facet_query.n_facets {
        return;
    }
    var i = vec3<u32>(3u * f, 3u * f + 1u, 3u * f + 2u);
    if facet_query.is_flat == 0u {
        i = vec3<u32>(facet_indices[3u * f], facet_indices[3u * f + 1u], facet_indices[3u * f + 2u]);
    }
    let a = facet_vertex(i.x);
    let b = facet_vertex(i.y);
    let c = facet_vertex(i.z);
    // The facet as a flat plate, as the thermophysical model has it.
    let n = normalize(cross(b - a, c - a));
    let centre = (a + b + c) / 3.0;
    // A body with a horizon map is not in its own layer: its own shadows are
    // the map's, from the facet's centre.
    var horizon = 1.0;
    if (body.flags & 16u) != 0u {
        horizon = horizon_seen(f, centre, normalize(view.light.pos - centre));
    }
    var sum = 4.0 * horizon;
    // Nothing in its layer this frame (flag bit 5): nothing there to read.
    if (body.flags & 32u) == 0u {
        // The body's own layer; with cascades, each point the one that holds
        // it -- a facet's corner can be past its centre's.
        let cascaded = view.light.cascades > 0u;
        let layer = select(body_layer(), shadow_layer_at(centre), cascaded);
        let near = view.light.sun_radius > 0.0 && facet_near_penumbra(a, b, c, n, layer);
        // The PCF kernel's steps, the same at the four points of a flat facet
        // in one layer.
        let ndotl = max(dot(n, normalize(view.light.pos - centre)), 0.0);
        var kernel: SunKernel;
        if globals.shadow_pcf > 0u {
            kernel = sun_kernel(centre, n, ndotl, layer);
        }
        sum = 0.0;
        for (var k = 0u; k < 4u; k++) {
            let p = select(select(select(centre, c, k == 2u), b, k == 1u), a, k == 0u);
            var at_layer = layer;
            var at_kernel = kernel;
            if cascaded {
                at_layer = shadow_layer_at(p);
                if at_layer != layer && globals.shadow_pcf > 0u {
                    at_kernel = sun_kernel(centre, n, ndotl, at_layer);
                }
            }
            sum += max(facet_sun(f, p, n, at_layer, horizon, near, at_kernel), 0.0);
        }
    }
    atomicStore(&facet_out[f], u32(sum * FACET_FIXED + 0.5));
}

/// Each walked point's share, into its facet's sum.
@compute @workgroup_size(64)
fn cs_facets_walked(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let n = min(atomicLoad(&walks.count), arrayLength(&walks.entries));
    for (var i = id.x; i < n; i += groups.x * 64u) {
        let info = facet_walks[i];
        let reach = walks.entries[i].reach;
        let seen = 1.0 - f32(atomicLoad(&walks.entries[i].hidden)) / f32(32u * SUN_AZIMUTHS);
        let v = seen * select(bitcast<f32>(info.y), 1.0, reach.x > 0.0) * select(bitcast<f32>(info.z), 1.0, reach.y > 0.0);
        atomicAdd(&facet_out[info.x], u32(v * bitcast<f32>(info.w) * FACET_FIXED + 0.5));
    }
}

// The camera's depth buffer cleared: nothing drawn (`gpu::DEPTH_CLEAR`).
const DEPTH_CLEAR: f32 = 0.0;

/// The queue, walked: one invocation per receiver and direction, each
/// direction's rings added to its receiver's. As many invocations as fill
/// the GPU, each taking the next of the queue's `32 n` until none is left.
@compute @workgroup_size(64)
fn cs_walk(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let n = min(atomicLoad(&walks.count), arrayLength(&walks.entries));
    let stride = groups.x * 64u;
    for (var k = id.x; k < n * SUN_AZIMUTHS; k += stride) {
        let i = k / SUN_AZIMUTHS;
        let hidden = sun_hidden(walks.entries[i].layer, walks.entries[i].uv, walks.entries[i].depth,
            walks.entries[i].grad, walks.entries[i].bias, walks.entries[i].tan_uv, walks.entries[i].reach,
            k % SUN_AZIMUTHS, SUN_AZIMUTHS);
        if hidden > 0u {
            atomicAdd(&walks.entries[i].hidden, hidden);
        }
    }
}

/// What the penumbra pass found for this fragment's surface, as `sun_seen`
/// has it: the disc's fraction its walked slices leave and which they
/// were, -1 for the hard lookups' answer; -2 where it queued a walk it had
/// no room for, for the main pass to walk itself. The pixel's
/// own centre first; where that is another surface or none -- an edge, a
/// limb -- the nearest of its neighbours that is this one, a pixel away;
/// and with none, the hard lookup. Walking here instead, for the 1,600
/// pixels of Didymos's limb and Dimorphos's edge that are not their own
/// centres' surface, cost 13 ms: each held a SIMD group of the main pass.
fn sun_walked(frag: vec4<f32>, dz: f32) -> vec3<f32> {
    if view.light.penumbra == 0u {
        return vec3<f32>(-2.0, 0.0, 0.0);
    }
    let p = vec2<i32>(frag.xy);
    let last = vec2<i32>(textureDimensions(t_walk)) - 1;
    for (var k = 0; k < 9; k++) {
        // 0, then the four sides, then the corners.
        let o = vec2<i32>(NEIGHBOURS[k]);
        let c = textureLoad(t_walk, clamp(p + o, vec2<i32>(0), last), 0).xy;
        let tol = select(3.0, 2.0, k == 0) * dz + 1.0e-6 * frag.z;
        if c.x == 0u || abs(bitcast<f32>(c.y) - frag.z) > tol {
            continue;
        }
        if c.x == 2u {
            return vec3<f32>(0.0, 1.0, 1.0);
        }
        if c.x < 4u {
            return vec3<f32>(select(-1.0, -2.0, c.x == 3u), 0.0, 0.0);
        }
        let w = walked.entries[c.x - 4u];
        return vec3<f32>(1.0 - f32(w.hidden) / f32(32u * SUN_AZIMUTHS),
                         select(vec2<f32>(0.0), vec2<f32>(1.0), w.reach > vec2<f32>(0.0)));
    }
    return vec3<f32>(-1.0, 0.0, 0.0);
}

const NEIGHBOURS = array<vec2<f32>, 9>(
    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 0.0), vec2<f32>(-1.0, 0.0), vec2<f32>(0.0, 1.0), vec2<f32>(0.0, -1.0),
    vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, -1.0),
);

// A body's own shadows from its horizon map (`app::horizon`), drawn into no
// layer of its own: the Sun stands above the facet's horizon or not, and as
// a disc, as much of it as the horizon leaves -- a line across it, from the
// horizon where the Sun's azimuth falls between two of the map's and its
// slope there, the limb darkened. The frame is the one the map was made in:
// up the radius, east along the body's spin axis cross it, north up it.
const HORIZON_AZIMUTHS: u32 = 32u;
const HORIZON_TOPS: vec2<u32> = vec2<u32>(1440u, 720u);

/// Azimuth `k`'s horizon of facet `f`, the sine of its elevation.
fn horizon_sine(f: u32, k: u32) -> f32 {
    let w = horizons[f * (HORIZON_AZIMUTHS / 2u) + k / 2u];
    let bits = (w >> (16u * (k & 1u))) << 16u;
    return f32(bitcast<i32>(bits) >> 16u) / 32767.0;
}

/// The limb-darkened disc's light above a line `d` of its radii below its
/// centre, as a fraction of all of it.
fn disc_above(d: f32) -> f32 {
    if d >= 1.0 {
        return 1.0;
    }
    if d <= -1.0 {
        return 0.0;
    }
    let u = LIMB_DARKENING;
    let g = (1.0 - u) * (d * sqrt(1.0 - d * d) + asin(d) + 0.5 * PI) + u * 0.5 * PI * (d - d * d * d / 3.0 + 2.0 / 3.0);
    return g / (PI * (1.0 - u / 3.0));
}

/// What a point `pos` of facet `f` sees of the Sun, `to_sun` its way, over
/// the facet's horizon.
fn horizon_seen(f: u32, pos: vec3<f32>, to_sun: vec3<f32>) -> f32 {
    let up = normalize(pos - body.mat[3].xyz);
    var east = cross(normalize(body.mat[2].xyz), up);
    if dot(east, east) < 1.0e-12 {
        east = normalize(body.mat[1].xyz);
    } else {
        east = normalize(east);
    }
    let north = cross(up, east);
    let sun_up = dot(to_sun, up);
    // Above every horizon about it, the map is not read: where the Sun
    // stands high, which is most of a lit planet, a read per pixel from a
    // map of 825 MB, in an order that is not the image's, cost the main
    // pass 2 ms on Mars at AFC's closest approach. The body's highest
    // first, then the highest in the degree of longitude and latitude the
    // point is in, a few hundred kilobytes read as the image goes.
    let radius = view.light.sun_radius / max(length(view.light.pos - pos), 1.0e-30);
    let e = asin(clamp(sun_up, -1.0, 1.0));
    if e - radius > asin(body.horizon_top) {
        return 1.0;
    }
    let v = pos - body.mat[3].xyz;
    let lon = atan2(dot(v, normalize(body.mat[1].xyz)), dot(v, normalize(body.mat[0].xyz)));
    let lat = asin(clamp(dot(v, normalize(body.mat[2].xyz)) / length(v), -1.0, 1.0));
    let cell = min(vec2<u32>(vec2<f32>((lon + PI) / (2.0 * PI), (0.5 * PI - lat) / PI) * vec2<f32>(HORIZON_TOPS)), HORIZON_TOPS - 1u);
    let tops = arrayLength(&horizons) - HORIZON_TOPS.x * HORIZON_TOPS.y;
    let top = f32(bitcast<i32>(horizons[tops + cell.y * HORIZON_TOPS.x + cell.x])) / 32767.0;
    if e - radius > asin(clamp(top, -1.0, 1.0)) {
        return 1.0;
    }
    let step = 2.0 * PI / f32(HORIZON_AZIMUTHS);
    // Azimuth `k` is the middle of its bin, `(k + 1/2) step` from north.
    var x = atan2(dot(to_sun, east), dot(to_sun, north)) / step - 0.5;
    x -= f32(HORIZON_AZIMUTHS) * floor(x / f32(HORIZON_AZIMUTHS));
    let k = u32(x) % HORIZON_AZIMUTHS;
    let t = fract(x);
    let h0 = horizon_sine(f, k);
    let h1 = horizon_sine(f, (k + 1u) % HORIZON_AZIMUTHS);
    let h = mix(h0, h1, t);
    if view.light.sun_radius <= 0.0 {
        return select(0.0, 1.0, sun_up > h);
    }
    // The horizon as a line across the disc, its slope the elevation's per
    // radian of azimuth over the cosine of the Sun's: how far below the
    // disc's centre it passes, in its radii.
    let slope = (asin(h1) - asin(h0)) / step / max(cos(e), 1.0e-3);
    return disc_above((e - asin(h)) / sqrt(1.0 + slope * slope) / radius);
}

/// The surface a fragment shows: from the vertex stage for a smooth mesh,
/// from its facet for a flat one -- normal, colour, mode and value read
/// here by facet, which is what lets the vertex stage skip them and the
/// draw be indexed over shared vertices.
fn surface(vertex: VertexOutput, prim: u32) -> Surface {
    var in: Surface;
    in.world_pos = vertex.world_pos;
    in.color = vertex.color; //@full
    in.world_normal = vertex.world_normal; //@full
    in.value = vertex.value; //@full
    in.color_mode = vertex.color_mode; //@full
    in.facet = prim;
    if (body.flags & 1u) != 0u {
        var facet = prim;
        if (body.flags & 8u) != 0u { facet = facet_of[chunk.facet_base + prim]; } //@imm
        in.facet = facet;
        let attr = attrs[facet];
        let normal_matrix = mat3x3<f32>(body.normal[0].xyz, body.normal[1].xyz, body.normal[2].xyz);
        in.color = attr.color;
        in.color_mode = attr.mode;
        in.value = attr.value;
        in.world_normal = normalize(normal_matrix * attr.normal);
    }
    return in;
}

@fragment
fn fs_main(vertex: VertexOutput, @builtin(primitive_index) prim: u32) -> @location(0) vec4<f32> { //@prim
fn fs_main(vertex: VertexOutput) -> @location(0) vec4<f32> { //@noprim
    let prim = vertex.facet; //@noprim
    // Here, while every fragment of the quad is still running.
    let dz = fwidth(vertex.clip_position.z);
    var in = surface(vertex, prim);
    in.frag = vertex.clip_position;
    in.dz = dz;
    // The barycentrics are corners' coordinates, which only a non-indexed
    // draw has (flag bit 2): a shared-vertex draw would be covered in noise,
    // so it is drawn shaded instead. The CPU side warns once for a smooth
    // mesh, and draws a flat one non-indexed whenever the wireframe is on.
    if globals.wireframe_mode == 1u && (body.flags & 4u) != 0u { return wireframe_only(vertex.bary); } //@full
    return wireframe_over(fs_shaded(in), vertex.bary); //@full
    return fs_shaded(in); //@lean
}

// Wireframe-only: keep just the edge fragments, so the mesh reads as a pure
// line drawing with the geometry still depth-tested behind it.
//
// Thresholded rather than alpha-blended because the pipeline blend state is
// REPLACE, so a fractional alpha would simply be ignored. That costs
// antialiasing here; the overlay path below still gets it, since it mixes
// against a colour it actually has in hand.
fn wireframe_only(bary: vec3<f32>) -> vec4<f32> {
    if wireframe_edge(bary) < 0.5 {
        discard;
    }
    return vec4<f32>(globals.wireframe_color, 1.0);
}

// Overlay: composite the line over the shaded surface in the same pass, so
// there is no second draw and therefore no depth fighting.
fn wireframe_over(shaded: vec4<f32>, bary: vec3<f32>) -> vec4<f32> {
    if globals.wireframe_mode != 2u || (body.flags & 4u) == 0u {
        return shaded;
    }
    // Faded by resolution, so a body far enough away to be a smear of facets
    // shows its shading rather than its wireframe. Only here:
    // `wireframe_mode == 1` has nothing behind the lines to fade into, and
    // fading them would make a distant mesh disappear altogether.
    var edge = wireframe_edge(bary);
    if globals.wireframe_fade != 0u {
        edge = edge * wireframe_resolution_fade(bary);
    }
    return vec4<f32>(mix(shaded.rgb, globals.wireframe_color, edge), shaded.a);
}

// The scattering laws of `src/scattering.rs`, in its convention: the
// radiance leaving a facet is `r J mu0`, so the I/F is `pi r mu0`. Each is
// that file's function written again, and `tests/test_scattering_render.py`
// holds the two to each other.

const PI: f32 = 3.14159265358979;

// Chandrasekhar's H, Hapke's 2002 approximation: `h_function`.
fn hapke_h(w: f32, x: f32) -> f32 {
    if w <= 0.0 || x <= 0.0 {
        return 1.0;
    }
    let gamma = sqrt(max(1.0 - w, 0.0));
    let r0 = (1.0 - gamma) / (1.0 + gamma);
    let ln = log((1.0 + x) / x);
    return 1.0 / (1.0 - w * x * (r0 + (1.0 - 2.0 * r0 * x) * 0.5 * ln));
}

// Two-lobe Henyey-Greenstein in the phase angle, the backward lobe largest
// at opposition: `henyey_greenstein`.
fn hapke_p(b: f32, c: f32, alpha: f32) -> f32 {
    let ca = cos(alpha);
    let b2 = b * b;
    let back = (1.0 - b2) / pow(max(1.0 - 2.0 * b * ca + b2, 1e-12), 1.5);
    let fwd = (1.0 - b2) / pow(max(1.0 + 2.0 * b * ca + b2, 1e-12), 1.5);
    return c * back + (1.0 - c) * fwd;
}

// The shadow-hiding opposition surge: `opposition_surge`.
fn hapke_surge(b0: f32, h: f32, alpha: f32) -> f32 {
    if b0 <= 0.0 || h <= 0.0 {
        return 0.0;
    }
    let t = tan(0.5 * alpha);
    if t < 0.0 || t > 1e30 {
        return 0.0;
    }
    return b0 / (1.0 + t / h);
}

// Hapke's 1984 macroscopic roughness, `(mu0e, mue, S)`: `roughness_terms`,
// branch for branch. A negative or endless `tan(psi / 2)` is psi past pi in
// f32, where the limit is zero.
fn hapke_roughness(theta_bar: f32, mu0: f32, mu: f32, alpha: f32) -> vec3<f32> {
    let sin_i = sqrt(max(1.0 - mu0 * mu0, 0.0));
    let sin_e = sqrt(max(1.0 - mu * mu, 0.0));
    var psi = 0.0;
    if sin_i * sin_e >= 1e-7 {
        psi = acos(clamp((cos(alpha) - mu0 * mu) / (sin_i * sin_e), -1.0, 1.0));
    }
    let tan_tb = tan(theta_bar);
    let cot_tb = 1.0 / tan_tb;
    let chi = 1.0 / sqrt(1.0 + PI * tan_tb * tan_tb);
    let ci = cot_tb * mu0 / max(sin_i, 1e-6);
    let ce = cot_tb * mu / max(sin_e, 1e-6);
    let e1i = exp(-(2.0 / PI) * ci);
    let e2i = exp(-(1.0 / PI) * ci * ci);
    let e1e = exp(-(2.0 / PI) * ce);
    let e2e = exp(-(1.0 / PI) * ce * ce);
    let eta_i = chi * (mu0 + sin_i * tan_tb * e2i / (2.0 - e1i));
    let eta_e = chi * (mu + sin_e * tan_tb * e2e / (2.0 - e1e));
    let cos_psi = cos(psi);
    let half = 0.5 * psi;
    let sin2_half = sin(half) * sin(half);
    let tan_half = tan(half);
    var f = 0.0;
    if tan_half >= 0.0 && tan_half < 1e30 {
        f = exp(-2.0 * tan_half);
    }
    var mu0e: f32;
    var mue: f32;
    var ratio: f32;
    if mu0 >= mu {
        // i <= e
        let d = 2.0 - e1e - (psi / PI) * e1i;
        mu0e = chi * (mu0 + sin_i * tan_tb * (cos_psi * e2e + sin2_half * e2i) / d);
        mue = chi * (mu + sin_e * tan_tb * (e2e - sin2_half * e2i) / d);
        ratio = mu0 / eta_i;
    } else {
        // i > e
        let d = 2.0 - e1i - (psi / PI) * e1e;
        mu0e = chi * (mu0 + sin_i * tan_tb * (e2i - sin2_half * e2e) / d);
        mue = chi * (mu + sin_e * tan_tb * (cos_psi * e2i + sin2_half * e2e) / d);
        ratio = mu / eta_e;
    }
    let shadow = (mue / eta_e) * (mu0 / eta_i) * chi / (1.0 - f + f * chi * ratio);
    return vec3<f32>(mu0e, mue, shadow);
}

// The I/F a unit colour gives under a body's law, `pi r(i, e, alpha) mu0`.
// Law 0 is Lambert at albedo 1, which is `mu0`: the shading as it always
// was. 1 is the mix, `a = (w, c)`; 2 is Hapke, `a = (w, b, c, b0)`,
// `b = (h, theta_bar, k)`, `k` the porosity factor (Hapke 2008, 1 for none). `mu` is kept off zero: a smooth mesh's interpolated
// normal can turn just past the limb on a facet still drawn.
fn law_factor(law: u32, a: vec4<f32>, b: vec4<f32>, mu0: f32, mu_in: f32, alpha: f32) -> f32 {
    if mu0 <= 0.0 {
        return 0.0;
    }
    if law == 0u {
        return mu0;
    }
    let mu = max(mu_in, 1e-4);
    if law == 1u {
        return mu0 * a.x * (a.y / (4.0 * (mu0 + mu)) + (1.0 - a.y));
    }
    let w = a.x;
    let k = max(b.z, 1.0);
    let p = hapke_p(a.y, a.z, alpha);
    let bg = hapke_surge(a.w, b.x, alpha);
    if b.y <= 0.0 {
        return k * mu0 * w / (4.0 * (mu0 + mu)) * ((1.0 + bg) * p + hapke_h(w, mu0 / k) * hapke_h(w, mu / k) - 1.0);
    }
    let rough = hapke_roughness(b.y, mu0, mu, alpha);
    if rough.x <= 0.0 || rough.y <= 0.0 {
        return 0.0;
    }
    // `pi r mu0`, with `r` on the effective cosines and divided by the true
    // `mu0`, as `reflectance_rough` has it: the `mu0` cancels.
    return k * w / 4.0 * rough.x / (rough.x + rough.y)
        * ((1.0 + bg) * p + hapke_h(w, rough.x / k) * hapke_h(w, rough.y / k) - 1.0) * rough.z;
}

// A dusty atmosphere over the surface: `src/atmosphere.rs`, the same
// formulas in f32, which `tests/test_atmosphere_render.py` holds to it.
struct Air {
    dust: f32,
    down: f32,
    up: f32,
    sky: f32,
    up_diffuse: f32,
    spherical: f32,
    twilight: f32,
};

struct Beam {
    ap: f32,
    bp: f32,
    a: f32,
    b: f32,
    direct: f32,
    diffuse: f32,
};

fn air_mass(mu: f32, x: f32) -> f32 {
    let m = max(mu, 0.0);
    return 2.0 / (m + sqrt(m * m + x));
}

fn air_hg(g: f32, c: f32) -> f32 {
    return (1.0 - g * g) / pow(1.0 + g * g + 2.0 * g * c, 1.5);
}

/// Eddington's two-stream solution for a beam of cosine `uu`, airmass `nn`.
fn air_beam(uu: f32, nn: f32, tp: f32, wp: f32, gp: f32, c1: f32, c2: f32, k: f32, r: f32, e: f32) -> Beam {
    let c3 = (2.0 - 3.0 * gp * uu) / 4.0;
    let c4 = 1.0 - c3;
    let d = k * k - nn * nn;
    var o: Beam;
    o.ap = wp * (c3 * (c1 - nn) + c2 * c4) / d;
    o.bp = wp * (c4 * (c1 + nn) + c2 * c3) / d;
    o.direct = exp(-tp * nn);
    o.a = (o.bp * r * e - o.ap * o.direct) / (1.0 - r * r * e * e);
    o.b = -o.bp - o.a * r * e;
    o.diffuse = (o.a * r + o.b * e + o.bp * o.direct) / uu;
    return o;
}

/// The atmosphere at a point `height` above its ellipsoid: `mu0`, `mu` the
/// Sun's and the camera's cosines from the local vertical, `alpha` the phase
/// angle. The air thins with height as the pressure does.
fn air(mu0: f32, mu: f32, alpha: f32, height: f32) -> Air {
    let tau = body.atmosphere[0] * exp(-height / body.atmosphere[1]);
    let x = 8.0 * body.atmosphere[1] / (PI * body.atmosphere[2]);
    let om = body.atmosphere[3];
    let g1 = body.atmosphere[4];
    let g2 = body.atmosphere[5];
    let q = body.atmosphere[6];
    var o: Air;
    let depression = degrees(asin(clamp(-mu0, 0.0, 1.0)));
    o.twilight = exp(-0.24 * depression - 0.0235 * depression * depression);
    let n0 = air_mass(mu0, x);
    let n = air_mass(mu, x);
    let u0 = 1.0 / n0;
    let u = 1.0 / n;
    let g = q * g1 + (1.0 - q) * g2;
    let f = g * g;
    let tp = max((1.0 - om * f) * tau, 1.0e-6);
    let wp = (1.0 - f) * om / (1.0 - om * f);
    let gp = g / (1.0 + g);
    let ca = cos(alpha);
    let phase = q * air_hg(g1, ca) + (1.0 - q) * air_hg(g2, ca);
    let single = om / (1.0 - om * f) * phase / 4.0 * u0 / (u0 + u) * (1.0 - exp(-tp * (n0 + n)));
    let c1 = (7.0 - wp * (4.0 + 3.0 * gp)) / 4.0;
    let c2 = -(1.0 - wp * (4.0 - 3.0 * gp)) / 4.0;
    let k = sqrt(max(c1 * c1 - c2 * c2, 0.0));
    let r = c2 / (c1 + k);
    let e = exp(-k * tp);
    let sun = air_beam(u0, n0, tp, wp, gp, c1, c2, k, r, e);
    let lp = (1.0 - exp(-tp * (n - k))) / (1.0 - k * u);
    let lm = (1.0 - exp(-tp * (n + k))) / (1.0 + k * u);
    let l0 = u0 / (u0 + u) * (1.0 - exp(-tp * (n + n0)));
    let sig = (1.0 + r) * (sun.a * e * lp + sun.b * lm) + (sun.ap + sun.bp) * l0;
    let dif = (1.0 - r) * (sun.a * e * lp - sun.b * lm) + (sun.ap - sun.bp) * l0;
    let multiple = (1.05 + 0.33 * log(1.0 + 1.0 / tp)) * wp * (0.5 * sig + 0.75 * gp * u * dif);
    let view = air_beam(u, n, tp, wp, gp, c1, c2, k, r, e);
    o.dust = single + multiple;
    o.down = sun.direct;
    o.up = view.direct;
    o.sky = max(mu0, 0.0) * sun.diffuse;
    o.up_diffuse = view.diffuse;
    o.spherical = r * (1.0 - e * e) / (1.0 - r * r * e * e);
    return o;
}

fn fs_shaded(in: Surface) -> vec4<f32> {
    // A facet carrying its own colour mode overrides the global one. This is
    // what makes a selection visible on one facet without unlighting the rest
    // of the body: the attribute has been on the mesh and in the vertex
    // buffer all along, and nothing read it.
    if in.color_mode == 1u {
        var picked = in.color;
        if globals.srgb_mode == 0 {
            picked = srgb_to_linear(picked, globals.gamma);
        }
        return vec4<f32>(picked, 1.0);
    }

    // `color_mode == 1` is the unlit mode, and unlit is what a quantitative
    // figure wants: shading a data map makes one value read as two colours.
    // So that mode *is* the data map, when the mesh carries values -- there
    // is no second switch to keep in step with it. A mesh without values
    // falls back to its vertex colours, which is what mode 1 always meant.
    let has_values = (body.flags & 2u) != 0u;

    if globals.color_mode == 1 {
        var color = select(in.color, colormap_lookup(in.value), has_values);
        if globals.srgb_mode == 0 {
            color = srgb_to_linear(color, globals.gamma);
        }
        return vec4<f32>(color, 1.0);
    } else if globals.color_mode == 2 {
        var color = globals.color;
        if globals.srgb_mode == 0 {
            color = srgb_to_linear(color, globals.gamma);
        }
        return vec4<f32>(color, 1.0);
    }
    // } else if globals.color_mode == ??? {

    // 0 or else
    //
    // else {
    let object_color = vec4<f32>(in.color, 1.0);

    let light_dir = normalize(view.light.pos - in.world_pos);
    let ndotl = max(dot(in.world_normal, light_dir), 0.0);

    let layer = shadow_layer_at(in.world_pos);
    let at = sun_at(in.world_pos, in.world_normal, ndotl, layer);
    var kernel: SunKernel;
    if globals.shadow_pcf > 0u {
        kernel = sun_kernel(in.world_pos, in.world_normal, ndotl, layer);
    }
    let uv = at.uv;
    let depth = at.depth;
    let bias = at.bias;

    // Nothing in its layer this frame (flag bit 5): no lookup. Its body and
    // the other bodies apart (`others_slice`), each slice's, both to be lit.
    let mapped = (body.flags & 32u) == 0u;
    let others = others_slice(layer);
    var own = 1.0;
    var theirs = 1.0;
    if mapped {
        own = sun_lookup(layer, at, kernel);
        if others != layer {
            theirs = sun_lookup(others, at, kernel);
        }
    }
    var shadow = own * theirs;

    // The Sun a disc: what the receiver sees of it, where a penumbra
    // reaching it is `WALK_MIN_REACH` texels wide; the hard lookup above
    // stands elsewhere. As the penumbra pass found it, or found here where it
    // did not; its plane unclamped, the disc's horizon being the plane's own.
    // A slice the walk did not take, nothing in it in reach, answers by its
    // hard lookup.
    if mapped && view.light.sun_radius > 0.0 && ndotl > 0.0 {
        var soft = sun_walked(in.frag, in.dz);
        if soft.x < -1.5 {
            soft = sun_seen(layer, uv, depth, sun_grad(in.world_pos, in.world_normal, ndotl, layer), bias, at.tan_uv);
        }
        if soft.x >= 0.0 {
            shadow = soft.x * select(own, 1.0, soft.y > 0.0) * select(theirs, 1.0, soft.z > 0.0);
        }
    }
    if (body.flags & 16u) != 0u && ndotl > 0.0 {
        shadow *= horizon_seen(in.facet, in.world_pos, light_dir);
    }
    if view.light.air_count > 0u && ndotl > 0.0 {
        shadow *= through_air(in.world_pos, light_dir);
    }

    // no shadow
    if globals.color_mode == 3 {
        shadow = 1.0;
    }

    // lighting: the body's law, from where the Sun and the camera are
    let view_dir = normalize(globals.camera_pos - in.world_pos);
    let alpha = acos(clamp(dot(light_dir, view_dir), -1.0, 1.0));
    let q = body.law_params;
    let reflected = law_factor(body.law, vec4<f32>(q[0], q[1], q[2], q[3]), vec4<f32>(q[4], q[5], q[6], q[7]),
        ndotl, dot(in.world_normal, view_dir), alpha);
    let ambient_color = view.light.color * globals.ambient_strength;
    let diffuse_color = view.light.color * reflected;
    var color = (ambient_color + diffuse_color * shadow) * object_color.xyz;

    // Under an atmosphere: the dust's own light, and the surface seen
    // through it, lit by the beam that got through -- where the shadow lets
    // it -- and by the sky; the planet a sphere about the body's centre.
    if body.atmosphere_on != 0u {
        let from_centre = in.world_pos - body.mat[3].xyz;
        let vertical = normalize(from_centre);
        // Height above the ellipsoid, about the body's own z axis.
        let sin_lat = dot(vertical, normalize(body.mat[2].xyz));
        let eq = body.atmosphere[2];
        let po = body.atmosphere[8];
        let cos_lat2 = max(1.0 - sin_lat * sin_lat, 0.0);
        let level = eq * po / sqrt(po * po * cos_lat2 + eq * eq * sin_lat * sin_lat);
        let mu0 = dot(vertical, light_dir);
        let a = air(mu0, clamp(dot(vertical, view_dir), 0.0, 1.0), alpha, length(from_centre) - level);
        // The beam the surface reflects by its law, times the colour; the
        // sky's light, and its surroundings', as a diffuse surface does, by
        // its law's albedo for a diffuse sky -- the colour itself for
        // Lambert, but with a law the colour is a scale on the law's own
        // brightness, about 4 for Mars under Hapke.
        let albedo = object_color.xyz;
        let diffuse = albedo * body.atmosphere[9];
        var around = vec3<f32>(body.atmosphere[7]);
        if body.atmosphere[7] < 0.0 {
            around = diffuse;
        }
        let sun_up = select(0.0, shadow, mu0 > 0.0);
        let beam = ndotl * sun_up * a.down;
        let surface = (reflected * albedo * sun_up * a.down * a.up + diffuse * a.sky * a.up
            + (beam + a.sky) * around * a.up_diffuse) / (1.0 - around * a.spherical);
        color = ambient_color * albedo + view.light.color * a.twilight * (vec3<f32>(a.dust) + surface);
    }
    
    if globals.srgb_mode == 1 {
        color = srgb_to_linear(color, globals.gamma);
    }

    return vec4<f32>(color, object_color.a);
}
