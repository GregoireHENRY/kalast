// Band-integrated radiance from facet temperature, by table lookup.
//
// `BandRadiance` on the CPU tabulates `eps * integral(B(T,w) R(w) dw)` over
// 20,000 temperatures and interpolates with `numpy.interp`, which binary
// searches. The table is a `linspace`, so the index is arithmetic and no
// search is needed at all -- which is most of why this is worth moving.
//
// One invocation per (facet, band).

struct Params {
    n_facets: u32,
    n_bands: u32,
    n_table: u32,
    // Stride between facets in `temps`. 1 when temperatures were uploaded on
    // their own; n_nodes when this reads a GpuTpm column buffer in place,
    // where the surface node is every n_nodes'th entry.
    temp_stride: u32,
    t_min: f32,
    // 1 / table spacing, so the lookup is a multiply rather than a divide.
    t_inv_step: f32,
    stride_x: u32,
    _pad: u32,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> temps: array<f32>;
@group(0) @binding(2) var<storage, read> tables: array<f32>;
@group(0) @binding(3) var<storage, read_write> radiance: array<f32>;

@compute @workgroup_size(64)
fn band_radiance(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.y * params.stride_x + gid.x;
    if (idx >= params.n_facets * params.n_bands) {
        return;
    }
    let facet = idx / params.n_bands;
    let band = idx % params.n_bands;

    let t = temps[facet * params.temp_stride];
    // Clamped at both ends, matching `numpy.interp`, which returns the
    // endpoints outside the tabulated range rather than extrapolating. The
    // CPU class raises instead; check the range there if it matters.
    let x = clamp(
        (t - params.t_min) * params.t_inv_step,
        0.0,
        f32(params.n_table - 1u) - 1e-4,
    );
    let i = u32(floor(x));
    let frac = x - floor(x);
    let base = band * params.n_table + i;
    radiance[idx] = mix(tables[base], tables[base + 1u], frac);
}
