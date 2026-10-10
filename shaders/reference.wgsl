// A reference image (`app::reference`): each frame a sample of every pixel
// -- the camera jittered within it, one ray to a point of the Sun's disc --
// added to one of four replicas' sums; shown as their mean; its error the
// spread of the four.

struct Reference {
    width: u32,
    height: u32,
    // The replica this frame adds to (`cs_accumulate`).
    replica: u32,
    // `shading.srgb_mode`: 1, the samples summed as the values the image
    // stores, as `msaa_resolve.wgsl` averages them; 0, as the shader wrote
    // them, linear, which the target encodes.
    srgb_mode: u32,
    // Each replica's samples so far.
    counts: vec4<u32>,
};

@group(0) @binding(0) var<uniform> reference: Reference;
// The four replicas' sums, one after the other, a vec4 a pixel.
@group(0) @binding(1) var<storage, read_write> sums: array<vec4<f32>>;
// This frame's sample of every pixel (`cs_accumulate`).
@group(0) @binding(2) var frame: texture_2d<f32>;
// How many pixels' standard error falls in each bin (`cs_error`): a third
// of an octave a bin, from 2^-24 up; the last, the largest error seen.
@group(0) @binding(3) var<storage, read_write> histogram: array<atomic<u32>, 97>;

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

fn at(replica: u32, x: u32, y: u32) -> u32 {
    return (replica * reference.height + y) * reference.width + x;
}

/// This frame's sample of each pixel into its replica's sum.
@compute @workgroup_size(8, 8)
fn cs_accumulate(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= reference.width || id.y >= reference.height {
        return;
    }
    let c = textureLoad(frame, vec2<i32>(id.xy), 0);
    var v = c.rgb;
    if reference.srgb_mode == 1u {
        v = encode(v);
    }
    let i = at(reference.replica, id.x, id.y);
    sums[i] = sums[i] + vec4<f32>(v, c.a);
}

/// Each pixel's mean: the replicas' means, averaged, each counting alike.
fn mean_of(x: u32, y: u32) -> vec4<f32> {
    var m = vec4<f32>(0.0);
    var r = 0.0;
    for (var k = 0u; k < 4u; k++) {
        let n = reference.counts[k];
        if n > 0u {
            m += sums[at(k, x, y)] / f32(n);
            r += 1.0;
        }
    }
    return m / max(r, 1.0);
}

@vertex
fn vs_resolve(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    // One triangle over the whole target.
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

/// The mean into the image, for the target to store as a frame drawn
/// directly would: decoded where the sum was of stored values.
@fragment
fn fs_resolve(@builtin(position) at_px: vec4<f32>) -> @location(0) vec4<f32> {
    let m = mean_of(u32(at_px.x), u32(at_px.y));
    var c = m.rgb;
    if reference.srgb_mode == 1u {
        c = decode(c);
    }
    return vec4<f32>(c, m.a);
}

/// Each pixel's standard error -- the spread of its four replicas' means,
/// over the square root of four, the largest of its channels -- into the
/// histogram.
@compute @workgroup_size(8, 8)
fn cs_error(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= reference.width || id.y >= reference.height {
        return;
    }
    var means: array<vec3<f32>, 4>;
    var m = vec3<f32>(0.0);
    for (var k = 0u; k < 4u; k++) {
        means[k] = sums[at(k, id.x, id.y)].rgb / f32(max(reference.counts[k], 1u));
        m += means[k];
    }
    m /= 4.0;
    var s2 = vec3<f32>(0.0);
    for (var k = 0u; k < 4u; k++) {
        let d = means[k] - m;
        s2 += d * d;
    }
    let se = sqrt(s2 / 3.0 / 4.0);
    let e = max(max(se.x, se.y), se.z);
    var bin = 0u;
    if e > 0.0 {
        bin = u32(clamp(floor((log2(e) + 24.0) * 3.0) + 1.0, 0.0, 95.0));
    }
    atomicAdd(&histogram[bin], 1u);
    atomicMax(&histogram[96], bitcast<u32>(e));
}
