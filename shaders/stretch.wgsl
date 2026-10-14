// A stretch (`app::stretch`): the image's values shown as SAO DS9 scales an
// image, each taken from between two limits -- black at the low one, white at
// the high one -- through a function. The limits come from the image each
// frame: its least and greatest values and a histogram of them
// (`cs_gather`), the percentiles read from it or IRAF's zscale fitted to 1000
// of the values, sorted (`cs_limits`); for histogram equalisation a second
// histogram, between the limits (`cs_equalize`, `cs_cdf`). A value is a
// pixel's largest channel, as the image stores it.

struct Params {
    // The image's size.
    width: u32,
    height: u32,
    // 1 where the image stores values (`Config::stores_values`): the frame's
    // floats are what an sRGB target stores decoded -- the values encoded
    // from them -- and what is shown is written decoded for the target to
    // store it.
    srgb_mode: u32,
    // `StretchScale::code`: linear, log, pow, sqrt, squared, asinh, sinh,
    // histequ.
    scale: u32,
    // `StretchLimits::code`: user, minmax, percent, zscale, zmax.
    limits: u32,
    // zscale's samples: how many at most, and the pixels from one to the
    // next along the rows.
    samples: u32,
    stride: u32,
    _pad0: u32,
    percent: f32,
    low: f32,
    high: f32,
    exponent: f32,
    contrast: f32,
    _pad1: f32,
    _pad2: f32,
    _pad3: f32,
};

// The limits found, and what equalisation shows each value as.
struct Result {
    low: f32,
    high: f32,
    // The image's least and greatest values.
    least: f32,
    greatest: f32,
    // Equalisation: at each of the `EQ_BINS + 1` edges between the limits,
    // the share of the values between them that lie below.
    cdf: array<f32, 2049>,
};

@group(0) @binding(0) var<uniform> params: Params;
// The image's floats, as the main pass draws them.
@group(0) @binding(1) var frame: texture_2d<f32>;
// [0] the least value's bits complemented, which `atomicMax` keeps; [1] the
// greatest's bits; [2] the values counted; [3] unused; then the histogram's
// `LOG_BINS` bins (`log_bin`), then equalisation's `EQ_BINS`.
@group(0) @binding(2) var<storage, read_write> counts: array<atomic<u32>>;
@group(0) @binding(3) var<storage, read_write> result: Result;

// Zero, then 32 an octave from 2^-40 to 2^24, the bins at each end taking
// what is beyond.
const LOG_BINS: u32 = 2049u;
const EQ_BINS: u32 = 2048u;
// A workgroup's square of pixels, `cs_gather` and `cs_equalize`.
const TILE: u32 = 64u;
// zscale's sort: its samples, padded.
const SORT: u32 = 1024u;

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

/// A pixel's channels as the image stores them.
fn stored(p: vec2<u32>) -> vec4<f32> {
    let c = textureLoad(frame, vec2<i32>(p), 0);
    if params.srgb_mode == 1u {
        return vec4<f32>(encode(c.rgb), c.a);
    }
    return c;
}

/// A pixel's value: its largest channel, as the image stores it, 0 for
/// anything below; -1 for none, a channel not a number or infinite. Tested
/// on the bits, which fast maths cannot fold away.
fn value_at(p: vec2<u32>) -> f32 {
    let c = stored(p).rgb;
    let v = max(max(c.r, c.g), c.b);
    if (bitcast<u32>(v) & 0x7f800000u) == 0x7f800000u {
        return -1.0;
    }
    return max(v, 0.0);
}

/// The histogram's bin of a value: 0 for zero, else from its exponent and
/// first five bits of mantissa, 32 an octave from 2^-40.
fn log_bin(v: f32) -> u32 {
    if v <= 0.0 {
        return 0u;
    }
    let bits = bitcast<u32>(v);
    let k = (i32((bits >> 23u) & 0xffu) - 87) * 32 + i32((bits >> 18u) & 31u);
    return 1u + u32(clamp(k, 0, i32(LOG_BINS) - 2));
}

