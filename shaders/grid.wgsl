// An infinite ground grid on the XY plane, shaded rather than drawn.
//
// The line-segment grid it replaces ended at the scene bounds, was fixed at
// one spacing, and was one pixel wide because WebGPU has no line width. This
// is the technique Blender's viewport uses and the WebGPU samples' "pristine
// grid" shows: a full-screen pass that intersects the view ray with the plane
// and works out, per pixel, how close that point is to a grid line.
//
// Three levels a factor `major` apart are drawn at once and crossfaded, which
// is what makes it read as infinite at any scale -- kalast scenes run from a
// unit cube to Mars at 1e4 km, so a single spacing cannot serve.

struct Grid {
    // Clip -> world, for turning a screen pixel back into a ray.
    inv_view_proj: mat4x4<f32>,
    // World -> clip, to give the plane a real depth so bodies occlude it.
    view_proj: mat4x4<f32>,

    thin: vec4<f32>,
    thick: vec4<f32>,
    axis_x: vec4<f32>,
    axis_y: vec4<f32>,

    // World units per cell of the finest level, before any coarsening.
    spacing: f32,
    // Line width, pixels.
    width: f32,
    // Cells per brighter line, and the ratio between levels.
    major: f32,
    // Fade out between these grazing factors, 0 looking straight down at the
    // plane and 1 along it.
    fade_near: f32,
    fade_far: f32,
};
@group(0) @binding(0)
var<uniform> grid: Grid;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

// One triangle covering the screen. Cheaper than a quad and, more to the
// point, has no diagonal seam across the middle where derivatives would jump.
@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(i) / 2) * 4.0 - 1.0;
    let y = f32(i32(i) & 1) * 4.0 - 1.0;
    out.ndc = vec2<f32>(x, y);
    out.clip_position = vec4<f32>(x, y, 1.0, 1.0);
    return out;
}

// World position of an NDC point at a given clip depth.
fn unproject(ndc: vec2<f32>, z: f32) -> vec3<f32> {
    let p = grid.inv_view_proj * vec4<f32>(ndc.x, ndc.y, z, 1.0);
    return p.xyz / p.w;
}

// Coverage of the nearest line of a unit-cell grid, per axis.
//
// `uv` is the position in cells and `deriv` how far that coordinate moves
// between neighbouring pixels, so `width * deriv` is the wanted line width
// expressed in cells -- which is what makes a line the same thickness on
// screen however far away, and however oblique, the plane is.
//
// The two clamps are the whole trick, and both were needed:
//
//   - never thinner than a pixel, or a line drifting between sample points
//     flickers; below that it dims instead of vanishing;
//   - never wider than half a cell, or neighbouring lines meet and the grid
//     fills in solid. That is what a grazing view does -- the derivative
//     along the direction running to the horizon is enormous -- and it is
//     what turned the 40-unit view into a grey wash with holes in it. Past
//     that point the coverage is mixed toward the average line density, so
//     the grid greys out smoothly rather than blocking up.
//
// Follows Ben Golus, "The Best Darn Grid Shader (Yet)". The naive form,
// distance-to-line divided by `fwidth`, has neither clamp and fails exactly
// as above.
fn line_coverage(uv: vec2<f32>, deriv: vec2<f32>, width: f32) -> vec2<f32> {
    let d = max(deriv, vec2<f32>(1e-8));
    let want = d * max(width, 0.0);
    let draw = clamp(want, d, vec2<f32>(0.5));
    // Half a pixel each side. `g` below is twice the distance to the line, so
    // this is a one-pixel transition. It was `d * 1.5` and that is three
    // pixels, which on a one-pixel line is not an edge but the whole line --
    // every cell came out two and a half pixels of ink wide and the grid read
    // as a wash.
    let aa = d * 0.5;

    // 0 on a line, 1 at the centre of a cell.
    let g = vec2<f32>(1.0) - abs(fract(uv) * 2.0 - vec2<f32>(1.0));
    var cov = vec2<f32>(1.0) - smoothstep(draw - aa, draw + aa, g);
    cov = cov * clamp(want / draw, vec2<f32>(0.0), vec2<f32>(1.0));
    // Under about two pixels a cell cannot be resolved at all; hand back the
    // fraction of it a line would cover, which is the grey it averages to.
    let wash = clamp(d * 2.0 - vec2<f32>(1.0), vec2<f32>(0.0), vec2<f32>(1.0));
    return mix(cov, min(want, vec2<f32>(1.0)), wash);
}

// Union rather than `max`, so a crossing is not darker than the two lines
// that meet there.
fn line_union(c: vec2<f32>) -> f32 {
    return mix(c.x, 1.0, c.y);
}

// A single line, for the two axes. Same width convention as the grid.
fn axis_coverage(v: f32, deriv: f32, width: f32) -> f32 {
    let d = max(deriv, 1e-8);
    let half_w = max(width, 1.0) * d * 0.5;
    let aa = d * 0.5;
    return 1.0 - smoothstep(half_w - aa, half_w + aa, abs(v));
}

struct FragmentOutput {
    @location(0) color: vec4<f32>,
    // Written so the grid sits in the scene rather than over it. The pass
    // does not *write* depth, but the test still needs a value to compare.
    @builtin(frag_depth) depth: f32,
};

