// A telescope's frame, made from the rendered light (`app::telescope`): the
// target -- its light the photometry's sum of this frame, read here on the
// GPU -- and the field's stars spread by the seeing (Moffat), the sky and
// the dark current, the photon and read noise, the gain, the bias and
// saturation; as `telescope::expose` makes one on the CPU, from the same
// plan (`telescope::Plan`).

struct Frame {
    width: u32,
    height: u32,
    n_tiles: u32,
    seed: u32,
    // Electrons a pixel, on average: the sky's and the dark current's.
    background: f32,
    gain: f32,
    read_noise: f32,
    bias: f32,
    full_well: f32,
    // Moffat's alpha and beta, pixels.
    alpha: f32,
    beta: f32,
    // Bit 0 psf, 1 photon noise, 2 read noise, 3 saturation, 4 bias.
    flags: u32,
    // The target's electrons for a rendered flux of one: the photometry's
    // sum, `sum(I/F * pixel area) / pi`, is multiplied by it.
    target_scale: f32,
    // A photometry pixel's area over pi, the scene's units squared.
    area_over_pi: f32,
    _pad0: u32,
    _pad1: u32,
};

struct Source {
    x: f32,
    y: f32,
    // A star's electrons; for one of the target's points along its trail,
    // its share of the target's light.
    electrons: f32,
    // How far its stamp goes, pixels.
    reach: f32,
    // 1: the target's: `electrons` times the rendered light.
    rendered: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<storage, read> sources: array<Source>;
// Electrons, times 256, added from every source a pixel's light comes from.
@group(0) @binding(2) var<storage, read_write> acc: array<atomic<u32>>;
// The frame, ADU.
@group(0) @binding(3) var<storage, read_write> image: array<f32>;
// The photometry's tiles, and the rendered flux they add to.
@group(0) @binding(4) var<storage, read> tiles: array<vec2<f32>>;
@group(0) @binding(5) var<storage, read_write> light: array<f32>;

const FIXED: f32 = 256.0;
// No source adds more than this to a pixel: past the full well already,
// and the fixed-point sum keeps its room.
const MOST: f32 = 1.0e6;
const PI: f32 = 3.14159265358979;

var<workgroup> part: array<f32, 256>;

// The rendered flux: the photometry's tiles added up.
@compute @workgroup_size(256)
fn cs_light(@builtin(local_invocation_index) li: u32) {
    var s = 0.0;
    for (var i = li; i < frame.n_tiles; i += 256u) {
        s += tiles[i].x;
    }
    part[li] = s;
    workgroupBarrier();
    for (var k = 128u; k > 0u; k = k >> 1u) {
        if li < k {
            part[li] = part[li] + part[li + k];
        }
        workgroupBarrier();
    }
    if li == 0u {
        light[0] = part[0] * frame.area_over_pi;
    }
}

fn moffat(r2: f32) -> f32 {
    let a2 = frame.alpha * frame.alpha;
    return (frame.beta - 1.0) / (PI * a2) * pow(1.0 + r2 / a2, -frame.beta);
}

// A source spread over its stamp: a workgroup a source, its 256 threads
// over the stamp's pixels, each pixel integrated on a 5 x 5 grid.
@compute @workgroup_size(16, 16)
fn cs_splat(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    let s = sources[wg.x];
    var e = s.electrons;
    if s.rendered == 1u {
        e = e * light[0] * frame.target_scale;
    }
    if e <= 0.0 {
        return;
    }
    let w = i32(frame.width);
    let h = i32(frame.height);
    if (frame.flags & 1u) == 0u {
        // No seeing: all its light in the pixel it falls in.
        if lid.x == 0u && lid.y == 0u {
            let i = i32(round(s.x));
            let j = i32(round(s.y));
            if i >= 0 && j >= 0 && i < w && j < h {
                atomicAdd(&acc[u32(j * w + i)], u32(min(e, MOST) * FIXED + 0.5));
            }
        }
        return;
    }
    let x0 = max(i32(floor(s.x - s.reach)), 0);
    let x1 = min(i32(ceil(s.x + s.reach)), w - 1);
    let y0 = max(i32(floor(s.y - s.reach)), 0);
    let y1 = min(i32(ceil(s.y + s.reach)), h - 1);
    for (var j = y0 + i32(lid.y); j <= y1; j += 16) {
        for (var i = x0 + i32(lid.x); i <= x1; i += 16) {
            var sum = 0.0;
            for (var sj = 0; sj < 5; sj++) {
                let dy = f32(j) - 0.5 + (f32(sj) + 0.5) / 5.0 - s.y;
                for (var si = 0; si < 5; si++) {
                    let dx = f32(i) - 0.5 + (f32(si) + 0.5) / 5.0 - s.x;
                    sum += moffat(dx * dx + dy * dy);
                }
            }
            let value = min(e * sum / 25.0, MOST);
            if value > 0.0 {
                atomicAdd(&acc[u32(j * w + i)], u32(value * FIXED + 0.5));
            }
        }
    }
}

// A pixel's own random numbers: PCG's hash, seeded by the frame's seed and
// the pixel, stepped per draw.
fn pcg(v: u32) -> u32 {
    let state = v * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

// Uniform in (0, 1): not `uniform`, which WGSL keeps for an address space.
fn unit(state: ptr<function, u32>) -> f32 {
    *state = pcg(*state);
    return (f32(*state >> 8u) + 0.5) / 16777216.0;
}

fn normal(state: ptr<function, u32>) -> f32 {
    let u = unit(state);
    let v = unit(state);
    return sqrt(-2.0 * log(u)) * cos(2.0 * PI * v);
}

// ln k! by Stirling's series from 10, exactly below.
fn ln_factorial(k: f32) -> f32 {
    if k < 10.0 {
        var f = 1.0;
        for (var i = 2.0; i <= k; i += 1.0) {
            f *= i;
        }
        return log(f);
    }
    let x = k + 1.0;
    return (x - 0.5) * log(x) - x + 0.91893853 + 1.0 / (12.0 * x) - 1.0 / (360.0 * x * x * x);
}

// Poisson with mean `mu`: Knuth's product under 10, Hormann's transformed
// rejection (PTRS) to 1000, a rounded normal above, where Poisson's skew is
// a thirtieth or less and f32 no longer holds PTRS's sums.
fn poisson(state: ptr<function, u32>, mu: f32) -> f32 {
    if mu <= 0.0 {
        return 0.0;
    }
    if mu < 10.0 {
        let limit = exp(-mu);
        var k = 0.0;
        var p = unit(state);
        while p > limit {
            k += 1.0;
            p *= unit(state);
        }
        return k;
    }
    if mu >= 1000.0 {
        return max(round(mu + sqrt(mu) * normal(state)), 0.0);
    }
    let smu = sqrt(mu);
    let b = 0.931 + 2.53 * smu;
    let a = -0.059 + 0.02483 * b;
    let inv_alpha = 1.1239 + 1.1328 / (b - 3.4);
    let vr = 0.9277 - 3.6224 / (b - 2.0);
    let log_mu = log(mu);
    for (var tries = 0; tries < 64; tries++) {
        let u = unit(state) - 0.5;
        let v = unit(state);
        let us = 0.5 - abs(u);
        let k = floor((2.0 * a / us + b) * u + mu + 0.43);
        if us >= 0.07 && v <= vr {
            return k;
        }
        if k < 0.0 || (us < 0.013 && v > us) {
            continue;
        }
        if log(v) + log(inv_alpha) - log(a / (us * us) + b) <= -mu + k * log_mu - ln_factorial(k) {
            return k;
        }
    }
    return round(mu);
}

// What the camera reads: each pixel's electrons drawn, read, converted.
@compute @workgroup_size(16, 16)
fn cs_read(@builtin(global_invocation_id) gid: vec3<u32>) {
    if gid.x >= frame.width || gid.y >= frame.height {
        return;
    }
    let idx = gid.y * frame.width + gid.x;
    let mean = f32(atomicLoad(&acc[idx])) / FIXED + frame.background;
    var state = pcg(frame.seed ^ pcg(idx + 0x9e3779b9u));
    var e = mean;
    if (frame.flags & 2u) != 0u {
        e = poisson(&state, mean);
    }
    if (frame.flags & 8u) != 0u {
        e = min(e, frame.full_well);
    }
    if (frame.flags & 4u) != 0u {
        e += frame.read_noise * normal(&state);
    }
    var adu = e / frame.gain;
    if (frame.flags & 16u) != 0u {
        adu += frame.bias;
    }
    if (frame.flags & 8u) != 0u {
        adu = clamp(adu, 0.0, 65535.0);
    }
    image[idx] = adu;
}
