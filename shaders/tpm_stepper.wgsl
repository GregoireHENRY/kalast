// The thermal columns stepped by a scheme of one's choice: the GPU twin of
// `src/tpm/stepper.rs`, which says what the schemes are and why they solve
// for a step's changes. Read that first; this is the same arithmetic.
//
// One thread a facet. A column is a system of its own, so a thread goes up
// and down its column in loops, and no thread reads what another writes.
//
// The temperatures are `[facet][node]`, node 0 the surface and the last the
// base: a facet's column in one stretch of memory.
//
// f32 throughout. The schemes add a step's changes to the temperatures with
// what the sum cannot hold kept for the next step (`rest`), which only works
// if the compiler does not reorder the three operations of that sum: wgpu
// compiles without fast math, and `tests/test_tpm_gpu_stepper.py` checks
// that a deep layer does not drift.

struct Params {
    n_facets: u32,
    n_nodes: u32,
    // Width of the dispatch grid in threads, so that a facet's index is
    // rebuilt from a two-dimensional dispatch past 65,535 workgroups.
    stride: u32,
    // Where a node's properties are: `(facet * node_stride + node) * 3`.
    // 0 for properties one a layer, `n_nodes` for one a layer and a facet.
    node_stride: u32,
    // Where a facet's are: `facet * facet_stride * 2`, 0 or 1.
    facet_stride: u32,
    newton_max_iter: u32,
    // 1 where the heat capacity follows its polynomial.
    capacity_law: u32,
    // 0: `flux` is what each facet absorbs (W/m2). 1: its surface's
    // temperature (K), given.
    surface_given: u32,
    // What a call of `stage` does: its layers, its surface, its base, and
    // whether it starts from nothing (the first call of a step).
    do_layers: u32,
    do_surface: u32,
    do_base: u32,
    first: u32,
    // 1 where `implicit` keeps the temperatures of the step's start (BDF2).
    keep_before: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    newton_threshold: f32,
    inv_dz2: f32,
    twodz: f32,
    dz: f32,
    // The laws: `k = kc (1 + chi (T / 350)^3)`, `c = c0 + c1 T + ... + c4 T^4`.
    chi: f32,
    c0: f32,
    c1: f32,
    c2: f32,
    c3: f32,
    c4: f32,
    // The heat flow that enters the base from below (W/m2).
    bottom: f32,
    // The implicit system: see `implicit`.
    gamma: f32,
    c_rhs: f32,
    w_before: f32,
    // A stage of RKC: `out = mu z1 + nu z2 + w F(t + z1)`.
    mu: f32,
    nu: f32,
    w_stage: f32,
    _pad3: f32,
    _pad4: f32,
    _pad5: f32,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> t: array<f32>;
// What the steps so far could not add to each temperature.
@group(0) @binding(2) var<storage, read_write> rest: array<f32>;
// One a facet: the flux it absorbs, or its surface's temperature.
@group(0) @binding(3) var<storage, read_write> flux: array<f32>;
// Three a node: its conductivity `kc`, its density and its heat capacity.
@group(0) @binding(4) var<storage, read> props: array<f32>;
// Two a facet: `e sigma`, and `1 - albedo`.
@group(0) @binding(5) var<storage, read> facet_props: array<f32>;
// Three working arrays the size of the temperatures. `implicit`: `P`, `S`
// and the temperatures of the step before. `stage`: the changes of the
// stage before, of the one before that, and of this one; the three are
// handed round from a stage to the next.
@group(0) @binding(6) var<storage, read_write> w1: array<f32>;
@group(0) @binding(7) var<storage, read_write> w2: array<f32>;
@group(0) @binding(8) var<storage, read_write> w3: array<f32>;

fn facet_of(gid: vec3<u32>) -> u32 {
    return gid.y * params.stride + gid.x;
}

fn prop_at(f: u32, i: u32) -> u32 {
    return (f * params.node_stride + i) * 3u;
}

/// `k(T) / kc`.
fn phi(temp: f32) -> f32 {
    let x = temp / 350.0;
    return 1.0 + params.chi * x * x * x;
}

/// A node's conductivity at a temperature.
fn conductivity(f: u32, i: u32, temp: f32) -> f32 {
    return props[prop_at(f, i)] * phi(temp);
}

/// `1 / (rho c dz^2)` of a node at a temperature.
fn inverse_capacity(f: u32, i: u32, temp: f32) -> f32 {
    let at = prop_at(f, i);
    var c = props[at + 2u];
    if (params.capacity_law == 1u) {
        c = params.c0 + temp * (params.c1 + temp * (params.c2 + temp * (params.c3 + temp * params.c4)));
    }
    return params.inv_dz2 / (props[at + 1u] * c);
}

/// The conductivity at the boundary between two layers: their harmonic mean.
fn harmonic(a: f32, b: f32) -> f32 {
    return 2.0 * a * b / max(a + b, 1.1754944e-38);
}

/// Newton's method on `q - se x^4 + kq phi(x) (a x + b) = 0`, from `start`:
/// `core::newton_balance`.
fn balance(f: u32, start: f32, q: f32, a: f32, b: f32) -> f32 {
    let se = facet_props[f * params.facet_stride * 2u];
    let kq = props[prop_at(f, 0u)] / params.twodz;
    var x = start;
    for (var it = 0u; it < params.newton_max_iter; it = it + 1u) {
        let r = x / 350.0;
        let p = 1.0 + params.chi * r * r * r;
        let slope = 3.0 * params.chi * r * r / 350.0;
        let lin = a * x + b;
        let set3 = se * x * x * x;
        let res = q - set3 * x + kq * p * lin;
        let dres = -4.0 * set3 + kq * (slope * lin + p * a);
        let delta = -res / dres;
        x = x + delta;
        if (abs(delta) < params.newton_threshold) {
            break;
        }
    }
    return x;
}

/// What the base sits above the layer over it for the heat flow (K), the
/// conductivity there at the temperatures of the step's start.
fn rise(f: u32, base: u32) -> f32 {
    if (params.bottom == 0.0) {
        return 0.0;
    }
    let last = params.n_nodes - 1u;
    let k = harmonic(conductivity(f, last - 1u, t[base + last - 1u]), conductivity(f, last, t[base + last]));
    return params.bottom * params.dz / k;
}

/// One implicit step of a column: `Stepper::implicit`.
///
/// The system for the step's changes `d = T' - T`, at each layer,
///
///   -g lo d_{i-1} + (1 + g (lo + hi)) d_i - g hi d_{i+1}
///       =  c F_i(T) + w (T_i - T_i before)
///
/// eliminated from the base up into `d_i = P_i d_{i-1} + S_i` (`w1`, `w2`),
/// the surface solved with its gradient written from its temperature alone,
/// and the changes added from the top down.
@compute @workgroup_size(64)
fn implicit(@builtin(global_invocation_id) gid: vec3<u32>) {
    let f = facet_of(gid);
    if (f >= params.n_facets) {
        return;
    }
    let n = params.n_nodes;
    let last = n - 1u;
    let base = f * n;

    // The base: `T'_last = T'_above + rise`, in changes.
    var k_here = conductivity(f, last - 1u, t[base + last - 1u]);
    var face_below = harmonic(k_here, conductivity(f, last, t[base + last]));
    var s_below = t[base + last - 1u] - t[base + last];
    if (params.bottom != 0.0) {
        s_below = s_below + params.bottom * params.dz / face_below;
    }
    var p_below = 1.0;
    w1[base + last] = p_below;
    w2[base + last] = s_below;

    var i = last - 1u;
    loop {
        let here = t[base + i];
        let above = t[base + i - 1u];
        let below = t[base + i + 1u];
        let k_above = conductivity(f, i - 1u, above);
        let face_above = harmonic(k_above, k_here);
        let inv = inverse_capacity(f, i, here);
        let lo = face_above * inv;
        let hi = face_below * inv;
        var rhs = params.c_rhs * (lo * (above - here) + hi * (below - here));
        if (params.w_before != 0.0) {
            rhs = rhs + params.w_before * (here - w3[base + i]);
        }
        let gl = params.gamma * lo;
        let gh = params.gamma * hi;
        let den = 1.0 + gl + gh * (1.0 - p_below);
        p_below = gl / den;
        s_below = (rhs + gh * s_below) / den;
        w1[base + i] = p_below;
        w2[base + i] = s_below;
        k_here = k_above;
        face_below = face_above;
        if (i == 1u) {
            break;
        }
        i = i - 1u;
    }

    // The surface's change.
    let t0 = t[base];
    var change = 0.0;
    if (params.surface_given == 1u) {
        change = flux[f] - t0;
    } else {
        let p1 = w1[base + 1u];
        let s1 = w2[base + 1u];
        let p2 = w1[base + 2u];
        let s2 = w2[base + 2u];
        let a = -3.0 + p1 * (4.0 - p2);
        let b = (-3.0 * t0 + 4.0 * t[base + 1u] - t[base + 2u]) - a * t0 + s1 * (4.0 - p2) - s2;
        change = balance(f, t0, flux[f], a, b) - t0;
    }

    // From the top down, each change added to its temperature.
    if (params.keep_before == 1u) {
        w3[base] = t0;
    }
    t[base] = t0 + change;
    for (var j = 1u; j <= last; j = j + 1u) {
        let d = w1[base + j] * change + w2[base + j];
        change = d;
        let v = t[base + j];
        if (params.keep_before == 1u) {
            w3[base + j] = v;
        }
        if (j < last) {
            let added = d + rest[base + j];
            let next = v + added;
            rest[base + j] = added - (next - v);
            t[base + j] = next;
        } else {
            // The base holds no heat: it follows the layer above it.
            t[base + j] = v + d;
        }
    }
}

/// A stage of RKC, in the stages' changes from the temperatures of the
/// step's start: `combine` and `Edges::set` of `stepper.rs`.
///
/// `w1` holds the changes of the stage before, `w2` those of the one before
/// that, and this one's go to `w3`. The explicit step is one stage.
@compute @workgroup_size(64)
fn stage(@builtin(global_invocation_id) gid: vec3<u32>) {
    let f = facet_of(gid);
    if (f >= params.n_facets) {
        return;
    }
    let n = params.n_nodes;
    let last = n - 1u;
    let base = f * n;

    if (params.first == 1u) {
        // The step's start: no change yet.
        for (var j = 0u; j < n; j = j + 1u) {
            w3[base + j] = 0.0;
        }
    } else {
        w3[base] = w1[base];
        w3[base + last] = w1[base + last];
    }
    if (params.do_layers == 1u) {
        var y_here = t[base + 1u] + w1[base + 1u];
        var k_here = conductivity(f, 1u, y_here);
        var face_above = harmonic(conductivity(f, 0u, t[base] + w1[base]), k_here);
        for (var j = 1u; j < last; j = j + 1u) {
            let at = base + j;
            let y_below = t[at + 1u] + w1[at + 1u];
            let k_below = conductivity(f, j + 1u, y_below);
            let face_below = harmonic(k_here, k_below);
            let inv = inverse_capacity(f, j, y_here);
            // A difference between two layers: that of their temperatures
            // plus that of their changes, each held to its own digits.
            let up = (t[at - 1u] - t[at]) + (w1[at - 1u] - w1[at]);
            let down = (t[at + 1u] - t[at]) + (w1[at + 1u] - w1[at]);
            var held = params.mu * w1[at];
            if (params.nu != 0.0) {
                held = held + params.nu * w2[at];
            }
            w3[at] = held + params.w_stage * inv * (face_above * up + face_below * down);
            y_here = y_below;
            k_here = k_below;
            face_above = face_below;
        }
    }
    if (params.do_surface == 1u) {
        if (params.surface_given == 1u) {
            w3[base] = flux[f] - t[base];
        } else {
            let t1 = t[base + 1u] + w3[base + 1u];
            let t2 = t[base + 2u] + w3[base + 2u];
            w3[base] = balance(f, t[base] + w3[base], flux[f], -3.0, 4.0 * t1 - t2) - t[base];
        }
    }
    if (params.do_base == 1u) {
        w3[base + last] = w3[base + last - 1u] + (t[base + last - 1u] - t[base + last]) + rise(f, base);
    }
}

/// The end of an explicit or RKC step: the last stage's changes (`w1`)
/// added to the temperatures, the layers' with what the sum cannot hold
/// kept in `rest`.
@compute @workgroup_size(64)
fn finish(@builtin(global_invocation_id) gid: vec3<u32>) {
    let f = facet_of(gid);
    if (f >= params.n_facets) {
        return;
    }
    let n = params.n_nodes;
    let last = n - 1u;
    let base = f * n;
    t[base] = t[base] + w1[base];
    for (var j = 1u; j < last; j = j + 1u) {
        let v = t[base + j];
        let added = w1[base + j] + rest[base + j];
        let next = v + added;
        rest[base + j] = added - (next - v);
        t[base + j] = next;
    }
    t[base + last] = t[base + last] + w1[base + last];
}
