# 2026-09-22 — the main pass draws a flat mesh over its shared vertices

Continues `2026-09-22_indexed_shadow_pass_and_caster_culling.md`, which left
the frame at 62-64 it/s with the render pass as the whole of it: 12.8-13.8 ms
of a 15.6 ms frame on `examples/didymos/main.py` at 2 × 3.1 M facets.

## Where the render pass went

Two variants of the same benchmark, one run each, to see what the pass
scales with:

| | render pass | frame |
|---|---|---|
| default (msaa 4, ~1.6 M px) | 13.44 ms | 63.2 it/s |
| msaa 1 | 11.13 | 74.9 |
| 400 × 300 window (msaa 4) | 10.06 | 80.0 |

Twelve times fewer pixels took 3.4 ms off; the remaining 10 ms is geometry.
The pass shaded 18.9 M vertices a frame -- a flat mesh was drawn non-indexed,
three corners per facet, because the shader found a facet's attributes by
`vertex_index / 3` and its wireframe barycentrics by `vertex_index % 3`.

## The change

`PRIMITIVE_INDEX` is a WebGPU feature in wgpu 30 (`@builtin(primitive_index)`
in the fragment stage, `enable primitive_index;`); this adapter (Apple M1
Pro, Metal) has it, as do Apple7+, Mac2, DX12 and most Vulkan.

- **The facet is found in the fragment stage.** For a flat mesh
  (`INSTANCE_FLAG_FLAT`) `fs_main` reads `attrs[primitive_index]` -- normal,
  colour, mode, value -- and the vertex stage does the position and nothing
  else. The instance's normal matrix reaches the fragment as three flat
  varyings for the normal. A smooth mesh is unchanged: per-vertex
  attributes read in the vertex stage and interpolated.
- **So a flat mesh is drawn indexed**, over the `shared_positions` buffer the
  shadow pass already had (`MeshBuffer::render_depth`), 1.6 M vertices
  instead of 9.4 M per body, with the same index buffer.
- **Except while the wireframe is on.** Its barycentrics are
  `vertex_index % 3`, which only a non-indexed draw has. `Window::update`
  sets `INSTANCE_FLAG_CORNERS` when `wireframe.mode != 0` and
  `render::Pass::render` picks the non-indexed draw from the same field;
  the shader's `can_wireframe` is that bit now, not the flat bit.
  `primitive_index` on a non-indexed draw is `vertex_index / 3`, so the
  attribute path is the same either way.
- **A device without the feature** gets a fallback: `gpu::shader_for`
  keeps lines tagged `//@prim` only with the feature and `//@noprim` only
  without, so it lives three tagged lines away from the fast path in the
  same file -- the vertex stage passes `vertex_index / 3` as a flat varying
  and the fragment reads that instead of the builtin. The draw has to be the
  corners then, and `render::Pass` asks the device the same question
  (`gpu::has_primitive_index`) once at creation and picks the corners path
  per draw. Exercised by forcing that function to `false` for one build:
  the flat and wireframe frames came out identical to the fast path's, at
  52.5 it/s on the benchmark (the corners draw plus the fragment-side
  fetch), so a machine without the feature draws correctly and no faster
  than before. None has been seen.
- The attribute bind group is visible to both stages now
  (`mesh_attrs_layout`), which is what the first build tripped on.

The expanded-corner `geometry_buffer` stays: the wireframe draw, the
facet-id and hemicube passes (`vertex_index / 3`) and the per-facet shadow
query still read it. No memory was added: `shared_positions` was already
there for the shadow pass.

## Measured

Same benchmark, three runs, median:

| | before (caster skip) | indexed main pass |
|---|---|---|
| render pass | 13.14 ms | **8.24** (8.26 / 8.24 / 8.21) |
| shadow pass | 7.57 | 7.42 |
| text | 2.16 | 3.02 |
| span | 15.46 | 10.42 |
| frame (wall) | 15.99 | 10.70 |
| it/s | 62.5 | **93.4** (93.4 / 93.4 / 92.6) |

From the morning's 41.6 it/s: **+125 %**, no proxies, no change to the
physics or to any shadow.

## Verified

- `test_far_sun`, `test_pcf_filters`, `test_shadow_layers`,
  `test_facet_shadow` (the shadow path through the main pass, identical
  margins), `test_load_mesh`, `test_mesh`, `test_stubs`; `cargo test
  --release` with and without the `python` feature (the one failure is the
  pre-existing `buffer_tests` race).
- Four frames of the crater plane and `ico3`, looked at: flat shaded;
  wireframe (corners path); smooth; and a per-facet colormap whose values
  are each facet's centroid x, which must come out as a left-to-right ramp
  whatever order the file lists facets in -- it does. (A ramp over facet
  *index* looked like a checkerboard first: the OBJ lists every cell's first
  triangle, then every cell's second, so facet `i` and `i + 1024` share a
  cell. The file's order, not the mapping.)

## Where it stands

The frame is 10.7 ms; 100 it/s is 10.0. Two more single-run variants to
find the critical path:

| | shadow | render | frame | it/s |
|---|---|---|---|---|
| as committed (8192, Blender axes) | 7.5 | 8.2 | 10.70 | 93.4 |
| no text pass at all (`axes.style = "off"`, no HUD) | 8.1 | 8.1 | 10.69 | 93.6 |
| `shadows.resolution = 4096` | 4.9 | 7.6 | 9.42 | **106.2** |
| `shadows.resolution = 2048` | 4.4 | 7.4 | 9.10 | 109.9 |

- **The text pass costs nothing.** Its 3.0 ms figure is waiting: a pass's
  number runs from its vertex stage starting to its fragment stage ending,
  and the text pass is queued behind the blit, which waits for the main
  pass. With no text pass the frame is the same to 0.01 ms. Not a lever.
- **The shadow pass is on the critical path after all**: the main pass's
  fragment stage samples the map, so it cannot start until the layers are
  stored, and at 8192 a layer is 268 MB of `Depth32Float` to store. Halving
  the resolution takes 2.6 ms off the shadow pass and 1.3 ms off the frame;
  2048 barely improves on 4096, so ~4.4 ms of the pass is the geometry
  (1.6 M vertices and 3.1 M triangles per body per layer), not the store.
- The physics query is unmoved by 4096: `test_facet_shadow` at 4096 gives
  the same counts as at 8192 (48/14724 lit-but-blocked, 52 dark-but-lit,
  worst angle 1.39 %). With per-body layers the texel on Didymos goes from
  10 cm to 21 cm and on Dimorphos from 2 to 4 cm, against ~1.5 m facets on
  the 3M models; and a layer drops from 268 to 67 MB. The bias follows the
  fit (`fit_shadow(…, resolution)`), so nothing else moves. **4096 is the
  default since this evening**, the user's decision on these numbers; at
  it, three runs: 105.3 / 106.6 / 97.3 it/s (shadow 4.9-5.8 ms, render
  7.5), and the seven tests -- config panel, bindings, stubs, facet shadow,
  far Sun, PCF, layers -- pass. From 41.6 it/s this morning: **2.5×**.
- The render pass, 7.6-8.2 ms: 6.3 M facets, about 2.3 ms of it MSAA.
