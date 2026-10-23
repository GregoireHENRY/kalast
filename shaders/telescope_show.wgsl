// A telescope's last frame shown in the window (`app::telescope`,
// `telescope.show`): its counts fitted to the image, rows as DS9 shows a
// FITS file's -- the first at the bottom -- and either written as they are,
// for the stretch to show, or from `low` to `high` linearly, black to white.

struct Show {
    // The frame's size, and the image's.
    frame: vec2<u32>,
    image: vec2<u32>,
    low: f32,
    high: f32,
    // 1: the counts as they are, into a float frame the stretch shows.
    raw: u32,
    _pad: u32,
};

@group(0) @binding(0) var<uniform> show: Show;
@group(0) @binding(1) var<storage, read> counts: array<f32>;

@vertex
fn vs_show(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

// The sRGB curve undone, so that an sRGB target stores `v` itself.
fn decoded(v: f32) -> f32 {
    if v <= 0.04045 {
        return v / 12.92;
    }
    return pow((v + 0.055) / 1.055, 2.4);
}

@fragment
fn fs_show(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let frame = vec2<f32>(show.frame);
    let image = vec2<f32>(show.image);
    // The frame fitted whole into the image, centred.
    let scale = min(image.x / frame.x, image.y / frame.y);
    let offset = 0.5 * (image - frame * scale);
    let p = (at.xy - offset) / scale;
    if p.x < 0.0 || p.y < 0.0 || p.x >= frame.x || p.y >= frame.y {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    // The image's rows run down, the frame's up.
    let x = u32(p.x);
    let y = show.frame.y - 1u - u32(p.y);
    let v = counts[y * show.frame.x + x];
    if show.raw == 1u {
        return vec4<f32>(v, v, v, 1.0);
    }
    let s = decoded(clamp((v - show.low) / max(show.high - show.low, 1e-6), 0.0, 1.0));
    return vec4<f32>(s, s, s, 1.0);
}
