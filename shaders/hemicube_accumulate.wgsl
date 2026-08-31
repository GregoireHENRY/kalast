// Hemicube view factors, accumulation half.
//
// Walks the integer atlas written by `hemicube.wgsl` and adds each pixel's
// delta form factor to whichever facet the pixel shows.
//
// Fixed point, not floats
// -----------------------
//
// WGSL has no atomic float add, and this is a scatter -- many pixels land on
// the same facet -- so it needs one. Each delta form factor is scaled by 2^30
// and added as a u32 instead.
//
// That is safe by construction rather than by luck. The delta form factors
// over a whole hemicube sum to exactly 1, so the total per facet cannot
// exceed 2^30 and cannot overflow a u32. At the other end the smallest weight
// on a 128 px face is ~8.7e-6, still ~9,300 in fixed point -- about four
// decimal digits of headroom on the least significant contribution.

const FIXED_SCALE: f32 = 1073741824.0;   // 2^30

struct Accum {
    // Where in `acc` this hemicube's row starts, and how long it is.
    row_offset: u32,
    n_facets: u32,
    width: u32,
    height: u32,
};
@group(0) @binding(0)
var<uniform> accum: Accum;
@group(0) @binding(1)
var ids: texture_2d<u32>;
@group(0) @binding(2)
var weights: texture_2d<f32>;
@group(0) @binding(3)
var<storage, read_write> acc: array<atomic<u32>>;

@compute @workgroup_size(8, 8, 1)
fn cs_accumulate(@builtin(global_invocation_id) gid: vec3<u32>) {
    if gid.x >= accum.width || gid.y >= accum.height {
        return;
    }
    let c = vec2<i32>(i32(gid.x), i32(gid.y));
    let id = textureLoad(ids, c, 0).r;
    if id == 0u {
        return;   // no facet at this pixel
    }
    let facet = id - 1u;
    if facet >= accum.n_facets {
        return;
    }
    let w = textureLoad(weights, c, 0).r;
    if w <= 0.0 {
        return;   // the masked half of a side face
    }
    atomicAdd(&acc[accum.row_offset + facet], u32(w * FIXED_SCALE));
}