/// The lower edge of bin `b` above 0; `LOG_BINS` gives the last one's upper
/// edge, 2^24.
fn bin_edge(b: u32) -> f32 {
    let k = b - 1u;
    return exp2(f32(i32(k / 32u) - 40)) * (1.0 + f32(k % 32u) / 32.0);
}

var<workgroup> tile_bins: array<atomic<u32>, 2049>;
var<workgroup> tile_least: atomic<u32>;
var<workgroup> tile_greatest: atomic<u32>;
var<workgroup> tile_count: atomic<u32>;

/// A tile's values counted into the histogram, their least and greatest
/// kept: in the workgroup's memory first -- a sky at 0 is a million adds to
/// one bin -- then each bin it filled added to the image's.
@compute @workgroup_size(16, 16)
fn cs_gather(
    @builtin(workgroup_id) group: vec3<u32>,
    @builtin(local_invocation_id) local: vec3<u32>,
    @builtin(local_invocation_index) li: u32,
) {
    for (var k = li; k < LOG_BINS; k += 256u) {
        atomicStore(&tile_bins[k], 0u);
    }
    if li == 0u {
        atomicStore(&tile_least, 0u);
        atomicStore(&tile_greatest, 0u);
        atomicStore(&tile_count, 0u);
    }
    workgroupBarrier();
    var least = 0u;
    var greatest = 0u;
    var n = 0u;
    for (var j = 0u; j < TILE; j += 16u) {
        for (var i = 0u; i < TILE; i += 16u) {
            let p = group.xy * TILE + local.xy + vec2<u32>(i, j);
            if p.x < params.width && p.y < params.height {
                let v = value_at(p);
                if v >= 0.0 {
                    let bits = bitcast<u32>(v);
                    least = max(least, ~bits);
                    greatest = max(greatest, bits);
                    n += 1u;
                    atomicAdd(&tile_bins[log_bin(v)], 1u);
                }
            }
        }
    }
    atomicMax(&tile_least, least);
    atomicMax(&tile_greatest, greatest);
    atomicAdd(&tile_count, n);
    workgroupBarrier();
    for (var k = li; k < LOG_BINS; k += 256u) {
        let c = atomicLoad(&tile_bins[k]);
        if c > 0u {
            atomicAdd(&counts[4u + k], c);
        }
    }
    if li == 0u {
        atomicMax(&counts[0], atomicLoad(&tile_least));
        atomicMax(&counts[1], atomicLoad(&tile_greatest));
        atomicAdd(&counts[2], atomicLoad(&tile_count));
    }
}

var<workgroup> sorted: array<f32, 1024>;
// A thread's part of a sum or a running count (`sum4`, `running`).
var<workgroup> partial: array<vec4<f32>, 256>;
// zscale's samples cut, a bit each, and the cut grown.
var<workgroup> cut_bits: array<atomic<u32>, 32>;
var<workgroup> grown_bits: array<atomic<u32>, 32>;
// The percentiles, set by the threads whose bins hold them.
var<workgroup> found: vec2<f32>;

/// The sum of every thread's `v`, the same to all: called by all of them
/// alike, as a barrier is.
fn sum4(li: u32, v: vec4<f32>) -> vec4<f32> {
    partial[li] = v;
    workgroupBarrier();
    for (var step = 128u; step > 0u; step >>= 1u) {
        if li < step {
            partial[li] += partial[li + step];
        }
        workgroupBarrier();
    }
    let total = workgroupUniformLoad(&partial[0]);
    workgroupBarrier();
    return total;
}

/// Each thread's count `c` added to those of the threads before it: the
/// count through its own part. Called as `sum4` is.
fn running(li: u32, c: f32) -> f32 {
    partial[li] = vec4<f32>(c, 0.0, 0.0, 0.0);
    workgroupBarrier();
    for (var offset = 1u; offset < 256u; offset <<= 1u) {
        var v = 0.0;
        if li >= offset {
            v = partial[li - offset].x;
        }
        workgroupBarrier();
        partial[li].x += v;
        workgroupBarrier();
    }
    return partial[li].x;
}

