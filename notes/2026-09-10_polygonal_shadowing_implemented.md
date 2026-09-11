# Exact partial shadowing, implemented

Follows `2026-09-10_polygonal_shadowing_assessment.md`, which recommended it,
and the measurement in `examples/analytical/shadow_quantisation.py`, which
priced the problem at 0.7-40 mmag. `src/shadowing.rs` is the answer.

## What it does

Each facet's lit fraction as an **area**, not a sample. Project every facet
onto the plane perpendicular to the Sun, clip the target against everything in
front of it, and the surviving area is the lit area. Along the observer vector
instead and it is the visible area — same code, different direction.

```python
from kalast._rs import shadowing as sh
lit = sh.lit_fractions(vertices, indices, sun_direction)   # (n_facets,) in [0,1]
```

## No Clipper2, and why that is fine here

Brož uses Vatti's general polygon clipper through Clipper2 because his
polygons are arbitrary. **Ours are triangles**, which is a much easier
problem:

- a triangle is convex, and convex ∩ convex is convex;
- `A \ B` for convex `B` with `n` edges decomposes *exactly* into at most `n`
  convex pieces, `A ∩ H_1 ∩ … ∩ H_{i-1} ∩ ~H_i` for each edge `i`;
- so every operation is Sutherland-Hodgman against one half-plane — a dozen
  lines, exact, no dependency.

That matters practically: adding a C++ toolchain to the `maturin develop` path
on two machines is a real cost, and the pure-Rust Clipper2 port is far less
exercised than the C++ original. The cost is that the piece count can grow
with occluders; `MAX_PIECES` bounds it and `LitArea::overflowed` reports
hitting the bound rather than quietly returning a wrong area.

## The bug that mattered, and how it was found

**Deciding which facet is in front at the two triangles' own centroids is
wrong**, and it is the obvious thing to write. Against a converged ray trace
it disagreed by up to **0.435** on facets with `mu_i < 0.05`.

The reason is geometric: a facet near grazing incidence is nearly edge-on to
the rays, so its plane is steep in projection and its centroid depth says
almost nothing about the depth where it actually meets an occluder.

The fix is to decide **at the centroid of the overlap**, using Brož's own
back-projection (his eq. 14, `w = a u + b v + c`) fitted from each triangle's
three projected points. Max disagreement fell 0.435 → 0.09, and the grazing
band stopped being special.

Worth noting what the aggregate said while this was wrong: the area-weighted
lit fraction agreed to 2e-4, because the mis-handled facets carry 0.22 % of
the lit area. **A disc-integrated check would have passed.** Only the
per-facet comparison found it.

## Validation: the ray trace converges onto it

Testing an exact method against a sampled one needs care — a fixed tolerance
against a fixed sample count measures the sampler. The honest test is
convergence, and it is decisive:

| samples/facet | max per-facet gap | mean |
|---|---|---|
| 45 | 0.2392 | 0.00741 |
| 91 | 0.1593 | 0.00472 |
| 231 | 0.1024 | 0.00290 |
| 561 | 0.0670 | 0.00185 |
| 1225 | 0.0452 | 0.00122 |

Falling as roughly `1/n_div`, which is how finely a barycentric lattice can
place a shadow edge inside a facet. If the clipper were wrong the reference
would converge somewhere else and this would flatten out.

## What it is worth, in mmag

Same measurement as before, now with the clipper beside the 4-point sampling:

| shape | facets | shadowed | q4 rms | **exact rms** |
|---|---|---|---|---|
| mild | 320 | 6.0 % | 4.55 | **0.76** |
| moderate | 320 | 10.1 % | 18.11 | **3.56** |
| strong | 320 | 23.2 % | 40.61 | **8.57** |
| mild | 1280 | 5.8 % | 1.33 | **0.26** |
| moderate | 1280 | 10.6 % | 3.92 | **0.87** |
| strong | 1280 | 24.4 % | 10.75 | **1.78** |
| mild | 5120 | 5.7 % | 0.68 | **0.07** |
| moderate | 5120 | 10.5 % | 1.21 | **0.17** |
| strong | 5120 | 24.1 % | 3.88 | **0.46** |

Five to ten times better everywhere, and **0.07 mmag** at 5120 facets — under
Brož's 0.1 mmag.

**The `exact` column is not the clipper's error.** It is mostly the
*reference's*. Refining the reference drives it toward zero while the `q4`
column rises to its true value:

| reference samples | exact rms | q4 rms |
|---|---|---|
| 91 | 1.552 | 3.41 |
| 231 | 0.939 | 3.69 |
| 561 | 0.583 | 3.88 |
| 1225 | 0.389 | 3.99 |

So the ray trace is no longer a fine enough yardstick to measure the clipper
against — the same lesson as the conduction reference, one layer up.

## Timings, which the paper does not give

Neither Brož's paper nor the 96-page deck reports a single timing, so:

| facets | one direction |
|---|---|
| 320 | 7 ms |
| 1280 | 34 ms |
| 5120 | 156 ms |

Roughly linear over this range, thanks to the projected-bounding-box grid; the
naive form is `O(n^2)` clips. A light curve needs two calls per epoch (Sun and
observer), so a 1280-facet shape costs ~70 ms an epoch — a few seconds for a
full rotation, which is comfortably inside a fitting loop.

**This does not replace the GPU shadow map.** At 3.1M facets the map does one
rasterisation pass; this is CPU work that grows with facet count and would be
hopeless there. The thermophysical model keeps the map, where the quantisation
averages out over a rotation. Photometry gets this.

## Open

- **`theta_bar` in the scattering law is still unimplemented**, so the two
  halves of the photometry are now exact area × smooth-surface Hapke.
- ~~**Mutual events between two bodies are untested.**~~ **Measured** —
  `notes/2026-09-11_mutual_events_measured.md`. The clipper handles them,
  and the assessment's claim about partial visibility holds in this regime
  and only this one: binarised visibility costs 2.10 mmag rms in event
  against 3.01 for the shadow quantisation, where on a single body it was
  negligible. The two also **partially cancel** — 2.40 together against
  3.01 for shadowing alone — so fixing one in isolation buys less than
  measuring it in isolation suggests.
- The overlap-centroid depth test is exact for non-interpenetrating meshes.
  Interpenetrating geometry (two bodies actually touching) would need the
  overlap split where the planes cross.
