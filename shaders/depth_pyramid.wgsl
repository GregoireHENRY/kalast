// The shadow layers' depth pyramid, for the Sun as a disc
// (`light.sun_as_point` off, `gpu::DepthPyramid`): mip `m` holds, for each
// block of `4 * 2^m` texels of the shadow map, the depths nearest and
// farthest from the Sun in it. Built after the shadow pass, a mip at a time;
// `mesh_shadow.wgsl` reads it to know, in a few loads, whether anything near
// a receiver is far enough in front of it to cast a penumbra six texels wide,
// and whether all of it is, the umbra.

// `from_depth` reads the map, `from_min` the mip below; each writes one mip.
@group(0) @binding(0) var depth_map: texture_depth_2d_array;
@group(0) @binding(1) var coarser: texture_storage_2d_array<rg32float, write>;
@group(0) @binding(2) var finer: texture_2d_array<f32>;
@group(0) @binding(3) var gather: sampler;

// Mip 0: 4 x 4 texels of the map each, four gathers of 2 x 2. Past its edge
// a block clamps to the edge, so holds nothing from outside it.
@compute @workgroup_size(8, 8, 1)
fn from_depth(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(coarser);
    if id.x >= size.x || id.y >= size.y {
        return;
    }
    let texel = 1.0 / vec2<f32>(textureDimensions(depth_map));
    // The centre of each 2 x 2: the shared corner of its four texels.
    let base = vec2<f32>(id.xy) * 4.0 + 1.0;
    var near = vec4<f32>(1.0);
    var far = vec4<f32>(0.0);
    for (var k = 0; k < 4; k++) {
        let at = (base + 2.0 * vec2<f32>(f32(k & 1), f32(k >> 1u))) * texel;
        let z = textureGather(depth_map, gather, at, i32(id.z));
        near = min(near, z);
        far = max(far, z);
    }
    let out = vec2<f32>(min(min(near.x, near.y), min(near.z, near.w)), max(max(far.x, far.y), max(far.z, far.w)));
    textureStore(coarser, vec2<i32>(id.xy), i32(id.z), vec4<f32>(out, 0.0, 0.0));
}

// Every other mip: 2 x 2 of the one below.
@compute @workgroup_size(8, 8, 1)
fn from_min(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(coarser);
    if id.x >= size.x || id.y >= size.y {
        return;
    }
    let last = vec2<i32>(textureDimensions(finer)) - 1;
    let base = vec2<i32>(id.xy) * 2;
    var out = vec2<f32>(1.0, 0.0);
    for (var y = 0; y < 2; y++) {
        for (var x = 0; x < 2; x++) {
            let z = textureLoad(finer, min(base + vec2<i32>(x, y), last), i32(id.z), 0).rg;
            out = vec2<f32>(min(out.x, z.x), max(out.y, z.y));
        }
    }
    textureStore(coarser, vec2<i32>(id.xy), i32(id.z), vec4<f32>(out, 0.0, 0.0));
}