/// The values under which `wanted.x` and `wanted.y` of them lie: each
/// thread's nine bins counted, the counts run through, and the thread
/// whose bins hold one finds the bin where its running count passes it,
/// linearly across the bin. `fallback` where none does.
fn quantiles(li: u32, wanted: vec2<f32>, fallback: vec2<f32>) -> vec2<f32> {
    if li == 0u {
        found = fallback;
    }
    let first = li * 9u;
    let end = min(first + 9u, LOG_BINS);
    var c = 0.0;
    for (var b = first; b < end; b++) {
        c += f32(atomicLoad(&counts[4u + b]));
    }
    let through = running(li, c);
    for (var q = 0u; q < 2u; q++) {
        let w = wanted[q];
        var seen = through - c;
        if seen < w && w <= through {
            for (var b = first; b < end; b++) {
                let cb = f32(atomicLoad(&counts[4u + b]));
                if cb > 0.0 && seen + cb >= w {
                    var v = 0.0;
                    if b > 0u {
                        v = mix(bin_edge(b), bin_edge(b + 1u), clamp((w - seen) / cb, 0.0, 1.0));
                    }
                    found[q] = v;
                    break;
                }
                seen += cb;
            }
        }
    }
    return workgroupUniformLoad(&found);
}

fn is_cut(k: u32) -> bool {
    return (atomicLoad(&cut_bits[k >> 5u]) & (1u << (k & 31u))) != 0u;
}

/// IRAF's zscale on the `npix` sorted samples, as astropy's
/// `ZScaleInterval` has it: a line fitted to them against their rank, the
/// samples further from it than 2.5 standard deviations cut and the cut
/// grown by a hundredth of them, five times at most or until nothing more
/// is cut; its slope over the contrast, about the median, the limits --
/// within the samples' own. The ranks scaled to -1..1 for float32's sake;
/// every sum shared among the threads. Called as `sum4` is.
fn zscale(li: u32, npix: u32) -> vec2<f32> {
    var low = sorted[0];
    var high = sorted[npix - 1u];
    if npix < 5u {
        return vec2<f32>(low, high);
    }
    let minpix = max(5u, u32(f32(npix) * 0.5));
    let ngrow = max(1u, u32(f32(npix) * 0.01));
    // Grown as numpy's convolution with `ngrow` ones, mode "same", has it:
    // sample i cut where any of i - ngrow/2 to i + (ngrow - 1)/2 is.
    let behind = ngrow / 2u;
    let ahead = (ngrow - 1u) / 2u;
    let xscale = 2.0 / f32(npix - 1u);
    if li < 32u {
        atomicStore(&cut_bits[li], 0u);
    }
    workgroupBarrier();
    var ngood = npix;
    var last = npix + 1u;
    var slope = 0.0;
    for (var iteration = 0u; iteration < 5u; iteration++) {
        if ngood >= last || ngood < minpix {
            break;
        }
        // The line through the samples kept, by least squares.
        var acc = vec4<f32>(0.0);
        var axy = 0.0;
        for (var k = li; k < npix; k += 256u) {
            if !is_cut(k) {
                let x = f32(k) * xscale - 1.0;
                let y = sorted[k];
                acc += vec4<f32>(1.0, x, y, x * x);
                axy += x * y;
            }
        }
        let t = sum4(li, acc);
        let sxy = sum4(li, vec4<f32>(axy, 0.0, 0.0, 0.0)).x;
        let delta = t.x * t.w - t.y * t.y;
        let b = (t.x * sxy - t.y * t.z) / delta;
        let a = (t.w * t.z - t.y * sxy) / delta;
        slope = b * xscale;
        // The residuals' standard deviation over them.
        var r = vec4<f32>(0.0);
        for (var k = li; k < npix; k += 256u) {
            if !is_cut(k) {
                let f = sorted[k] - (a + b * (f32(k) * xscale - 1.0));
                r += vec4<f32>(f, f * f, 0.0, 0.0);
            }
        }
        let rr = sum4(li, r);
        let mean = rr.x / t.x;
        let threshold = 2.5 * sqrt(max(rr.y / t.x - mean * mean, 0.0));
        for (var k = li; k < npix; k += 256u) {
            let f = sorted[k] - (a + b * (f32(k) * xscale - 1.0));
            if f < -threshold || f > threshold {
                atomicOr(&cut_bits[k >> 5u], 1u << (k & 31u));
            }
        }
        if li < 32u {
            atomicStore(&grown_bits[li], 0u);
        }
        workgroupBarrier();
        for (var i = li; i < npix; i += 256u) {
            let start = select(0u, i - behind, i >= behind);
            let stop = min(i + ahead, npix - 1u);
            for (var j = start; j <= stop; j++) {
                if is_cut(j) {
                    atomicOr(&grown_bits[i >> 5u], 1u << (i & 31u));
                    break;
                }
            }
        }
        workgroupBarrier();
        if li < 32u {
            atomicStore(&cut_bits[li], atomicLoad(&grown_bits[li]));
        }
        workgroupBarrier();
        var good = 0.0;
        for (var k = li; k < npix; k += 256u) {
            if !is_cut(k) {
                good += 1.0;
            }
        }
        last = ngood;
        ngood = u32(sum4(li, vec4<f32>(good, 0.0, 0.0, 0.0)).x + 0.5);
    }
    if ngood >= minpix {
        if params.contrast > 0.0 {
            slope = slope / params.contrast;
        }
        let center = (npix - 1u) / 2u;
        var median = sorted[center];
        if npix % 2u == 0u {
            median = 0.5 * (sorted[center] + sorted[center + 1u]);
        }
        low = max(low, median - (f32(center) - 1.0) * slope);
        high = min(high, median + f32(npix - center) * slope);
    }
    return vec2<f32>(low, high);
}

