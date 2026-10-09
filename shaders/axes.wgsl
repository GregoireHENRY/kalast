// Reference axes: world-space line segments, coloured per vertex.
//
// No lighting and no shadow: these are annotation, not geometry, and a tick
// that dims as the Sun moves would be unreadable half the time.
//
// Each segment is an instance of a four-vertex strip, widened on screen to
// the line and a pixel past it either side, where the fragment blends the
// line's edge in -- or cuts it, `axes.antialias` off. Their own antialiasing,
// whatever the main pass's MSAA: the strip's own edges are transparent.

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
    wireframe_fade: u32,
    value_min: f32,
    value_max: f32,
    wireframe_antialias: u32,
    axes_antialias: u32,
    image_size: vec2<f32>,
};
@group(0) @binding(0)
var<uniform> globals: Globals;

struct Camera {
    view_proj: mat4x4<f32>,
};
struct Light {
    view_proj: mat4x4<f32>,
    view_proj_layers: array<mat4x4<f32>, 8>,
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

struct Segment {
    @location(0) p0: vec3<f32>,
    @location(1) c0: vec3<f32>,
    @location(2) p1: vec3<f32>,
    @location(3) c1: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    // Distance from the line's middle across it, in pixels, signed. Linear on
    // screen, not perspective-correct: the widening is on screen.
    @location(1) @interpolate(linear) across: f32,
};

// The one-pixel line the axes always were.
const HALF_WIDTH: f32 = 0.5;
// A pixel past the line on each side, for the blend to fade out in.
const MARGIN: f32 = 1.0;
// An end behind a perspective eye would project mirrored into the frame: it
// is brought along the segment to this far in front of it first.
const NEAR_W: f32 = 1e-5;

@vertex
fn vs_main(@builtin(vertex_index) i: u32, s: Segment) -> VertexOutput {
    var out: VertexOutput;
    var a = view.camera.view_proj * vec4<f32>(s.p0, 1.0);
    var b = view.camera.view_proj * vec4<f32>(s.p1, 1.0);
    if a.w < NEAR_W && b.w < NEAR_W {
        // Wholly behind the eye: outside the clip volume, so nothing drawn.
        out.clip_position = vec4<f32>(2.0, 2.0, 2.0, 1.0);
        return out;
    }
    if a.w < NEAR_W {
        a = mix(a, b, (NEAR_W - a.w) / (b.w - a.w));
    } else if b.w < NEAR_W {
        b = mix(b, a, (NEAR_W - b.w) / (a.w - b.w));
    }

    // The segment on screen, in pixels, and the way across it.
    let half_image = globals.image_size * 0.5;
    var along = (b.xy / b.w - a.xy / a.w) * half_image;
    let len = length(along);
    along = select(vec2<f32>(1.0, 0.0), along / len, len > 1e-6);
    let normal = vec2<f32>(-along.y, along.x);

    // 0 and 1 at the first end, 2 and 3 at the second; even on one side,
    // odd on the other.
    let end = select(a, b, i >= 2u);
    let side = select(-1.0, 1.0, (i & 1u) == 1u);
    let extent = HALF_WIDTH + MARGIN;
    let offset = normal * side * extent / half_image;
    out.clip_position = end + vec4<f32>(offset * end.w, 0.0, 0.0);
    out.color = select(s.c0, s.c1, i >= 2u);
    out.across = side * extent;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let d = abs(in.across);
    var alpha = 1.0 - smoothstep(HALF_WIDTH - 0.5, HALF_WIDTH + 0.5, d);
    if globals.axes_antialias == 0u {
        // Cut where the blend is half: the line keeps its width.
        alpha = select(0.0, 1.0, d <= HALF_WIDTH);
    }
    if alpha <= 0.0 {
        discard;
    }
    return vec4<f32>(in.color, alpha);
}
