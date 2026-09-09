// Bounding-box occlusion queries.
//
// Draws each body's world-space bounding box with the depth test on and depth
// writes off, wrapped in an occlusion query. The count that comes back is the
// number of samples that passed depth, so zero means the box -- and therefore
// the body -- contributed nothing to the image.
//
// Drawn at the *end* of the main pass, once every body has written depth, so
// the answer is occlusion against the finished scene rather than against
// whatever happened to be drawn first. Querying the bodies' own draws instead
// would count samples that passed at the moment each was drawn, which a body
// drawn early and covered later still does.
//
// A box is a conservative stand-in for the body: it can be visible where the
// body is not. That is the same direction the frustum test already errs in --
// never claiming something is missing when it is there.

// Only the camera is needed, and this is a prefix of the renderer's existing
// `View` uniform, so the pass reuses that bind group rather than keeping a
// second camera that could disagree with what was drawn.
struct Camera {
    view_proj: mat4x4<f32>,
};

struct View {
    camera: Camera,
};
@group(0) @binding(0)
var<uniform> view: View;

struct Box {
    lo: vec3<f32>,
    _pad0: f32,
    hi: vec3<f32>,
    _pad1: f32,
};
@group(1) @binding(0)
var<uniform> aabb: Box;

/// Corner index for one of the 36 vertices of the box, three bits of it
/// selecting `hi` over `lo` on each axis. Wound arbitrarily -- the pipeline
/// culls nothing, because a box the camera is inside would otherwise present
/// only faces that are behind it.
fn corner(i: u32) -> u32 {
    var t = array<u32, 36>(
        0u, 2u, 1u,  1u, 2u, 3u,   // z = lo
        4u, 5u, 6u,  5u, 7u, 6u,   // z = hi
        0u, 1u, 4u,  1u, 5u, 4u,   // y = lo
        2u, 6u, 3u,  3u, 6u, 7u,   // y = hi
        0u, 4u, 2u,  2u, 4u, 6u,   // x = lo
        1u, 3u, 5u,  3u, 7u, 5u,   // x = hi
    );
    return t[i];
}

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let c = corner(i);
    let p = vec3<f32>(
        select(aabb.lo.x, aabb.hi.x, (c & 1u) != 0u),
        select(aabb.lo.y, aabb.hi.y, (c & 2u) != 0u),
        select(aabb.lo.z, aabb.hi.z, (c & 4u) != 0u),
    );
    return view.camera.view_proj * vec4<f32>(p, 1.0);
}

// Present, and writing to a target masked off entirely, rather than absent.
// An occlusion query counts samples that survive the per-fragment tests, and
// a pipeline with no fragment stage is a thinner guarantee than is worth
// relying on for a number the panel prints.
@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 0.0);
}
