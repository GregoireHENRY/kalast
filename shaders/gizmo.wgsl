// The navigation gizmo, drawn as screen-space quads.
//
// Two triangles per shape with the shape cut out here from `uv`: a disc and a
// ring antialias themselves at any size that way, where a triangle fan shows
// its facets and needs a segment count chosen against the radius.
//
// No uniforms and no camera. The widget is a flat drawing of a rotated basis,
// laid out on the CPU in `src/app/gizmo.rs`, so everything it needs is
// already in the vertices.

struct VertexInput {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) disc: f32,
    @location(4) inner: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    // Flat: both are constant over a quad, and interpolating a shape switch
    // would give the middle of one triangle a shape that is neither.
    @location(2) @interpolate(flat) disc: f32,
    @location(3) @interpolate(flat) inner: f32,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    // 1.0 is the near plane under reversed-Z, so the widget always passes the
    // depth test. It writes no depth, so it cannot occlude the scene either
    // -- the same arrangement the colour scale uses.
    out.clip_position = vec4<f32>(in.pos, 1.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    out.disc = in.disc;
    out.inner = in.inner;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Signed distance to the edge of the shape, negative inside: radial for a
    // ball, across the thickness for a stem.
    var d: f32;
    if in.disc > 0.5 {
        d = length(in.uv) - 1.0;
    } else {
        d = abs(in.uv.y) - 1.0;
    }

    // One pixel of transition, taken from the derivative so it is a pixel
    // whatever the shape's size on screen. `fwidth` is the change across a
    // whole pixel and the band is two `w` wide, so this is half of it.
    let w = max(fwidth(d) * 0.5, 1e-6);
    var a = 1.0 - smoothstep(-w, w, d);

    // The hole, for the negative axes.
    if in.inner > 0.0 {
        let r = length(in.uv);
        let wi = max(fwidth(r) * 0.5, 1e-6);
        a = a * smoothstep(in.inner - wi, in.inner + wi, r);
    }

    return vec4<f32>(in.color.rgb, in.color.a * a);
}
