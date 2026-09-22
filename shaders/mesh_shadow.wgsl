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
    // @location(17) color_mode: u32,
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

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(1) color: vec3<f32>,
    @location(2) world_normal: vec3<f32>,
    @location(3) world_pos: vec3<f32>,
    // Barycentric coordinate of this corner, interpolated across the
    // triangle so the fragment shader knows its distance to each edge.
    @location(4) bary: vec3<f32>,
    // Flat-shaded (per-face) interpolation: a flag must not be blended
    // across the triangle.
    @location(5) @interpolate(flat) flags: u32,
    // Flat for the same reason as `flags`: an index must not be blended.
    @location(6) @interpolate(flat) shadow_layer: u32,
    // Flat: every corner of a facet carries the same value, and interpolating
    // would smear one facet's datum into its neighbour.
    @location(7) @interpolate(flat) value: f32,
    // Per-vertex colour mode, overriding the global one for this facet alone.
    // Flat, like the others: a mode must not be blended across a triangle.
    @location(8) @interpolate(flat) color_mode: u32,
    // The instance's normal matrix, for a flat mesh whose facet normal is
    // only known in the fragment stage (see `fs_main`). Flat: one per body.
    @location(9) @interpolate(flat) normal_row_0: vec3<f32>,
    @location(10) @interpolate(flat) normal_row_1: vec3<f32>,
    @location(11) @interpolate(flat) normal_row_2: vec3<f32>,
    // The facet, on a device without `primitive_index`: `vertex_index / 3`
    // of a non-indexed flat draw. Unused (zero) where the builtin exists.
    @location(12) @interpolate(flat) facet: u32,
};

fn srgb_to_linear(color: vec3<f32>, gamma: f32) -> vec3<f32> {
    return pow(color, vec3<f32>(gamma));
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
        let attr = attrs[vertex_index];
        out.color = attr.color;
        out.color_mode = attr.mode;
        out.world_normal = normalize(normal_matrix * attr.normal);
        out.value = attr.value;
    }
    out.normal_row_0 = normal_matrix[0];
    out.normal_row_1 = normal_matrix[1];
    out.normal_row_2 = normal_matrix[2];
    out.facet = vertex_index / 3u; //@noprim

    var world_pos = model_matrix * vec4<f32>(vertex.pos, 1.0);
    out.world_pos = world_pos.xyz;

    out.clip_position = view.camera.view_proj * world_pos;

    // A flat mesh drawn non-indexed -- one vertex per triangle corner, which
    // is how it is drawn whenever the wireframe is on (flag bit 2) -- has
    // vertex_index modulo 3 *as* the corner index, giving barycentric
    // coordinates for free. Meaningless for a shared-vertex draw, indexed
    // flat or smooth, and the fragment stage does not read it then.
    let corner = vertex_index % 3u;
    out.bary = vec3<f32>(
        f32(corner == 0u),
        f32(corner == 1u),
        f32(corner == 2u),
    );
    out.flags = instance.flags;
    out.shadow_layer = instance.shadow_layer;

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

// Ceiling on the receiver-plane slope, as tan(incidence).
//
// The per-tap adjustment extends the receiver's plane out to the tap. That is
// right where the tap lands on the receiver itself and wrong where it lands
// on an occluder -- there the receiver's slope says nothing about the stored
// depth -- or where the surface has curved away from its own plane. Both
// failures grow with the slope, and at a terminator the slope runs to
// infinity: a receiver at 89.9 degrees extrapolated sixteen texels rises a
// kilometre in depth, past Dimorphos, and the tap goes lit.
//
// This used to be a fixed depth ceiling, `GRAD_MAX = 1e-4`, about one texel
// at 8192. That left the adjustment nothing to say beyond the first tap, and
// the acne it should have prevented was being suppressed instead by scaling
// the normal offset with the kernel -- which lifted the lookup up to
// seventeen texels off the surface and ate the shadow at grazing incidence
// (see `notes/2026-09-17_pcf_erosion.md`). A slope ceiling scales with the
// tap's distance, so a wall stays a wall out to the kernel's edge, and it
// bites only where the planar assumption is already false.
//
// 85 degrees, not 80: on a sphere most of the acne-prone surface is the band
// just short of the terminator, and a ceiling at tan(80) let it self-shadow
// again -- 211 -> 3,075 px on Didymos at pcf 4, against 473 at tan(85) and
// 211 unclamped. Unclamped would do here, but a close occluder at a
// terminator -- a boulder's shadow at sunset -- is exactly where an unbounded
// extrapolation flips, so the ceiling stays, set where it costs almost
// nothing.
const GRAD_MAX_SLOPE: f32 = 11.43; // tan(85 deg)

@group(2) @binding(0)
var t_shadow: texture_depth_2d_array;
@group(2) @binding(1)
var s_shadow: sampler_comparison;

