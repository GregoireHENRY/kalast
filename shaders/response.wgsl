// A camera's response (`app::response`): the image convolved with its point
// spread and smear by FFT. Each element of the transformed arrays holds two
// complex numbers, `xy` and `zw`: red and green, blue and alpha. The kernel
// is real, so each pair's convolution is the convolution of each part.

struct Params {
    // The image's size.
    width: u32,
    height: u32,
    // The transform's, powers of two, at least twice the image's.
    nx: u32,
    ny: u32,
    // A stage's sub-transform length (`cs_fft`), 1 to half the line.
    p: u32,
    // 0: along rows, 1: along columns.
    axis: u32,
    // 1: the inverse transform's twiddles.
    inverse: u32,
    // `shading.srgb_mode`: 1, the image's floats are what an sRGB target
    // stores decoded, the values the image stores taken by encoding them.
    srgb_mode: u32,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dst: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> spectrum: array<vec2<f32>>;
@group(0) @binding(4) var frame: texture_2d<f32>;

const PI: f32 = 3.14159265358979;

fn encode(c: vec3<f32>) -> vec3<f32> {
    let low = c * 12.92;
    let high = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(high, low, c <= vec3<f32>(0.0031308));
}

fn decode(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((max(c, vec3<f32>(0.0)) + 0.055) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

fn cmul(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(a.x * b.x - a.y * b.y, a.x * b.y + a.y * b.x);
}

/// The image's floats into the transform's first array, the values the image
/// stores, zero beyond the image.
@compute @workgroup_size(8, 8)
fn cs_load(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.nx || id.y >= params.ny {
        return;
    }
    var v = vec4<f32>(0.0);
    if id.x < params.width && id.y < params.height {
        let c = textureLoad(frame, vec2<i32>(id.xy), 0);
        var rgb = c.rgb;
        if params.srgb_mode == 1u {
            rgb = encode(rgb);
        }
        v = vec4<f32>(rgb, c.a);
    }
    dst[id.y * params.nx + id.x] = v;
}

/// Element `pos` of line `line`: a row, or a column.
fn at(line: u32, pos: u32) -> u32 {
    if params.axis == 0u {
        return line * params.nx + pos;
    }
    return pos * params.nx + line;
}

/// One stage of a radix-2 Stockham transform along every line at once, `src`
/// to `dst`: sub-transforms of length `p` merged into ones of `2p`, in order,
/// no bit reversal (Bainville's form). Neighbouring threads take neighbouring
/// elements: along a row, the butterflies; down the columns, the columns.
/// A column's butterflies taken by neighbours, a row apart, made the whole
/// response six times slower: 167 ms against 27 at 2048 x 2048 on an M1 Pro.
@compute @workgroup_size(64)
fn cs_fft(@builtin(global_invocation_id) id: vec3<u32>) {
    let n = select(params.nx, params.ny, params.axis == 1u);
    let lines = select(params.ny, params.nx, params.axis == 1u);
    let half = n / 2u;
    let i = select(id.x, id.y, params.axis == 1u);
    let line = select(id.y, id.x, params.axis == 1u);
    if i >= half || line >= lines {
        return;
    }
    let p = params.p;
    let k = i & (p - 1u);
    let u0 = src[at(line, i)];
    let u1 = src[at(line, i + half)];
    let angle = select(-1.0, 1.0, params.inverse == 1u) * PI * f32(k) / f32(p);
    let w = vec2<f32>(cos(angle), sin(angle));
    let v = vec4<f32>(cmul(u1.xy, w), cmul(u1.zw, w));
    let j = (i << 1u) - k;
    dst[at(line, j)] = u0 + v;
    dst[at(line, j + p)] = u0 - v;
}

/// The image's spectrum times the kernel's.
@compute @workgroup_size(8, 8)
fn cs_multiply(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.nx || id.y >= params.ny {
        return;
    }
    let e = id.y * params.nx + id.x;
    let s = spectrum[e];
    let u = src[e];
    dst[e] = vec4<f32>(cmul(u.xy, s), cmul(u.zw, s));
}

/// The kernel's spectrum: the point spread's, in `xy`, times the smear's, in
/// `zw`, both transformed together.
@compute @workgroup_size(8, 8)
fn cs_kernel(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.nx || id.y >= params.ny {
        return;
    }
    let e = id.y * params.nx + id.x;
    let u = src[e];
    spectrum[e] = cmul(u.xy, u.zw);
}

@vertex
fn vs_show(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    // One triangle over the whole target.
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

/// The convolved image into the target, as a frame drawn directly would be:
/// decoded where its floats are the values the image stores.
@fragment
fn fs_show(@builtin(position) at_px: vec4<f32>) -> @location(0) vec4<f32> {
    let v = src[u32(at_px.y) * params.nx + u32(at_px.x)] / (f32(params.nx) * f32(params.ny));
    var rgb = max(v.xyz, vec3<f32>(0.0));
    if params.srgb_mode == 1u {
        rgb = decode(rgb);
    }
    return vec4<f32>(rgb, clamp(v.w, 0.0, 1.0));
}
