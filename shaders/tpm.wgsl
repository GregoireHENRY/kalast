// Thermophysical model: radiative surface boundary + 1D conduction, per facet.
//
// One facet is one independent column, which is what makes this a good GPU
// workload: 10,000 facets x 34 nodes is 340,000 independent stencil updates a
// step, and the CPU path spends 93.5 % of its time here.
//
// **f32, not f64.** WGSL has no f64. The CPU model runs float64, so the two
// cannot agree to the last bit and the difference has to be measured rather
// than assumed -- see examples/analytical/tpm_gpu_vs_cpu.py. Surface
// temperatures are 100-400 K, where f32 resolves ~3e-5 K, well under the
// 0.1 K Newton threshold; the risk is not the absolute value but the
// conduction increment, which is a small difference of nearby numbers.

struct Params {
    n_facets: u32,
    n_nodes: u32,
    // Radiative boundary: se = emissivity * sigma.
    se: f32,
    conductivity: f32,
    twodz: f32,
    newton_threshold: f32,
    newton_max_iter: u32,
    // Width of the dispatch grid in invocations, one per entry point.
    //
    // A dispatch is capped at 65,535 workgroups per dimension. At 3.1M facets
    // and 34 nodes the conduction pass needs 1,671,168 of them, so it is
    // dispatched two-dimensionally and the linear index rebuilt here.
    stride_surface: u32,
    stride_conduction: u32,
};

@group(0) @binding(0) var<uniform> params: Params;
// Node temperatures, row-major: index = facet * n_nodes + node.
@group(0) @binding(1) var<storage, read_write> t_in: array<f32>;
@group(0) @binding(2) var<storage, read_write> t_out: array<f32>;
// Absorbed flux per facet, W/m2.
@group(0) @binding(3) var<storage, read> flux: array<f32>;
// Per interior node: 2 dt / (h_lo * (h_lo + h_hi)) and its h_hi twin.
@group(0) @binding(4) var<storage, read> coef_lo: array<f32>;
@group(0) @binding(5) var<storage, read> coef_hi: array<f32>;
// Diffusivity per node, so a layered column is expressible.
@group(0) @binding(6) var<storage, read> diffusivity: array<f32>;

/// Solve `F - se T^4 + k (-3T + 4T1 - T2) / (2 dz) = 0` for the surface node.
///
/// One thread per facet: each reads only its own three shallowest nodes, so
/// there is no sharing and no race. Writes in place, exactly as the CPU path
/// does before conduction runs.
@compute @workgroup_size(64)
fn surface(@builtin(global_invocation_id) gid: vec3<u32>) {
    let f = gid.y * params.stride_surface + gid.x;
    if (f >= params.n_facets) {
        return;
    }
    let base = f * params.n_nodes;
    var t = t_in[base];
    let t1 = t_in[base + 1u];
    let t2 = t_in[base + 2u];
    let k_over = params.conductivity / params.twodz;

    for (var i = 0u; i < params.newton_max_iter; i = i + 1u) {
        let se_t3 = params.se * t * t * t;
        let fn_ = flux[f] - se_t3 * t + k_over * (-3.0 * t + 4.0 * t1 - t2);
        let dfn = -4.0 * se_t3 - 3.0 * k_over;
        let delta = fn_ / dfn;
        t = t - delta;
        if (abs(delta) < params.newton_threshold) {
            break;
        }
    }
    t_in[base] = t;
}

/// Advance every interior node one explicit step, `t_in` -> `t_out`.
///
/// Out of place because the stencil reads its neighbours: updating in place
/// would let a thread read a value another thread had already advanced. The
/// CPU path gets this for free, since numpy evaluates the whole right-hand
/// side before assigning.
@compute @workgroup_size(64)
fn conduction(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.y * params.stride_conduction + gid.x;
    let total = params.n_facets * params.n_nodes;
    if (idx >= total) {
        return;
    }
    let node = idx % params.n_nodes;
    let last = params.n_nodes - 1u;

    // Surface node is set by `surface`, and carried across untouched.
    if (node == 0u) {
        t_out[idx] = t_in[idx];
        return;
    }

    // The base node is held at zero gradient: it copies the node above it
    // *after* that node has been advanced, so recompute rather than read a
    // neighbour this dispatch may not have written yet.
    var n = node;
    if (node == last) {
        n = last - 1u;
    }
    let base = idx - node;
    let t_mid = t_in[base + n];
    let t_lo = t_in[base + n - 1u];
    let t_hi = t_in[base + n + 1u];
    let c = n - 1u;
    t_out[idx] = t_mid + diffusivity[n]
        * (coef_lo[c] * (t_lo - t_mid) + coef_hi[c] * (t_hi - t_mid));
}