/// The limits, by `params.limits`, every thread taking part: the
/// percentiles from the histogram, or zscale on its samples, sorted by a
/// bitonic sort. What decides a barrier is the uniform's alone.
@compute @workgroup_size(256)
fn cs_limits(@builtin(local_invocation_index) li: u32) {
    let n = atomicLoad(&counts[2]);
    let least = bitcast<f32>(~atomicLoad(&counts[0]));
    let greatest = bitcast<f32>(atomicLoad(&counts[1]));
    var percentiles = vec2<f32>(least, greatest);
    if params.limits == 2u && params.percent < 100.0 {
        let cut = 0.5 * (1.0 - max(params.percent, 0.0) / 100.0) * f32(n);
        percentiles = quantiles(li, vec2<f32>(cut, f32(n) - cut), percentiles);
    }
    var z = vec2<f32>(0.0);
    if params.limits == 3u || params.limits == 4u {
        let size = params.width * params.height;
        let npix = min(params.samples, (size + params.stride - 1u) / params.stride);
        for (var k = li; k < SORT; k += 256u) {
            var v = 3.0e38;
            if k < npix {
                let at = k * params.stride;
                v = max(value_at(vec2<u32>(at % params.width, at / params.width)), 0.0);
            }
            sorted[k] = v;
        }
        workgroupBarrier();
        for (var block = 2u; block <= SORT; block <<= 1u) {
            for (var j = block >> 1u; j > 0u; j >>= 1u) {
                for (var t = li; t < SORT / 2u; t += 256u) {
                    let a = 2u * t - (t & (j - 1u));
                    let b = a + j;
                    let up = (a & block) == 0u;
                    let x = sorted[a];
                    let y = sorted[b];
                    if (x > y) == up {
                        sorted[a] = y;
                        sorted[b] = x;
                    }
                }
                workgroupBarrier();
            }
        }
        z = zscale(li, npix);
    }
    if li != 0u {
        return;
    }
    var low = params.low;
    var high = params.high;
    if n == 0u {
        low = 0.0;
        high = 1.0;
    } else if params.limits == 1u || params.limits == 2u {
        low = clamp(percentiles.x, least, greatest);
        high = clamp(percentiles.y, least, greatest);
    } else if params.limits == 3u || params.limits == 4u {
        low = z.x;
        high = select(z.y, greatest, params.limits == 4u);
    }
    result.low = low;
    result.high = high;
    result.least = select(least, 0.0, n == 0u);
    result.greatest = select(greatest, 0.0, n == 0u);
}

var<workgroup> tile_eq: array<atomic<u32>, 2048>;

