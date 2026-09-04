# Shadow map: four bugs, and a calibration problem left open

A day of chasing visible artefacts on the Mars swing-by quick-look and the
crater self-shadow example. Four defects found and fixed, one of them wrong
physics rather than a wrong picture. The underlying bias calibration is **not**
solved, and is the most important thing here.

## 1. The layer extent was reverse-engineered, and wrong

`fit_light_view_proj` computed the shadow layer's half-extent, built the
matrix, and threw the value away. The caller recovered it as
`1.0 / view_proj.x_axis.x` -- but `view_proj` is `projection * view`, so that
element is `R[0][0] / side` and inverting it gives `side / |R[0][0]|`, correct
only when the light happens to look down an axis.

Measured at the swing-by geometry:

| layer | recovered | actual | error |
|---|---|---|---|
| Mars | 1.0 (fallback) | 3,788 km | **3,788x** |
| Phobos | 9.5e5 km | 227 km | 4,200x |
| Deimos | 7.3e4 km | 14 km | 5,100x |

Mars's `R[0][0]` fell below `f32::EPSILON`, so it took the `else { 1.0 }`
branch: a 3,788 km body biased as though it were 1 km across, giving a 0.35 m
normal offset. The whole disc rendered with self-shadow acne that no automatic
setting could clear, and `shadow_per_body = False` did not visibly help
because the single scene layer was mis-sized too.

`fit_light_view_proj` now returns a `LightFit { view_proj, side, near, far }`.
Regression test `light_fit_reports_its_own_extent_from_every_direction` fits
from five light directions, including ones that drive `R[0][0]` near zero.

**Effect:** disc mean 170.5 -> 229.5, dark-pixel fraction 8.80% -> 0.07%.

## 2. `facet_shadow` used scene-wide bias -- wrong physics

`shaders/facet_shadow.wgsl` opens by stating it matches the render "with the
same projection and the same depth bias". That became false when per-body
shadow layers landed: the compute pass kept reading `Globals`, which is fitted
to the **whole scene**, while the render moved to per-layer bias.

One scalar across a 403x spread of body sizes. On a 6.6 km Deimos beside a
3,536 km Mars, the scene-fitted 3.45 km normal offset pushes every sample more
than half a body-radius along its normal, so nothing registers as occluded:

| body | before | after |
|---|---|---|
| Mars, 3536 km | 47.6% | 42.7% |
| Phobos, 114 km | 41.9% | 51.3% |
| **Deimos, 6.6 km** | **0.55%** | **45.6%** |

**This feeds the TPM.** `facet_shadow` is the insolation occlusion term, so
0.55% self-shadowing on Deimos was absorbed flux, not a rendering detail.

**Anything that consumed `facet_shadow` before 4 September is affected** --
the Didymos phase-1/phase-2 spin-ups, the view-factor work, and the Deimos
preroll `tiri_deimos_photometry.py` relies on. On the Didymos/Dimorphos mutual
eclipse of 2027-01-23 the shadowed count moves -13.8% (Didymos) and -14.1%
(Dimorphos), which raises absorbed flux and should make both slightly warmer
than the existing spin-ups say.

## 3. The debug light cube corrupted the scene

`shaders/light_render.wgsl` declares its own copies of `Globals` and `Light`.
A second declaration of the same buffer does not fail to compile when a field
is missing -- it silently shifts every field after it.

- `Globals` lacked `srgb_mode` and `gamma`, so `light_cube_scale` read
  `gamma` (2.2) and the cube came out ~9x oversized.
- `Light` lacked `view_proj_layers` and `layer_bias`, added with per-body
  layers, so `pos` was read out of the middle of a matrix.

The cube was therefore drawn at a garbage position covering ~9% of the screen,
and with `debug_light_cube_show = True` the crater example lost nearly half its
lit surface -- independent of `light_cube_scale`, and present with shadows
disabled entirely, which is what ruled out a shadow cause.