@fragment
fn fs_main(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;

    // The ray through this pixel. Two unprojected points rather than a
    // direction, so it holds for an orthographic camera too, where every ray
    // has the same direction but a different origin.
    let near = unproject(in.ndc, 1.0);
    let far = unproject(in.ndc, 0.0);
    let dir = far - near;

    // Where it meets z = 0. A ray parallel to the plane never does.
    if abs(dir.z) < 1e-12 {
        discard;
    }
    let t = -near.z / dir.z;
    let p = near + dir * t;

    // The one test that matters is whether the ground is in front of the
    // camera; the near and far planes are not a statement about the plane.
    // They are fitted to the bodies, so at 40 units out the frustum is a slab
    // a couple of units deep and clipping to it left the grid as a narrow
    // band across the middle of the screen with black above and below -- the
    // hard edges were the near and far planes, not the horizon.
    //
    // For a perspective camera "in front" is the sign of the clip w. An
    // orthographic one has every w at 1 and its near plane *is* the camera
    // plane, so there `t` answers it instead.
    let clip = grid.view_proj * vec4<f32>(p, 1.0);
    let persp = abs(grid.view_proj[0].w)
        + abs(grid.view_proj[1].w)
        + abs(grid.view_proj[2].w) > 1e-9;
    if persp {
        if clip.w <= 1e-9 {
            discard;
        }
    } else if t < 0.0 {
        discard;
    }

    // Taken once, on the world position, and divided down per level. Taking
    // `fwidth` of the already-divided coordinate instead reads the derivative
    // across a level boundary, where the divisor itself jumps, and speckles
    // the plane with bright dots.
    let deriv = fwidth(p.xy);
    let world_px = max(max(deriv.x, deriv.y), 1e-12);

    // Coarsen until the finest level spans at least eight pixels. `major` is
    // both the ratio between levels and the count of cells per brighter line,
    // which is the same statement seen from two sides.
    let base = max(grid.spacing, 1e-12);
    let r = max(grid.major, 2.0);
    let lod = max(0.0, log(8.0 * world_px / base) / log(r));
    let lo = floor(lod);
    let blend = lod - lo;

    let s0 = base * pow(r, lo);
    let s1 = s0 * r;
    let s2 = s1 * r;

    let c0 = line_union(line_coverage(p.xy / s0, deriv / s0, grid.width)) * (1.0 - blend);
    let c1 = line_union(line_coverage(p.xy / s1, deriv / s1, grid.width));
    let c2 = line_union(line_coverage(p.xy / s2, deriv / s2, grid.width)) * blend;

    // Three levels rather than two, weighted so the stack is continuous as it
    // shifts: the finest fades out, the middle is always solid, the coarsest
    // fades in. With two, the level arriving at the top arrived at full
    // strength and popped.
    //
    // The middle level's colour walks from thick to thin over the same blend,
    // so that when `lo` steps and every level moves up one slot, the line
    // changing hands does not change shade either.
    let mid = mix(grid.thick, grid.thin, blend);

    var col = grid.thin.rgb;
    var a = c0 * grid.thin.a;
    let a1 = c1 * mid.a;
    if a1 > a {
        a = a1;
        col = mid.rgb;
    }
    let a2 = c2 * grid.thick.a;
    if a2 > a {
        a = a2;
        col = grid.thick.rgb;
    }

    // The two axes on top, so the origin reads without hunting for it.
    let on_x = axis_coverage(p.y, deriv.y, grid.width);
    let on_y = axis_coverage(p.x, deriv.x, grid.width);
    if on_x * grid.axis_x.a > a {
        a = on_x * grid.axis_x.a;
        col = grid.axis_x.rgb;
    }
    if on_y * grid.axis_y.a > a {
        a = on_y * grid.axis_y.a;
        col = grid.axis_y.rgb;
    }

    // Fade out as the view ray flattens, or the grid ends on a hard line of
    // aliasing at the horizon.
    //
    // On obliquity, not distance. Two distance-based versions failed here for
    // the same reason: the plane is infinite and the far plane is not what
    // bounds it. The visible edge is the true horizon, where the ray turns
    // parallel to the plane and `t` goes negative -- so a fade keyed to the
    // far plane, in world units or as a fraction of it, never reaches its
    // ramp and the band stayed at full brightness right up to the cut. The
    // angle between the ray and the plane normal does not care how big the
    // scene is, which is the property wanted.
    let graze = 1.0 - abs(normalize(dir).z);
    let f1 = max(grid.fade_far, grid.fade_near + 1e-4);
    a = a * (1.0 - smoothstep(grid.fade_near, f1, graze));

    if a <= 0.002 {
        discard;
    }

    // Depth from the real intersection, in whatever convention the camera's
    // projection uses -- reversed-Z included, since this is the same matrix
    // the bodies were drawn with. A hardcoded value here is the mistake
    // `colorbar.wgsl` made.
    //
    // Clamped rather than clipped, which is the other half of drawing ground
    // outside the depth range: past the far plane it pins to the farthest
    // depth, so every body still occludes it, and nearer than the near plane
    // it pins to the nearest, so it occludes them -- which is what ground
    // between you and the scene should do. The pass writes no depth, so
    // nothing downstream inherits the pinned value.
    //
    // The floor is an epsilon above the far value, not the far value itself,
    // and that is the whole difference between an infinite plane and one that
    // stops. Depth is reversed here: far is 0.0, the buffer is *cleared* to
    // 0.0, and the test is `Greater`. So clamping to exactly 0.0 pinned every
    // fragment past the far plane onto the clear value, where `0.0 > 0.0` is
    // false and the depth test threw it away -- against empty background, not
    // against any geometry. The grid looked like it was being clipped to the
    // far plane because it effectively was, by the depth test rather than by
    // the frustum, and no amount of widening the obliquity fade moved the
    // edge. Real geometry sits far above this floor (reversed depth at the
    // far plane is near/far, ~1e-3 for the ratios fitted here), so a body
    // still wins everywhere it should.
    out.depth = clamp(clip.z / clip.w, 1e-7, 1.0);
    out.color = vec4<f32>(col, a);
    return out;
}