@fragment
fn fs_main(vertex: VertexOutput, @builtin(primitive_index) prim: u32) -> @location(0) vec4<f32> { //@prim
fn fs_main(vertex: VertexOutput) -> @location(0) vec4<f32> { //@noprim
    let prim = vertex.facet; //@noprim
    // A flat mesh's surface is the facet's, not the corners': its normal,
    // colour, mode and value are read here by facet, which is what lets the
    // vertex stage skip them and the draw be indexed over shared vertices.
    var in = vertex;
    if (in.flags & 1u) != 0u {
        let attr = attrs[prim];
        let normal_matrix = mat3x3<f32>(in.normal_row_0, in.normal_row_1, in.normal_row_2);
        in.color = attr.color;
        in.color_mode = attr.mode;
        in.value = attr.value;
        in.world_normal = normalize(normal_matrix * attr.normal);
    }

    // The barycentrics are corners' coordinates, which only a non-indexed
    // draw has (flag bit 2): a shared-vertex draw would be covered in noise,
    // so it is drawn shaded instead. The CPU side warns once for a smooth
    // mesh, and draws a flat one non-indexed whenever the wireframe is on.
    let can_wireframe = (in.flags & 4u) != 0u;

    // Wireframe-only: keep just the edge fragments, so the mesh reads as a
    // pure line drawing with the geometry still depth-tested behind it.
    //
    // Thresholded rather than alpha-blended because the pipeline blend state
    // is REPLACE, so a fractional alpha would simply be ignored. That costs
    // antialiasing here; the overlay path below still gets it, since it mixes
    // against a colour it actually has in hand.
    if globals.wireframe_mode == 1u && can_wireframe {
        if wireframe_edge(in.bary) < 0.5 {
            discard;
        }
        return vec4<f32>(globals.wireframe_color, 1.0);
    }

    let shaded = fs_shaded(in);

    // Overlay: composite the line over the shaded surface in the same pass,
    // so there is no second draw and therefore no depth fighting.
    if globals.wireframe_mode == 2u && can_wireframe {
        // Faded by resolution, so a body far enough away to be a smear of
        // facets shows its shading rather than its wireframe. Only here:
        // `wireframe_mode == 1` has nothing behind the lines to fade into,
        // and fading them would make a distant mesh disappear altogether.
        var edge = wireframe_edge(in.bary);
        if globals.wireframe_fade != 0u {
            edge = edge * wireframe_resolution_fade(in.bary);
        }
        return vec4<f32>(mix(shaded.rgb, globals.wireframe_color, edge), shaded.a);
    }

    return shaded;
}

fn fs_shaded(in: VertexOutput) -> vec4<f32> {
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
    let has_values = (in.flags & 2u) != 0u;

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
    let k = 1.0 - ndotl;
    let k2 = k * k;

    // shadow: this body's own layer, with that layer's bias
    let layer = min(in.shadow_layer, max(view.light.n_layers, 1u) - 1u);
    let lb = view.light.layer_bias[layer];
    // One texel diagonal, whatever the kernel. This used to scale with the
    // PCF radius to keep far taps from self-shadowing a tilted receiver, but
    // lifting the lookup N texels off the surface moves the shadow's edge --
    // at grazing incidence by far more than N texels along the surface --
    // and Dimorphos's shadow on Didymos shrank from 78,042 to 8,539 px
    // between pcf 0 and 16 at 512. The far taps are the receiver-plane
    // term's job below; the offset only has to clear the texel it is in.
    let normal_offset = lb.x * k;
    let offset_pos = in.world_pos + in.world_normal * normal_offset;
    let light_space = view.light.view_proj_layers[layer] * vec4<f32>(offset_pos, 1.0);
    var proj = light_space.xyz / light_space.w;
    proj.y = -proj.y;
    let uv = proj.xy * 0.5 + 0.5;
    let depth = proj.z;
    let bias = max(lb.y * k2, lb.z);

    var grad = receiver_plane_grad(
        view.light.view_proj_layers[layer], offset_pos, in.world_normal);
    // `grad` is depth per uv. Per texel, in units of one texel's depth, it is
    // tan(incidence) -- which is what the ceiling is written in.
    let slope = length(grad) / (f32(globals.shadow_resolution) * max(lb.w, 1.0e-12));
    if slope > GRAD_MAX_SLOPE {
        grad *= GRAD_MAX_SLOPE / slope;
    }

    var shadow = 1.0;

    if globals.shadow_pcf == 0 {
        shadow = textureSampleCompare(
            t_shadow,
            s_shadow,
            uv,
            layer,
            depth - bias
        );
    }
    else {
        // Accumulate into a separate sum: `shadow` starts at 1.0 (the
        // no-shadow default) and adding taps onto it biased every filtered
        // result brighter by 1/(2*pcf+1)^2 -- ~+11% at pcf=1.
        var sum = 0.0;
        let texel_size = 1.0 / vec2<f32>(f32(globals.shadow_resolution));
        for (var x = -i32(globals.shadow_pcf); x <= i32(globals.shadow_pcf); x++) {
            for (var y = -i32(globals.shadow_pcf); y <= i32(globals.shadow_pcf); y++) {
                let offset = vec2<f32>(f32(x), f32(y)) * texel_size;
                // Compare against the depth the receiver actually has at
                // this tap, not at the kernel centre.
                let adj = dot(offset, grad);
                sum += textureSampleCompare(t_shadow, s_shadow, uv + offset, layer,
                                            depth + adj - bias);
            }
        }
        let taps = f32(globals.shadow_pcf * 2u + 1u);
        shadow = sum / (taps * taps);
    }

    // no shadow
    if globals.color_mode == 3 {
        shadow = 1.0;
    }

    // lighting
    let ambient_color = view.light.color * globals.ambient_strength;
    let diffuse_color = view.light.color * ndotl;
    var color = (ambient_color + diffuse_color * shadow) * object_color.xyz;
    
    if globals.srgb_mode == 1 {
        color = srgb_to_linear(color, globals.gamma);
    }

    return vec4<f32>(color, object_color.a);
}
