# Primitive picking — 9 September 2026

The second of the three items from `2026-09-09_HANDOFF_gpu_timing.md`, done
after reversed-Z.

Most of it already existed. `src/app/facet_id.rs` has rendered facet indices
into an R32Uint target for a while, exposed as `request_facet_id` /
`facet_id_map` and feeding the FITS products. What it could not do was answer
about **one pixel**: `render_and_read` copies the whole target, which at
2116x1376 is a ~12 MB staging buffer, a blocking map and 2.9M texels unpadded
on the CPU — to answer a question about four bytes.

So the work was the readback shape, not the pass.

## What was added

- **`FacetIdPass::read_at(x, y)`**, beside `render_and_read`. The render is
  factored out into `record`, which both call; only the copy differs — one
  texel into a 256-byte staging buffer, since `bytes_per_row` has to meet the
  alignment even for a single pixel.
- **`Window::facet_id_at(x, y)`**, mirroring `facet_id_map`.
- **`Simulation::resolve_facet_id`**, which turns a texel into the tuple
  `pick_facet` returns. The GPU says *which* facet; one triangle test
  (`Mesh::intersect_facet`, also new) says *where*. That keeps the lat/lon the
  click handler prints, which a bare index cannot give.
- **`Simulation::pickable_on_gpu`**, the guard below.
- **`sim.request_facet_pick(x, y)` / `sim.facet_pick()`** in Python, following
  the request/read shape every other GPU query here uses.

The click handler uses it and falls back to the ray; `pick_facet` is unchanged
and still public, because it answers what a screen pixel cannot — a ray from an
arbitrary origin, such as an instrument boresight.

## The measurement, which changes the recommendation

The handoff assumed the CPU ray was the slow one and the GPU the fix. That is
true, but only above about 100k facets — and getting the number required
throwing away a first attempt.

**One Didymos body, 800x600, `msaa = 1`, `vsync = False`, medians of 60 frames:**

| facets | plain frame | + 1x1 pick | + whole-map readback | `pick_facet` |
|---|---|---|---|---|
| 81,708 | 2.98 ms | 4.07 ms (**+1.09**) | 6.25 ms (+3.27) | **0.83 ms** |
| 2,621,156 | 14.24 ms | 17.98 ms (**+3.74**) | 19.72 ms (+5.48) | **23.08 ms** |

So:

- At full resolution the GPU pick is **6.2x faster** than the ray, and nearly
  flat in mesh size where the ray is linear.
- At 100k the ray still wins, and both are under a millisecond, so it does not
  matter. The crossover is around 100k.
- **Shrinking the readback bought 1.7–2.2 ms.** The second geometry pass is the
  rest of the cost, and is why this is per *click* and not something to leave
  on. Making picking genuinely O(1) would mean an id target resident on the
  main pass, which costs bandwidth every frame for a feature used on click.

### The first measurement was wrong, and said so plainly

Interleaving the three modes in one run — plain, pick, map, round-robin — gave
a 1x1 copy costing **more** than a 12 MB one (33.81 ms against 20.69 ms at
2.6M facets). That is not a subtle bias, it is impossible, which is what made
it obvious.

The cause: a frame that blocks on a readback drains whatever the previous frame
left in flight. Round-robin puts `pick` immediately after a non-blocking
`plain` frame every time, so it is charged for that frame's work, while `map`
follows an already-drained `pick`. The plain frame looked like 0.55 ms for
2.6M facets by the same mechanism — it was measuring how long it took to
*queue* the work, not to do it.

Fixed by running **one mode per process**, every frame the same, so each
reaches steady state and no frame inherits another's backlog. The plain frame
then reads 14.24 ms, which is the real number and consistent with
`2026-09-09_gpu_pass_timings.md`.

Worth remembering generally: **any benchmark that mixes blocking and
non-blocking frames measures the ordering, not the work.**

## The trap: one indexed mesh spoils the scene

The id pass draws only flattened meshes — the facet index comes from the vertex
index, so an indexed mesh would produce wrong indices rather than none, and the
pass skips it. The consequence is not confined to that body: it is **absent
from the target**, so a body behind it is picked straight through it.

Hence `pickable_on_gpu` is all-or-nothing. One indexed body and the whole
scene falls back to the ray, which handles both. Guarded by
`one_indexed_body_takes_the_whole_scene_off_the_gpu_path`.

## A reversed-Z regression, found by doing this

`select_at_cursor` built its ray by unprojecting hardcoded clip depths — 0.0
for near, 1.0 for far. Reversed-Z swaps those, so the ray started on the *far*
plane pointing back at the camera: a click would have selected the far side of
the body rather than missing. `pick_facet`'s own tests pass an explicit ray and
never touch the unprojection, so nothing caught it.

The construction now lives in `Eye::ray_through_ndc`, next to the matrices that
define the convention, with `the_picking_ray_leaves_the_eye_going_forwards`
over three screen positions. It is also what the GPU path needs for its
triangle test, so there is one ray builder rather than two.

## Verification

`read_at` is the risky part — an origin or row-alignment mistake picks the
wrong facet silently — and no unit test can reach it without a GPU. Checked
from Python over 54 probe pixels on the crater, 48 on geometry and 6 on
background:

- every pick names the facet `facet_id_map` holds at that pixel, which is the
  single-texel copy agreeing with the full one;
- every pick agrees with `pick_facet` along the ray from the camera through the
  returned point, which is an independent implementation and is what would
  catch a depth sense resolving to the far surface;
- every returned point lies on the returned facet, to 1e-5 — `Float` is `f32`
  here, and two rays reaching one facet differ at about 1e-7.

All 54 agree. The background pixels return `None` rather than facet 0 of body
0, which is what the `+ 1` in the encoding is for.

Three CPU-side tests cover the rest without a GPU: `resolve_facet_id` handed
the id the pass *would* have written must reproduce what the ray found, an
out-of-range id must decode to nothing, and the indexed-mesh guard above.