/// For equalisation: a tile's values between the limits counted into
/// `EQ_BINS` bins of equal width, then added to the image's.
@compute @workgroup_size(16, 16)
fn cs_equalize(
    @builtin(workgroup_id) group: vec3<u32>,
    @builtin(local_invocation_id) local: vec3<u32>,
    @builtin(local_invocation_index) li: u32,
) {
    for (var k = li; k < EQ_BINS; k += 256u) {
        atomicStore(&tile_eq[k], 0u);
    }
    workgroupBarrier();
    let low = result.low;
    let span = result.high - low;
    for (var j = 0u; j < TILE; j += 16u) {
        for (var i = 0u; i < TILE; i += 16u) {
            let p = group.xy * TILE + local.xy + vec2<u32>(i, j);
            if p.x < params.width && p.y < params.height && span > 0.0 {
                let v = value_at(p);
                if v >= low && v <= low + span {
                    let k = min(u32((v - low) / span * f32(EQ_BINS)), EQ_BINS - 1u);
                    atomicAdd(&tile_eq[k], 1u);
                }
            }
        }
    }
    workgroupBarrier();
    for (var k = li; k < EQ_BINS; k += 256u) {
        let c = atomicLoad(&tile_eq[k]);
        if c > 0u {
            atomicAdd(&counts[4u + LOG_BINS + k], c);
        }
    }
}

/// Equalisation's running shares at the bins' edges: eight bins a thread,
/// their counts run through.
@compute @workgroup_size(256)
fn cs_cdf(@builtin(local_invocation_index) li: u32) {
    let first = li * 8u;
    var c = 0.0;
    for (var k = first; k < first + 8u; k++) {
        c += f32(atomicLoad(&counts[4u + LOG_BINS + k]));
    }
    let through = running(li, c);
    let total = workgroupUniformLoad(&partial[255]).x;
    if li == 0u {
        result.cdf[0] = 0.0;
    }
    var seen = through - c;
    for (var k = first; k < first + 8u; k++) {
        seen += f32(atomicLoad(&counts[4u + LOG_BINS + k]));
        // Nothing between the limits: linear.
        result.cdf[k + 1u] = select(f32(k + 1u) / f32(EQ_BINS), seen / total, total > 0.0);
    }
}

/// The function, of `x` from 0 to 1.
fn stretched(x: vec3<f32>) -> vec3<f32> {
    let a = params.exponent;
    var y = x;
    switch params.scale {
        case 1u: {
            if a > 0.0 {
                y = log(a * x + 1.0) / log(a + 1.0);
            }
        }
        case 2u: {
            if a > 0.0 && abs(a - 1.0) >= 1e-6 {
                y = (pow(vec3<f32>(a), x) - 1.0) / (a - 1.0);
            }
        }
        case 3u: {
            y = sqrt(x);
        }
        case 4u: {
            y = x * x;
        }
        case 5u: {
            y = asinh(10.0 * x) / asinh(10.0);
        }
        case 6u: {
            y = sinh(3.0 * x) / sinh(3.0);
        }
        case 7u: {
            for (var c = 0u; c < 3u; c++) {
                let at = x[c] * f32(EQ_BINS);
                let k = min(u32(at), EQ_BINS - 1u);
                y[c] = mix(result.cdf[k], result.cdf[k + 1u], clamp(at - f32(k), 0.0, 1.0));
            }
        }
        default: {}
    }
    return y;
}

@vertex
fn vs_show(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    // One triangle over the whole target.
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

/// The image through the stretch into the target, as a frame drawn directly
/// would store it: decoded where the frame's floats are.
@fragment
fn fs_show(@builtin(position) at_px: vec4<f32>) -> @location(0) vec4<f32> {
    let c = stored(vec2<u32>(at_px.xy));
    let low = result.low;
    let span = max(result.high - low, 1e-30);
    var y = clamp(stretched(clamp((c.rgb - low) / span, vec3<f32>(0.0), vec3<f32>(1.0))), vec3<f32>(0.0), vec3<f32>(1.0));
    if params.srgb_mode == 1u {
        y = decode(y);
    }
    return vec4<f32>(y, clamp(c.a, 0.0, 1.0));
}
