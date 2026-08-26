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
| full 3.1M *(reference)* | 55.1 it/s | 1.00x | — |
| **100k** | **81.9 it/s** | **1.49x** | **9 / 1,040,400 (0.0009%)** |
| 10k | 84.9 it/s | 1.54x | 223 / 1,040,400 (0.0214%) |

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

## Recommendation: 100k, and do not go coarser

100k buys **1.49x for 9 pixels**. Dropping to 10k buys only **3% more**
(1.49x -> 1.54x) while multiplying the error **25x**. Past 100k the shadow
pass is no longer the bottleneck, so there is nothing left to win — coarser
proxies trade accuracy for essentially nothing.

No need to generate custom decimation levels below 100k.

## Caveat

One epoch, one camera geometry. The error is concentrated on shadow edges, so
configurations with more shadow boundary visible (a large cast shadow filling
the frame, or a grazing terminator) would show proportionally more differing
pixels. If a future scene leans hard on shadow detail, re-run this comparison
for that geometry rather than assuming 9 pixels.

Reproduce by rendering the same fixed epoch with `shadow_path` set to each
candidate and diffing the exported PNGs against the no-`shadow_path` render.

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
