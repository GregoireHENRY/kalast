// The colour scale, drawn as one screen-space quad.
//
// The point of this shader is that it produces colour the *same way the
// surface does*. A bar drawn as a naive gradient would be wrong here: the
// surface is written into an sRGB target, so a linear ramp lands on screen as
// `value^(1/2.2)` and mid-grey reads as 0.216 of the quantity, not 0.5. By
// running the same lookup and the same transfer as `mesh_shadow.wgsl`, the bar
// cannot disagree with what it labels.

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
    wireframe_mode: u32,
    wireframe_width: f32,
    wireframe_color: vec3<f32>,
    value_mode: u32,
    value_min: f32,
    value_max: f32,
};
@group(0) @binding(0)
var<uniform> globals: Globals;

struct Colormap {
    lut: array<vec4<f32>, 256>,
};
@group(3) @binding(0)
var<uniform> colormap: Colormap;

struct Bar {
    // Rectangle in normalised device coordinates: xy = min corner, zw = size.
    rect: vec4<f32>,
    // 1 when the long axis is vertical.
    vertical: u32,
    // 0 = the value colormap, 1 = the diffuse shading.
    source: u32,
    _pad: vec2<u32>,
};
@group(4) @binding(0)
var<uniform> bar: Bar;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    // 0 at the low end of the scale, 1 at the high end.
    @location(0) t: f32,
};

fn srgb_to_linear(color: vec3<f32>, gamma: f32) -> vec3<f32> {
    return pow(color, vec3<f32>(gamma));
}

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VertexOutput {
    // Two triangles, no vertex buffer: the quad is known from its index.
    var corner = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    let c = corner[i];

    var out: VertexOutput;
    out.clip_position = vec4<f32>(bar.rect.xy + c * bar.rect.zw, 0.0, 1.0);
    // NDC y is up, so a vertical bar already runs low-to-high bottom-to-top.
    out.t = select(c.x, c.y, bar.vertical == 1u);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let t = clamp(in.t, 0.0, 1.0);

    if bar.source == 1u {
        // Exactly what the shaded surface computes for a white facet at
        // `cos(i) * visibility = t`, including the ambient floor -- so the
        // bar's grey at t matches the body's grey where the geometry gives t.
        var color = vec3<f32>(globals.ambient_strength + t);
        if globals.srgb_mode == 1u {
            color = srgb_to_linear(color, globals.gamma);
        }
        return vec4<f32>(color, 1.0);
    }

    // Same lookup and interpolation as the surface.
    let x = t * 255.0;
    let i = u32(floor(x));
    let j = min(i + 1u, 255u);
    var color = mix(colormap.lut[i].rgb, colormap.lut[j].rgb, x - f32(i));

    // The flat path in mesh_shadow.wgsl converts when srgb_mode is 0; match it,
    // or the bar and a flat data map sit at different brightnesses.
    if globals.srgb_mode == 0u {
        color = srgb_to_linear(color, globals.gamma);
    }
    return vec4<f32>(color, 1.0);
}
