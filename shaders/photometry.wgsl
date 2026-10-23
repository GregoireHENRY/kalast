// The flux of what the camera sees, summed (`app::photometry`): the
// photometry pass's floats -- each pixel the I/F the Sun leaves it, the
// body's law times the Sun it sees -- added up 16 x 16 pixels at a time, a
// pair a tile: the tile's sum, and the share of it on the frame's edge,
// which says a lit body runs off the frame. The tiles are added in f64 on
// the CPU, so the sum does not depend on the image's size.

@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> tiles: array<vec2<f32>>;

var<workgroup> sums: array<vec2<f32>, 256>;

@compute @workgroup_size(16, 16)
fn cs_sum(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(local_invocation_index) li: u32,
    @builtin(workgroup_id) wg: vec3<u32>,
    @builtin(num_workgroups) n: vec3<u32>,
) {
    let size = textureDimensions(image);
    var v = vec2<f32>(0.0);
    if gid.x < size.x && gid.y < size.y {
        let f = textureLoad(image, vec2<i32>(gid.xy), 0).r;
        let edge = gid.x == 0u || gid.y == 0u || gid.x + 1u == size.x || gid.y + 1u == size.y;
        v = vec2<f32>(f, select(0.0, f, edge));
    }
    sums[li] = v;
    workgroupBarrier();
    // Pairwise, so the tile's 256 add with 8 roundings each, not 255.
    for (var s = 128u; s > 0u; s = s >> 1u) {
        if li < s {
            sums[li] = sums[li] + sums[li + s];
        }
        workgroupBarrier();
    }
    if li == 0u {
        tiles[wg.y * n.x + wg.x] = sums[0];
    }
}
