// The sunlight each facet absorbs, for `tpm_stepper.wgsl`: computed on the
// GPU from the Sun's position, so that a step sends three floats and no
// array. A shader of its own: with the stepper's eight arrays it would pass
// the eight storage buffers that a device has to allow a stage.

struct Sun {
    // The Sun in the body's own frame (m).
    pos: vec3<f32>,
    // The solar constant at 1 au (W/m2).
    constant: f32,
    au: f32,
    n_facets: u32,
    // Width of the dispatch grid in threads.
    stride: u32,
    // Where a facet's properties are: `facet * facet_stride * 2`, 0 or 1.
    facet_stride: u32,
};

@group(0) @binding(0) var<uniform> sun: Sun;
// Facet centres and normals in the body's frame, flat xyz.
@group(0) @binding(1) var<storage, read> positions: array<f32>;
@group(0) @binding(2) var<storage, read> normals: array<f32>;
// The lit fraction of each facet, 1 where fully lit.
@group(0) @binding(3) var<storage, read> lit: array<f32>;
// Two a facet: `e sigma`, and `1 - albedo`.
@group(0) @binding(4) var<storage, read> facet_props: array<f32>;
// One a facet: what it absorbs (W/m2).
@group(0) @binding(5) var<storage, read_write> flux: array<f32>;

/// `S (1 - A) cos(i) lit / r^2` for each facet.
@compute @workgroup_size(64)
fn insolation(@builtin(global_invocation_id) gid: vec3<u32>) {
    let f = gid.y * sun.stride + gid.x;
    if (f >= sun.n_facets) {
        return;
    }
    let i = f * 3u;
    let p = vec3<f32>(positions[i], positions[i + 1u], positions[i + 2u]);
    let nrm = vec3<f32>(normals[i], normals[i + 1u], normals[i + 2u]);
    let v = sun.pos - p;
    let d = length(v);
    let cosi = max(dot(nrm, v / d), 0.0);
    let r = d / sun.au;
    let absorbs = facet_props[f * sun.facet_stride * 2u + 1u];
    flux[f] = sun.constant * absorbs * cosi * lit[f] / (r * r);
}
