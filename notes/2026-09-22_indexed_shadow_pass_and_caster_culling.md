# 2026-09-22 — the shadow pass draws shared vertices, and culls

**Question.** `examples/didymos/main.py` on the full models -- 2 × 3,145,728
facets, flat -- ran at 41.6 it/s. Where does the frame go, and how far can
the render pipeline be pushed *without* shadow proxies (`shadow_path`), which
were ruled out for this round?

Measured with an instrumented copy of the example in the scratch directory
(`config.debug.gpu_timing = True`, `open_in_background`), steady state from
10 to 40 s after the loop starts, three runs per configuration, the first
run after each rebuild discarded. Numbers below are the median run.

## The frame, before

| | ms |
|---|---|
| frame (wall) | 24.07 |
| span (GPU, first to last timestamp) | 24.46 |
| shadow pass | 19.65 |
| render pass | 15.68 |
| text | 2.15 |

GPU-bound: wall and span agree to 0.4 ms, so the CPU (SPICE calls, the
Python loop, submission) is not the limit. The shadow pass is the largest
item: two 8192² layers (one per body, `shadows.per_body`), every body drawn
into every layer, 4 draws of 3.1 M facets a frame. The per-pass numbers
overlap on the GPU, which is why shadow + render exceeds the span.

## What it was drawing

A flat mesh is drawn non-indexed, three vertices per facet, because the
shaded pass finds a facet's attributes by `vertex_index / 3`
(`mesh_shadow.wgsl`; `notes/2026-09-18_memory_meshes_and_shadow_maps.md`).
The shadow pass went through the same `MeshBuffer::render`, so it shaded
9.4 M corners per body per layer -- 37.7 M vertex invocations a frame -- for
a vertex stage that reads a position and an instance matrix and nothing
else; `shadow.wgsl` never touches `vertex_index`. The index buffer was
already uploaded for flat meshes (the per-facet shadow query reads it as
storage) and never drawn with.

## 1. Indexed, over the shared vertices

`MeshBuffer` gains `shared_positions`: for a flat mesh, `positions` as loaded
-- 1.58 M × 12 B = **19 MB per 3M model**, on top of the 213 MB it already
holds -- and `None` for a smooth mesh, whose geometry buffer already is that.
`MeshBuffer::render_depth` binds it and draws `draw_indexed` through the
existing index buffer. The shadow pass calls that; the facet-id, hemicube
and light-cube passes keep `render`, since the first two still name the
facet by `vertex_index / 3`. Same triangles, same depths; the vertex stage
runs 1.6 M times per body per layer instead of 9.4 M.

| | before | indexed |
|---|---|---|
| it/s | 41.6 | **54.6** (56.2 / 53.9 / 54.6) |
| shadow | 19.65 | 10.96 |
| render | 15.68 | 13.27 |
| span | 24.46 | 17.76 |

The render pass got faster though nothing in it changed: it overlaps the
shadow pass on the GPU, and a lighter neighbour leaves it more of the
machine.

## 2. Back faces culled in the shadow pass

The shadow pipeline was `cull_mode: None`, on purpose -- CONFIG.md said so:
non-closed geometry (open craters, clipped sections, single-sided surfaces)
must cast from whichever side faces the light. On closed geometry, though,
the nearest surface along any ray from the light is a front face, so culling
the back faces leaves the depth map identical and halves the primitives the
rasteriser is handed. `shading.render_back_face` already is a script's
declaration that its geometry is not closed, for the main pass; the shadow
pass now reads the same flag. `false` (the default) culls in both; `true`
culls in neither, which is exactly the old shadow behaviour for the meshes
that needed it. A toggle rebuilds every pipeline (`Window::rebuild_passes`),
as it already did for the main pass.

| | indexed | indexed + culled |
|---|---|---|
| it/s | 54.6 | **64.2** (64.2 / 67.4 / 59.6) |
| shadow | 10.96 | 8.71 |
| render | 13.27 | 12.76 |
| span | 17.76 | 17.10 |
| frame (wall) | 18.31 | 15.57 |

Wall is now under span: consecutive frames overlap on the GPU, one frame's
shadow pass starting before the previous frame's render pass has drained.

Not tried: front-face culling in the shadow pass, the textbook acne fix. It
would move the stored depth from the front surface to the back, and the
biases in `mesh_shadow.wgsl` are calibrated against the front
(`notes/2026-09-08_shadow_bias.md`); the culled-back-face map is identical
to the unculled one, so there is nothing to buy by changing the sense.

## 3. A body is not drawn into a layer it cannot reach

Every body was drawn into every layer -- right, since anything between the
Sun and a body has to cast into that body's layer -- but a per-body layer is
sized laterally to its own body, and for most of an orbit the other body
projects wholly beside it. `Window::update` now keeps, per layer, the bodies
whose world AABB can rasterise under the layer's matrix
(`aabb_may_hit_frustum`: all eight corners beyond one clip plane means every
fragment would have been clipped, so skipping the draw leaves the map bit
for bit as it was; a box astride a plane is kept). Unit-tested for the six
cases; the five shadow tests pass unchanged.

| | indexed + culled | + caster skip |
|---|---|---|
| shadow | 8.71 | **7.57** (7.57 / 7.56 / 8.71) |
| render | 12.76 | 13.14 |
| span | 17.10 | 15.46 |
| it/s | 64.2 | 62.5 (64.1 / 62.5 / 62.1) |

The shadow pass lost 13 % and the frame did not move: the shadow pass now
overlaps the render pass completely, and the frame is the render pass plus
the text pass. Kept anyway -- it is free per frame, output-identical, and it
pays again the moment the render pass shrinks or a scene has more bodies.

## Where it stands

**41.6 → 64.2 it/s, +54 %**, no proxies, and no change to any shadow a
closed mesh casts. `test_pcf_filters`, `test_far_sun`, `test_shadow_layers`,
`test_facet_shadow`, `test_polygon_shadow` pass; `cargo test --release` with
and without the `python` feature.

The frame is now render 12.8-13.8 ms with shadow 7.6 fully overlapped, plus
2.2 of text; 100 it/s is 10 ms. What is left, roughly by expected return:

- **The render pass, 12.8 ms, is the largest item now.** 6.3 M facets
  through the shaded vertex stage, non-indexed, then PCF per pixel. It is
  non-indexed *for* the `vertex_index / 3` lookup, and a shared vertex
  belongs to about six facets, so drawing it indexed needs the facet id from
  somewhere else -- a shader design question, not a one-line change.
- ~~The shadow pass still draws every body into every layer.~~ Done above;
  it no longer shows in the frame because the render pass is the frame.
- **Layer resolution.** 8192² per body; 4096 quarters the fill. Whether the
  shadow on the smaller body holds at that is a measurement not yet made.
- Shadow proxies (`shadow_path`), ruled out for this round.
