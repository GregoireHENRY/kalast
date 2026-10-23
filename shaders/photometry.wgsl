// Photometry's image (`app::photometry`): each pixel the I/F the Sun leaves
// on it, the body's law times the Sun it sees, -1 where no body is. Summed
// here, and shown.

@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> tiles: array<vec2<f32>>;

// The flux of what the camera sees: the image added up 16 x 16 pixels at a
// time, a pair a tile -- the tile's sum, and the share of it on the frame's
// edge, which says a lit body runs off the frame. The tiles are added in
// f64 on the CPU, so the sum does not depend on the image's size.

var<workgroup> sums: array<vec2<f32>, 256>;

@compute @workgroup_size(16, 16)
fn cs_sum(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(local_invocation_index) li: u32,
    @builtin(workgroup_id) wg: vec3<u32>,
    @builtin(num_workgroups) n: vec3<u32>,
) {
    let size = textureDimensions(image);
    var v = vec2<f32>(0.0);
    if gid.x < size.x && gid.y < size.y {
        // The background's -1 adds nothing.
        let f = max(textureLoad(image, vec2<i32>(gid.xy), 0).r, 0.0);
        let edge = gid.x == 0u || gid.y == 0u || gid.x + 1u == size.x || gid.y + 1u == size.y;
        v = vec2<f32>(f, select(0.0, f, edge));
    }
    sums[li] = v;
    workgroupBarrier();
    // Pairwise, so the tile's 256 add with 8 roundings each, not 255.
    for (var s = 128u; s > 0u; s = s >> 1u) {
        if li < s {
            sums[li] = sums[li] + sums[li + s];
        }
        workgroupBarrier();
    }
    if li == 0u {
        tiles[wg.y * n.x + wg.x] = sums[0];
    }
}

// The image shown: what the main pass would have stored for each pixel --
// the Sun's colour times the exposure times the pixel's I/F, encoded as
// `mesh_shadow.wgsl`'s `fs_shaded` encodes it -- and the background where no
// body is.

struct Show {
    // The Sun's colour times the exposure: `view.light.color`.
    light: vec3<f32>,
    // 1: the values stored, decoded for an sRGB target (`srgb_mode` "linear").
    srgb_mode: u32,
    background: vec4<f32>,
    // `shading.gamma`, 0 for the sRGB curve.
    gamma: f32,
    // Three floats, not a vec3, which would align to 16 and make this 64
    // bytes where `photometry::Show` is 48.
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};
@group(1) @binding(0) var<uniform> show: Show;

// `mesh_shadow.wgsl`'s.
fn srgb_to_linear(color: vec3<f32>, gamma: f32) -> vec3<f32> {
    if gamma > 0.0 {
        return pow(color, vec3<f32>(gamma));
    }
    let low = color / 12.92;
    let high = pow((max(color, vec3<f32>(0.0)) + 0.055) / 1.055, vec3<f32>(2.4));
    return select(high, low, color <= vec3<f32>(0.04045));
}

@vertex
fn vs_show(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_show(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let p = textureLoad(image, vec2<i32>(at.xy), 0).r;
    if p < 0.0 {
        return show.background;
    }
    var color = show.light * p;
    if show.srgb_mode == 1u {
        color = srgb_to_linear(color, show.gamma);
    }
    return vec4<f32>(color, 1.0);
}
