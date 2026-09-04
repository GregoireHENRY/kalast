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
    value_mode: u32,
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
    // Per layer: (normal_offset_scale, bias_scale, bias_minimum, unused).
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
    @location(1) tex: vec2<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) tangent: vec3<f32>,
    @location(4) bitangent: vec3<f32>,
    @location(5) color: vec3<f32>,
    @location(6) color_mode: u32,
    @location(7) extra: u32,
    @location(18) value: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex: vec2<f32>,
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
    out.tex = vertex.tex;

    // if instance.color_mode == 0 {
    //     out.color = vertex.color;
    // } else {
    //     out.color = instance.color;
    // }

    out.color = vertex.color;

    out.world_normal = normalize(normal_matrix * vertex.normal);

    var world_pos = model_matrix * vec4<f32>(vertex.pos, 1.0);
    out.world_pos = world_pos.xyz;

    out.clip_position = view.camera.view_proj * world_pos;

    // Flattened meshes store one vertex per triangle corner and are drawn
    // non-indexed, so vertex_index modulo 3 *is* the corner index -- giving
    // barycentric coordinates for free, with no extra vertex attribute.
    // Indexed (smooth) meshes share vertices, so this is meaningless for
    // them; the CPU side falls back to a line-mode pass there.
    let corner = vertex_index % 3u;
    out.bary = vec3<f32>(
        f32(corner == 0u),
        f32(corner == 1u),
        f32(corner == 2u),
    );
    out.flags = instance.flags;
    out.shadow_layer = instance.shadow_layer;
    out.value = vertex.value;

    return out;
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

// Ceiling on the per-tap receiver-plane adjustment, in normalised depth.
//
// The gradient is only valid where the occluder *is* the receiver -- a wall
// shadowing itself. Where the occluder is another surface, such as the rim
// casting onto the crater floor, the receiver's slope says nothing about the
// stored depth, and an unclamped adjustment pushes those taps out of shadow:
// the floor leak went 215 -> 3,892 px at pcf=2 with no ceiling. Measured on
// the crater, 1e-4 removes the wall acne (39,219 -> 710 px) while leaving the
// floor leak at its best value.
const GRAD_MAX: f32 = 1.0e-4;

@group(2) @binding(0)
var t_shadow: texture_depth_2d_array;
@group(2) @binding(1)
var s_shadow: sampler_comparison;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Indexed meshes share vertices, so vertex_index is not a triangle
    // corner and the barycentrics above are meaningless -- draw them shaded
    // rather than covered in noise. The CPU side warns once when this hits.
    let can_wireframe = (in.flags & 1u) != 0u;

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
        let edge = wireframe_edge(in.bary);
        return vec4<f32>(mix(shaded.rgb, globals.wireframe_color, edge), shaded.a);
    }

    return shaded;
}

fn fs_shaded(in: VertexOutput) -> vec4<f32> {
    // `value_mode` is orthogonal to `color_mode`: it decides *what* the
    // surface colour is, while `color_mode` decides whether that colour is
    // lit. So a data map can be shaded (0) or flat (1).
    let surface_color = select(
        in.color,
        colormap_lookup(in.value),
        globals.value_mode == 1u,
    );

    if globals.color_mode == 1 {
        var color = surface_color;
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
    // object_color = textureSample(t_diffuse, s_diffuse, in.tex);

    // 0 or else
    //
    // else {
    let object_color = vec4<f32>(surface_color, 1.0);

    let light_dir = normalize(view.light.pos - in.world_pos);
    let ndotl = max(dot(in.world_normal, light_dir), 0.0);
    let k = 1.0 - ndotl;
    let k2 = k * k;

    // shadow: this body's own layer, with that layer's bias
    let layer = min(in.shadow_layer, max(view.light.n_layers, 1u) - 1u);
    let lb = view.light.layer_bias[layer];
    // Scaled by the PCF kernel radius, not one texel. `lb.x` is one texel
    // diagonal, which is the right separation for a single tap -- but a
    // `shadow_pcf = N` kernel reaches N texels away, and every one of those
    // taps compares against a stored depth that far along the surface. With a
    // one-texel offset those taps flip, and averaging turns a black interior
    // into grey: 7,952 px of the crater floor lifted out of full shadow at
    // N = 4, against 494 with the offset scaled to the kernel.
    //
    // `N = 0` keeps exactly the previous value, so the single-tap path -- and
    // every product rendered with it -- is unchanged.
    let normal_offset = lb.x * (1.0 + f32(globals.shadow_pcf)) * k;
    let offset_pos = in.world_pos + in.world_normal * normal_offset;
    let light_space = view.light.view_proj_layers[layer] * vec4<f32>(offset_pos, 1.0);
    var proj = light_space.xyz / light_space.w;
    proj.y = -proj.y;
    let uv = proj.xy * 0.5 + 0.5;
    let depth = proj.z;
    let bias = max(lb.y * k2, lb.z);

    let grad = receiver_plane_grad(
        view.light.view_proj_layers[layer], offset_pos, in.world_normal);

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
                let adj = clamp(dot(offset, grad), -GRAD_MAX, GRAD_MAX);
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
