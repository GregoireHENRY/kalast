// `hud.antialias` off: the text drawn on a layer of its own, copied over with
// a hard edge -- a pixel is text where the glyph covers half of it or more,
// in the text's own colour, and untouched elsewhere.

@group(0) @binding(0)
var layer: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    // One triangle over the whole target.
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    // Blended over transparent black, the layer holds the colour times the
    // glyph's coverage, and the coverage: undone, and cut at a half.
    let t = textureLoad(layer, vec2<i32>(at.xy), 0);
    if t.a < 0.5 {
        discard;
    }
    return vec4<f32>(t.rgb / t.a, 1.0);
}
