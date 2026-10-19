# 2026-10-07 — chunked level of detail, and what a vertex carries

Asked: `afc.py` ran at 10 fps with the 12.9M-facet Mars; make it much faster,
240 fps if possible; draw only what the camera sees, and size the shadow map
to it.

Drawn whole, Mars cost ~45 ms of camera pass and ~55 ms of shadow pass a
frame on the M1 Pro, whatever was in view: every vertex goes through the GPU
before the off-screen ones are dropped, and in AFC's 5.5 deg field nearly all
of them are.

## The tree (`src/app/lod.rs`)

- A binary tree over the facets, split by their centres along the longest
  axis down to 4096-facet leaves. Built once per flat mesh of 16,384 facets or
  more, on a thread of its own: 4.5 s for Mars (8191 nodes, 27.1M triangles at
  every level), 0.1 s for the 196,608-facet Deimos. Until it is in, the mesh
  is drawn whole.
- Every frame, `Lod::select` walks it: a node outside the frustum goes, one
  whose normal cone faces away goes, one whose triangles project to at most
  `shading.lod_pixels` is drawn whole, otherwise its children are tried.
  Ranges of adjacent nodes are merged into one draw; each draw's first
  triangle goes in four bytes of immediates, for `facet_of`.
- Every drawn triangle stands for an original facet (`facet_of`), whose
  colour, mode, value and normal it shows.
- Vertices renumbered in the order the tree first draws them, so a node's
  vertices sit together. In the mesh's own order a leaf touched 0.31 cache
  lines per vertex (packed: 0.09). No measurable change in pass time, but the
  vertex buffer is now uploaded from the build instead of a GPU copy of the
  shared positions.

### Simplification, and the holes

Inner nodes hold their two children's surfaces halved. Vertex clustering did
it first, and left holes where cells met and the triangles across them were
cut along the wrong diagonals: 238-1,500 pixels at 12:08:31 showed the
background through the planet. Now edge collapse, shortest first, each edge
at most `sqrt 2` times the children's size, the kept end at its own place
(so every level draws from the mesh's positions), refused when it would:

- break the link condition (join two parts that only touched),
- turn a triangle over, crush it flat, or tilt it more than 80 deg off the
  facet it stands for (a run of collapses each turning a triangle less than
  over folded one all the same),
- leave an edge longer than twice the cell.

Each node keeps **two surfaces**. The one passed up holds its outline at full
resolution, so two siblings meet edge for edge and their seam is simplified
in the parent like the rest. With both simplified freely the seams never
met again and piled up inside every parent. The one drawn has the outline
simplified too, along itself: held, a coarse node was mostly outline
(2,000-5,000 triangles where its inside needed 1,500).

Neighbours drawn at different levels, or with their outlines simplified
apart, no longer meet edge for edge: every drawn surface hangs a **skirt**, a
strip 8 cells down the surface's own normal (not the radius, which on Deimos
leans far off it) and flared 3 cells out under the neighbour, shaded as the
facet above.

Checked by rendering with a red background, so a hole is red and a shadow
black: **0 holes** at 12:08:31 (238 before the two surfaces, 1,501 with
clustering).

### Shadow casters

The shadow layers are fitted to the spheres of the nodes the camera draws.
Casters drawn coarser than the receivers (`shadows.lod_pixels` 3 against the
camera's 1.5) shadowed them wherever the coarse surface passed above the fine
one, across crater floors: 229 pixels black at 12:08:31 that the full mesh
lights. Now a shadow cut judges the nodes in the camera's frustum by the
camera's own measures (`View::seen`), so where the camera looks the map holds
the very triangles it draws, and only casters outside it are coarser: **19**
such pixels, against 8 the other way round. What is left are single pixels
where a simplified triangle's facet faces away from the Sun, sampling noise
between two point-sampled images.

## What a vertex carries

`mesh_shadow.wgsl`'s vertex output was 160 bytes. A tiled GPU writes every
vertex's outputs to memory and reads them back per tile, so at 2M triangles
a frame that traffic was most of the pass.

- The body's constants -- flags, shadow layer, normal matrix, the law and its
  numbers, 80 bytes, the same on every vertex -- moved out: the fragment stage
  reads them from the instance buffer itself, bound as storage (`Body`, group
  5 binding 2). Render pass: 2.9 to 1.3 ms far, 5.8 to 3.0 ms close.
- A lean build of the shader (`//@lean` / `//@full` lines, `shader_for`) for a
  flat mesh drawn indexed without the wireframe -- every large body -- whose
  surface the fragment stage reads by facet: a vertex carries its clip and
  world positions, 28 bytes. About 10 % more. Pixel for pixel the same image.

## Measured

`afc.py`'s loop, 1020 px, M1 Pro, `gpu_timing` span (first timestamp to last):

| | p10 | p50 |
|---|---|---|
| far, 05:52 on, a minute a frame | 1.6 ms | 3.6-4.9 ms |
| closest, 12:08 on, 2 s a frame | 4.0-4.2 ms | 8.2-8.8 ms |

The medians are not the code's. Three apps were busy on the GPU (iTerm2,
WindowServer, a chat client), and a 10k-facet scene showed the same tail: a
2 ms median frame, 60 ms stalls on one frame in six, 183 fps at best. The
same settings measured 43-131 fps run to run. The p10 is the frame the GPU
gave to kalast alone: ~600 fps far and ~240 at closest approach on a quiet
machine.

Image against every facet drawn (`shading.lod = False`), 12:08:31, mean
absolute difference after a one-pixel Gaussian (AFC's PSF is under 2 px):
0.25 % at `lod_pixels` 1.5, **0.35 % at 2** (the default), 0.76 % at 3.

Also: `hold_off_app_nap` (`src/app/macos.rs`) opts the process out of App
Nap while it runs, which throttles an occluded window's app; not the cause of
the noise above, as it turned out.

## Open

- Closest approach is still ~4 ms of GPU at best, a shadow pass and a
  render pass of about 2 ms each. The next step for terrain self-shadowing is
  likely horizon maps (a precomputed horizon per facet and azimuth), which
  would leave the shadow pass only the moons.
- A simplified triangle shows one facet's colour and normal, not their mean
  over what it covers. Fine at about a triangle per pixel; an averaged
  attribute per LOD triangle would be the exact thing.
- The LOD's GPU buffers for Mars: 27.1M triangles, ~435 MB, built even with
  `shading.lod` off.
- `KALAST_LOD_STATS=1` prints each cut's size and a node histogram every 60
  frames; a development aid, undocumented.