Both structs now mirror the Rust layouts. The cube also gets a
`depth_write: false` pipeline and is drawn after the bodies, so a debug marker
cannot occlude geometry again. Verified: cube on vs off is 0 px differ, and it
lands at the Sun (centre 400,298 against a predicted 400,300).

**Regressed at `452ac5d`.** Bisected: lit fraction 9.95% before, 5.87% after.

## 4. PCF: two artefacts, opposite directions

Both only at `shadow_pcf > 0`; `shadow_pcf = 0` output is bit-identical
throughout.

**Grey floor.** The normal offset was one texel diagonal while the kernel
reaches `N` texels, so off-centre taps compared against stored depth that far
along the surface and flipped. Fixed by scaling the offset with the kernel:
`lb.x * (1 + shadow_pcf)`. Floor leak at `N = 4`: 7,952 -> 388 px, and now flat
across `N` rather than growing with it, which is the signature that the
mechanism is right.

**Acne on the lit wall.** A receiver tilted in light space self-shadows under
the filter -- 39,219 px darkened at `N = 4`. Fixed with a per-tap
receiver-plane depth bias, gradient derived **analytically from the facet
normal**. A first attempt using `dpdx`/`dpdy` made things worse (facet-edge
leak 1,279 px against 215) because screen-space derivatives are meaningless
across a facet boundary and a flat-shaded mesh is nothing but boundaries.
Clamped at `GRAD_MAX = 1e-4`: the gradient is only valid where the occluder is
the receiver, and unclamped it pushed rim-shadowed floor taps into light,
215 -> 3,892 px.

Final, both cameras, `shadow_pcf = 4`: lit-wall darkened 39,219 -> 710 px,
floor leak 7,952 -> 388 px, lit-wall roughness 1.860 -> 1.217 against 1.256 at
`shadow_pcf = 0` -- i.e. now smoother than unfiltered, which is what a filter
should do.

Residuals, not zero: 710 px on the wall, 388 px in the floor.

## Still open: the automatic bias is not calibrated

The crater has an **exact** answer -- the depression is 63.28% of facets by
count (1296/2048), and at grazing Sun every one of them is rim-shadowed, while
the flat plane outside has nothing to occlude it. So `facet_shadow` must
converge to 63.281%.

| configuration | shadowed at grazing |
|---|---|
| hand-pinned bias + pinned Sun frustum | **63.281%** exact |
| automatic, before today | 47.07% |
| automatic, now | 38.57% |

**Automatic bias leaks 16-25 points on a case with a known answer, and today's
work made that worse, not better.** The per-layer fix (§2) is right and is what
moved it -- `Eye::fit_projection` fits from a bounding sphere with texel
snapping while `fit_light_view_proj` uses AABB corners, so they disagree even
for a single body -- but the destination is still wrong.

A `tan(theta)` slope factor was tried and **rejected**: `1 - N.L` saturates at
1 while the required bias goes as `tan(theta)`, which is real, and it fixed the
Mars/Phobos comb teeth -- but it scored 37.60% on the crater, worse than the
47.07% it replaced. Parked, not committed.

The crater is the right harness for this: it has an exact target, it is fast,
and it is already in the repo. Calibrate against it rather than by eye.

## Not a bug: the fullscreen stall

Toggling into macOS native fullscreen blocks in `acquire drawable` for
1001 ms and 3725 ms -- 1001 being Metal's `nextDrawable` timeout exactly.
Launching fullscreen instead costs one 104-588 ms stall at startup. Documented
under `fullscreen` in `CONFIG.md`, along with why `shadow_pcf` amplifies it and
`vsync` does not explain it. Not fixed; `config.fullscreen = True` avoids it.

Also measured while chasing it, and worth keeping: PCF cost is per-fragment,
so it scales with pixel count. At 800x600 `shadow_pcf = 4` costs +2.5 ms; at
3024x1964 it costs +7.8 ms, nearly tripling frame time (4.0 -> 11.8 ms). Any
PCF benchmark taken at the default window size understates it by ~3x.
