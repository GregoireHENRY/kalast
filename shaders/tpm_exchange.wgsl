// What facets send to one another, on the GPU: the twin of `tpm::exchange`.
//
// The view factors are compressed sparse rows; one thread a facet sums its
// own row. The radiation leaving a facet, its radiosity, is
//
//   visible:  J = A (S + F J)                 S the sunlight falling on it
//   thermal:  J = e sigma T^4 + (1 - e) F J
//
// found by substitution: `source` writes the sources, each `bounce` reads the
// radiosity of the pass before and writes the next (the two arrays change
// places between dispatches), and `absorb` writes what each facet keeps,
// `(1 - A) F J` of the first and `e F J` of the second.
//
// Eight storage buffers, the number a device has to allow a stage.

struct Params {
    n_facets: u32,
    // Width of the dispatch grid in threads.
    stride: u32,
    // Stefan and Boltzmann's constant (W m-2 K-4).
    sigma: f32,
    pad: u32,
};

@group(0) @binding(0) var<uniform> params: Params;
// Row `f` holds `value[start[f] .. start[f + 1]]`, in the columns `column[...]`.
@group(0) @binding(1) var<storage, read> start: array<u32>;
@group(0) @binding(2) var<storage, read> column: array<u32>;
@group(0) @binding(3) var<storage, read> value: array<f32>;
// Two a facet: albedo, emissivity.
@group(0) @binding(4) var<storage, read> props: array<f32>;
// Two a facet: the sunlight falling on it (W/m2), its temperature (K).
@group(0) @binding(5) var<storage, read> given: array<f32>;
// Two a facet: the radiosity in the visible and in the thermal infrared.
@group(0) @binding(6) var<storage, read> before: array<f32>;
@group(0) @binding(7) var<storage, read_write> after: array<f32>;
// Two a facet: what it absorbs of each (W/m2).
@group(0) @binding(8) var<storage, read_write> absorbed: array<f32>;

fn sources(f: u32) -> vec2<f32> {
    let t = given[2u * f + 1u];
    let t2 = t * t;
    return vec2<f32>(props[2u * f] * given[2u * f], props[2u * f + 1u] * params.sigma * t2 * t2);
}

/// `sum_j F_fj J_j`, for the two radiosities.
fn arriving(f: u32) -> vec2<f32> {
    var sum = vec2<f32>(0.0, 0.0);
    let end = start[f + 1u];
    for (var k = start[f]; k < end; k = k + 1u) {
        let c = column[k];
        sum = sum + value[k] * vec2<f32>(before[2u * c], before[2u * c + 1u]);
    }
    return sum;
}

@compute @workgroup_size(64)
fn source(@builtin(global_invocation_id) gid: vec3<u32>) {
    let f = gid.y * params.stride + gid.x;
    if (f >= params.n_facets) {
        return;
    }
    let s = sources(f);
    after[2u * f] = s.x;
    after[2u * f + 1u] = s.y;
}

@compute @workgroup_size(64)
fn bounce(@builtin(global_invocation_id) gid: vec3<u32>) {
    let f = gid.y * params.stride + gid.x;
    if (f >= params.n_facets) {
        return;
    }
    let s = sources(f);
    let e = arriving(f);
    after[2u * f] = s.x + props[2u * f] * e.x;
    after[2u * f + 1u] = s.y + (1.0 - props[2u * f + 1u]) * e.y;
}

@compute @workgroup_size(64)
fn absorb(@builtin(global_invocation_id) gid: vec3<u32>) {
    let f = gid.y * params.stride + gid.x;
    if (f >= params.n_facets) {
        return;
    }
    let e = arriving(f);
    absorbed[2u * f] = (1.0 - props[2u * f]) * e.x;
    absorbed[2u * f + 1u] = props[2u * f + 1u] * e.y;
}
