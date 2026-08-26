# Shadow-mesh proxies: how coarse can the occluder be?

The shadow pass re-renders every body a second time, from the light's point
of view, into the shadow map. At full resolution that doubles the geometry
cost of a frame. But the shadow map only answers *"is this fragment occluded
from the light?"* — it never carries per-facet data — so it can use a coarser
mesh than the camera view without touching any science output.

That is the key difference from LOD: swapping the **render** mesh would
invalidate facet-indexed results (`tmp_surf.csv`, `rad_all.csv` columns are
facet indices into one specific topology). Swapping only the **shadow** mesh
cannot, because nothing reads facet identity from it.

## API

```python
app.simulation.load_mesh(
    path=".../g_01165mm_spc_obj_didy_0000n00000_v003.obj",   # 3.1M facets, rendered
    mat=mat,
    flatten=True,
    shadow_path=".../g_01165mm_..._decimated_100k.obj",      # 100k facets, shadow map only
)
```

Omit `shadow_path` (the default) and the main mesh is used for shadowing, as
before. The proxy inherits the same `flatten` setting and follows the body's
transform every frame.

Rust: `Simulation::load_mesh_with_shadow`, or set `Body::shadow_mesh`.

## Measurements

Apple M1 Pro, **release build**, `vsync = False`, 1020x1020, both bodies at
3.1M facets rendered. Scene held fixed at **2027-01-21T05:36:00 UTC** — the
Dimorphos-transit epoch from `pcf_shadow_comparison/`, chosen because it has
an actual cast shadow rather than just a terminator.

| Shadow mesh | Rate | Speedup | Pixels differing from full-res shadow |
|---|---|---|---|
| full 3.1M *(reference)* | 55.2 it/s | 1.00x | — |
| **100k** | **83.3 it/s** | **1.51x** | **9 / 1,040,400 (0.0009%)** |
| 10k | ~84 it/s | 1.52x | 223 / 1,040,400 (0.0214%) |
| 12-facet cube *(ceiling)* | 84.0 it/s | 1.52x | *(not a real occluder)* |

Error detail, against the full-resolution shadow as ground truth:

| | 100k | 10k |
|---|---|---|
| max abs diff (any channel) | 154/255 | 232/255 |
| mean abs diff, whole frame | 0.0003/255 | 0.0055/255 |
| mean abs diff, lit body pixels only | 0.0043/255 | 0.0797/255 |
| pixels differing by >8 | 6 | 121 |
| pixels differing by >64 | 1 | 22 |

The differing pixels sit on shadow boundaries — individual pixels flip fully,
which is why max diff is large while the count stays tiny.

## Recommendation: 100k, and there is nothing left below it

The last row is the important one. Substituting a **12-facet cube** — an
absurd occluder, standing in for "the shadow pass costs nothing at all" —
reaches 84.0 it/s. A 100k proxy already reaches 83.3. So **100k captures ~99%
of everything the shadow pass has to give**, and the entire remaining
headroom in that pass is under **1%**.

That answers the obvious follow-up: is it worth writing a decimator that
matches the *silhouette* with far fewer facets than 100k? No. However clever
it is, it is competing for ≤1%, and 100k already costs 9 pixels of error.

Measured across repeated runs (single runs vary by several it/s, and a first
run after a rebuild can be much slower — discard it):

| Shadow mesh | run 1 | run 2 |
|---|---|---|
| 100k | 83.5 | 83.1 |
| 12-facet cube | 83.8 | 84.2 |

Indistinguishable. An earlier version of this note claimed 10k bought 3% over
100k; that was run-to-run noise, not signal. 10k, 100k and 12 facets all sit
at the ceiling.

**Use 100k. Do not generate coarser levels, and do not build a silhouette
decimator.**

## Caveat

One epoch, one camera geometry. The error is concentrated on shadow edges, so
configurations with more shadow boundary visible (a large cast shadow filling
the frame, or a grazing terminator) would show proportionally more differing
pixels. If a future scene leans hard on shadow detail, re-run this comparison
for that geometry rather than assuming 9 pixels.

Reproduce by rendering the same fixed epoch with `shadow_path` set to each
candidate and diffing the exported PNGs against the no-`shadow_path` render.

## Where the remaining performance actually is

With the shadow pass effectively free, the ~84 it/s floor is the **main
render pass** drawing 3.1M facets. And at this geometry that is heavily
oversampled: the two bodies cover **71,785 of 1,040,400 pixels**, so

| | facets per body pixel |
|---|---|
| 3.1M mesh | **43.8** |
| 100k mesh | 1.39 |

44 triangles per pixel. The framebuffer cannot resolve any of it. Dropping
the *render* mesh to 100k (which is roughly one facet per pixel) measures:

| Render mesh | Rate | Differing pixels vs 3.1M | Mean diff on lit body |
|---|---|---|---|
| 3.1M | 55.3 it/s | — | — |
| 100k | **305.2 it/s (5.5x)** | 40,771 (3.92%) | 1.285 / 255 |

So there is a **5.5x** sitting in the render mesh — far more than the shadow
pass ever had. But note the error is a different class from the shadow swap:
40,771 differing pixels versus 9. Small per pixel (mean 1.3/255, only 64
pixels differ by more than 32) but genuinely visible in aggregate, and this
is the mesh that carries facet-indexed data. Two rules follow:

- **Frames carrying per-facet data** (`tiri_data.py`-style colormaps of
  temperature or radiance): the render mesh must stay full-resolution, and
  ~84 it/s is the floor. Nothing further to win without changing the science.
- **Pure-geometry frames** (diffuse + shadow only, no per-facet data): 100k
  is available for 5.5x, at 1.3/255 mean error.

The 43.8 figure is distance-specific — Hera is 25.8 km out here. Break-even
(~1 facet/pixel with the 3.1M mesh) would be around **6.6x closer, ~4 km**.
Inside that, full resolution earns its keep; outside it, it is paying for
detail the pixels cannot show. That is the honest argument for
distance-driven LOD later, and the number to base it on.

## Release build flags

`maturin develop --release` is worth 2.3x over debug at 3.1M facets, and much
more when exporting (debug 22.6 -> release 53.1 it/s), because the per-pixel
export loops are what debug Rust punishes hardest.

Beyond the default `opt-level = 3` there is nothing to add. `lto = "fat"` +
`codegen-units = 1` was measured and gave **no improvement** — render loop
52.7 vs 55.1 it/s, mesh load+flatten 1.07 vs 1.10 s, both inside run-to-run
noise — while pushing the build from ~59 s to ~87 s. The render path is
GPU-bound, and the hot CPU loops are already vectorised at `opt-level 3`.
Recorded in `Cargo.toml` so it is not re-tried blindly.
