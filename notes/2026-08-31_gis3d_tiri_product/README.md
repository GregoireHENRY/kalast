# The GIS3D TIRI product: simulated Didymos-system radiance at 2027-01-21

Seven simulated TIRI FITS of the Didymos system at 2027-01-21T05:36 UTC, one
per filter, in physical band-integrated radiance, plus 168 context frames
across +/-6.5 h.

This was a separate piece of work from the conduction solvers it was built on,
and it lived inside `2026-08-27_conduction_solvers/` as section 7.7 for longer
than it should have. Moved here intact; the solver note keeps a pointer.

**Depends on**: the conduction solvers (`2026-08-27_conduction_solvers/`) for
the thermophysical model, and the shadow map for eclipse geometry.

**Superseded in part**: the delivered product carries no self or mutual
heating, and its Dimorphos temperatures are low as a result -- quantified in
`2026-08-31_view_factors/`, which also fixes a view-factor bug that made the
secondary's mutual term 4.6x too small. See "What this product still needs"
at the end.

---

## Results, in one place

| | value |
|---|---|
| Epoch | 2027-01-21T05:36:00 UTC |
| Filters | 7 (`a`-`g`), band-integrated, `BUNIT = W m^-2 sr^-1` |
| Didymos in frame | ~133 px across, 5,889 camera-facing facets |
| Didymos sampled | 100 % in FOV, 83 % resolved (rest sub-pixel) |
| Didymos surface T | driven by the eclipse: **-93.7 K**, -78.9 % band radiance |
| Dimorphos surface T | driven by **self**-shadowing: **-116.2 K** |
| Context frames | 168 over +/-6.5 h (1.14 Dimorphos orbits) |
| Mutual events captured | 3 -- two umbra passages (-5.7 h, +5.7 h), one shadow transit at epoch, each 93 min |
| Shape models | Didymos 13.1 m facets, Dimorphos 2.73 m facets (10k decimated) |
| Absent physics | self heating, mutual heating, thermal roughness |

The two bodies are dominated by **different** terms at this epoch: Didymos by
the eclipse, Dimorphos by its own self-shadowing, since it is at conjunction
and fully lit. That asymmetry is the headline result.

---

## Dimorphos's own thermophysical state

`tpm.py` is now parameterised by `BODY`, so the same script spins up either
body. Dimorphos needs no different physics, only a different grid: the grid
follows the *diurnal* skin depth, and Dimorphos is tidally locked at 11.37 h
against Didymos's 2.26 h, so its skin depth is sqrt(5) larger. 29 nodes to
5.54 m against Didymos's 34 to 6.17 m, and a stability limit of 703 s against
140 s. Three solar orbits took 644,850 steps at 1.46 ms/step, ~16 minutes.

One trap: `DIMORPHOS.orbit_period` is its **11.9 h orbit around Didymos**,
not a heliocentric year. Using it for the seasonal skin depth would build a
column centimetres deep. The script takes `DIDYMOS.orbit_period` for both,
which is the physically correct shared heliocentric period.

A second, harder one: mission kernels cover 2026-07 to 2027-07, and both
`spkpos` and `pxform` for Dimorphos fail with `SPKINSUFFDATA` before that.
Didymos has a Horizons ephemeris and a rotation model back to 2023;
Dimorphos has neither, so a multi-orbit spin-up cannot be driven from kernels
alone. Two approximations bridge it:

1. The Sun's direction is taken relative to Didymos. The bodies are 1.19 km
   apart at 1.5e8 km, so the directions differ by 8e-9 rad.
2. Dimorphos's body-fixed frame is extended backwards as a uniform rotation
   about its own spin axis at the true rate, anchored to the real orientation
   at the study epoch. Being tidally locked, its rotation *is* uniform to
   good approximation. Measured against the kernels where both exist, the
   synthetic frame drifts **0.013 deg/day** — 1.6 deg over 120 days.

The rotational phase would not matter even if it were wrong: after thousands
of rotations the column keeps no memory of its initial phase, and what a
spin-up delivers is the deep seasonal field.

## A frame bug the ablation caught

The first `self` runs put Dimorphos alone in the scene but still placed the
Sun using *Didymos's* frame, because `before_render` assumed body 0 was
always Didymos. The shadow map and the TPM were then in different frames: the
facets the renderer reported as shadowed were not the facets the physics
thought were lit.

It announced itself through an impossibility — `self` came out **colder** than
`mutual` (215.4 K against 239.9 K), when mutual is self plus an extra
occluder and can only be colder or equal. Worth recording as a diagnostic
pattern: an ablation whose terms are nested gives a free monotonicity check,
and it caught a bug that a plausible-looking number would have hidden.

A second version of the same class: with only one body in `state`, the shared
timestep was drawn from that body's stability limit alone (281 s instead of
55.94 s), so an ablation difference would have measured the timestep as well
as the physics. The grids are now built for every body regardless of which is
stepped.

## Two-body ablation at the study epoch

| body | term | facets | worst ΔT | mean ΔT | facets < −5 K | worst radiance |
|---|---|---|---|---|---|---|
| Didymos | self-shadowing | 4,105 | −12.4 K | −0.16 K | 44 | −36.5 % |
| | **eclipse** | 3,977 | **−95.9 K** | −1.34 K | 366 | **−80.2 %** |
| | both | 4,240 | −95.9 K | −1.49 K | 406 | −80.2 % |
| Dimorphos | **self-shadowing** | 7,791 | **−116.3 K** | −4.57 K | 1,853 | **−92.2 %** |
| | eclipse | 9,211 | −13.7 K | −0.63 K | 1,239 | −27.0 % |
| | both | 8,966 | −115.4 K | −5.20 K | 2,923 | −89.8 % |

The two bodies are dominated by *different* terms, and the reason is the
epoch. At 05:36 Dimorphos is at conjunction — between Didymos and the Sun,
fully lit — so its −11 K is not a live eclipse but residual memory of its own
total eclipse six hours earlier, half a Dimorphos day back. What dominates
Dimorphos instead is self-shadowing, at −116 K: it is more irregular than the
primary and rotates five times slower, so a facet that falls into a
topographic shadow stays there long enough to lose more than a hundred
kelvin.

Evaluated three spins later the ranking shifts again — Dimorphos's eclipse
term reaches −37 K, because its second total eclipse (10:48–11:45) ends
forty minutes before. Which instant is chosen changes the answer completely,
which is the practical form of the §7.5b conclusion.

## Dimorphos spin-up: the staged plan

What was run is the first stage of a three-stage plan, and the later stages
are not done:

1. **Coarse seasonal initialisation** (done). Dimorphos is treated as
   co-located with Didymos with respect to the Sun -- the 1.19 km offset is
   8e-9 of the heliocentric distance -- but rotating on its own 11.37 h
   period. That gives the correct heliocentric distance history and the
   correct diurnal cycle, which is what sets the deep seasonal field. The
   synthetic tidally-locked frame described above is what makes it possible
   outside kernel coverage.
2. **Several correct orbits around Didymos** (not done). Once inside kernel
   coverage, run with the true relative geometry so the secondary's own
   eclipse history -- 57 minutes of totality every 11.37 h -- is imprinted
   before anything is measured. Section 7.5b shows those events leave
   residues that partially accumulate, so a state that has never seen one is
   not the right starting point.
3. **Phase 2** (done, but starting from stage 1 rather than stage 2). The
   binary mutual effects at high fidelity.

Stage 2 is the gap. The current phase 2 segment covers 20.4 h, which contains
two total eclipses of the secondary, so it is not starting cold -- but its
restart state carries none. Two or three Dimorphos orbits of stage 2 would
settle that, and at 7 ms/step it costs seconds. This is also where the
geometric eclipse-window optimisation starts to matter, since a longer
segment spends proportionally more time outside any mutual event.

## The facet-index buffer

`sim.request_facet_id()` / `sim.facet_id_map()`, backed by
`src/app/facet_id.rs` and `shaders/facet_id.wgsl`. The scene is rendered a
second time through the same view matrix into an `R32Uint` target holding
`1 + offset + facet` per pixel, 0 where nothing is drawn, and read back.

The alternative was to colour facets by radiance and read the colour image
back. That was rejected: colour quantises to 8 bits, is entangled with
lighting and tone mapping, and inverting a colormap to recover a physical
number is exactly the kind of step that silently loses accuracy in a product
whose pixel values *are* the deliverable. It would also have run straight
into the known per-body colour-mode bug, where `mesh.color_modes[:] = 1` has
no effect because the shader reads only the scene-wide `globals.color_mode`.

With an index buffer the mapping is exact and the lookup happens in numpy at
full float precision. Visibility comes free: the pass carries its own depth
buffer, so a facet absent from the map is one the instrument genuinely cannot
see — over the limb, outside the field of view, or hidden by the other body.

Implementation notes worth keeping:

- The facet index is `vertex_index / 3`, so the pass is only valid for
  **flattened** meshes where each facet owns its three vertices. Indexed
  meshes are skipped rather than given indices that look valid and are not.
- `@interpolate(flat)` on the index — interpolating it across a triangle
  would blend indices into meaningless values.
- Culling is off. A shape model with inconsistent winding would otherwise
  drop facets, and a missing facet is indistinguishable from an occluded one.
  Depth still decides what is in front.
- Bodies share one index space through a per-body offset supplied as a
  dynamic-offset uniform, so one readback resolves both which body and which
  facet.
- It reuses the renderer's existing view bind group rather than keeping a
  second camera uniform, which could silently disagree with what was drawn.

## Radiance, and a units error worth recording

`kalast/tpm/radiance.py`. Band radiance is a scalar function of temperature
once the filter is fixed, so it is tabulated once over 30-500 K and looked up
with `numpy.interp`; direct evaluation would mean a
`(n_facets, n_wavelengths)` Planck array per call. Interpolation error is
1e-4 relative, reported by the class rather than asserted.

Two reductions are defensible and differ by six orders of magnitude:

- **band-integrated radiance**, `integral B eps R dw`, in W/m2/sr;
- **band-averaged spectral radiance**, `integral B eps R dw / integral R dw`,
  in W/m2/sr/um.

**The first version of this work used the second, and it was wrong.** The
argument for it was that `Response_Fil-a..f` carried a factor 0.5 that
`Response_Fil-g` did not, so a quantity linear in `R` would misreport the
narrow filters -- and the normalised form cancels the response scale.

That factor does not exist. It came from dividing `Response_Fil-x` by
`Bolometer * Lens * Filter-x` at wavelengths where the denominator is nearly
zero, in columns quoted to four decimals, and taking the minimum of a
quotient that is quantisation noise there. Checked properly, on the median
rather than the extremes, the ratio is **1.000000 for all seven filters**:
`Response_Fil-x` *is* the product, an absolute throughput with peaks between
0.37 and 0.81.

The error announced itself observationally. Because a band *average* is
almost filter-independent, the wide band `g` came out no brighter than the
narrow `a` -- which is physically absurd for a 5x wider filter, and was
spotted on opening the files.

Settled against ground truth: a real calibrated TIRI product
(`data_calibrated/necp/.../tiri_cal_*.fits`) carries
`BUNIT = 'W m^-2 sr^-1'`. So the simulated frames must be band-integrated to
be comparable at all. In those units, at 345 K:

| filter | eff. wavelength | bandwidth `integral R dw` | L(345 K) |
|---|---|---|---|
| a | 8.43 um | 0.514 um | 9.24 W/m2/sr |
| b | 8.61 um | 0.669 um | 12.01 |
| c | 9.59 um | 0.598 um | 10.33 |
| d | 10.44 um | 0.382 um | 6.19 |
| e | 11.53 um | 0.431 um | 6.26 |
| f | 12.66 um | 0.361 um | 4.59 |
| **g (wide)** | 10.27 um | **2.707 um** | **43.66** |

`g` is 4.7x `a`, tracking the bandwidth ratio of 5.3x as it should.
`band_averaged` still exists for pipelines that quote radiance per unit
wavelength, and the ratio of the two is a useful diagnostic.

**Two lessons.** Do not characterise a ratio by its extremes when the
denominator passes through zero. And a physical cross-check -- "the wide
filter must read higher" -- caught in seconds what the derivation did not.

**No emission-angle cosine.** A grey Lambertian surface emits the same
*radiance* in every direction; the cosine enters only when integrating to a
flux. A pixel measures radiance and the projected area is already accounted
for by which pixels a facet covers. (`rad.py` carries a `cose` term because
it goes on to sum irradiance over the disk -- a different quantity.)

Reflected sunlight is omitted, having been checked rather than assumed: at
1.02 AU with A=0.07, reflected solar over 8-14 um is 0.032 W/m2/sr against
90.4 emitted at 345 K -- 0.04 %, rising only to 0.16 % at 250 K. The argument
closes itself, since reflected sunlight exists only on lit facets, which are
the warm ones.

## Why Dimorphos self-shadows so much more than Didymos

A surprise from the ablation: Dimorphos's self-shadowing costs it -116 K at
worst, against -11 K for Didymos, and at the study epoch it exceeds the
mutual eclipse. Two real effects and one caveat.

**More of its surface is shadowed.** Median day-side self-shadowed facets
over the segment: 739 for Dimorphos against 118 for Didymos, of 10,000 --
6.3x. It is the more irregular body relative to its size.

**Each shadowed facet stays shadowed far longer.** Dimorphos is tidally
locked at 11.37 h against Didymos's 2.26 h, so topography that blocks the Sun
blocks it for five times as long in absolute time. The worst Dimorphos facet
would peak at 324 K unshadowed and reaches only 271 K, and stays more than
5 K below the no-shadow run for 15.2 h. Its grid works *against* the effect
-- a 6.76 mm first layer against Didymos's 3.02 mm is more thermal mass in
the surface node, so a slower response -- and the drop is still an order of
magnitude larger.

**The caveat: the two are not compared at equal ground resolution.** Both
meshes carry 10,000 facets, but Didymos is 780 m across and Dimorphos 170 m,
so mean facet scales are **13.1 m and 2.73 m** -- Dimorphos is resolved 4.8x
finer. A finer mesh resolves more topography and therefore more
self-shadowing, so some unknown part of the 6.3x facet-count ratio is
resolution rather than shape. The *duration* argument is unaffected, being
purely rotational, and the ranking is unlikely to reverse -- but the
comparison should not be quoted as a shape difference without re-running
both at matched ground scale. Worth doing when the full-resolution meshes
are used.

## The product

`examples/hera_didymos/tiri_fits.py` writes seven FITS files, one per TIRI
filter, 1024x768 to match the real detector, `BITPIX=-32`, `BUNIT` in
W/m2/sr/um. Headers carry the model provenance — grid, stencil, solver,
spin-up length, which shadowing terms are on, that mutual and self heating
are **not**, thermal inertia, albedo, facet count, and the two methodological
choices above.

At the study epoch Hera is 25.8 km from Didymos, so the primary spans ~133
pixels and the pair fills 1.56 % of the frame. Dimorphos's shadow is an
obvious dark ellipse ~30 px across, and it is far more striking in radiance
than in temperature, which is the T^4 sensitivity made visible.

The temperature state is taken from a step landing 19 s from the epoch —
`tpm_phase2.py` saves that exactly, rather than letting the FITS pick from
the 280 s snapshot series, which would have been a third of the time the
shadow spot needs to cross a facet.

## Context frames for the delivered FITS

`examples/hera_didymos/tiri_movie.py` and `tiri_movie_compose.py`. The FITS
are a single instant, and a single instant of a binary in mutual event is
hard to read, so the same scene is rendered across +/-6.5 h of the study
epoch -- 13 hours, 1.14 Dimorphos orbits -- through the real TIRI pointing.
Three frames per epoch: diffuse for geometric context, surface temperature,
and the TIRI wide band that the FITS actually carry.

168 frames at a 280 s cadence, 10.5 s of wall time. The cadence is not
chosen: it is the phase 2 snapshot interval, so every frame is a state the
model computed rather than an interpolation between two.

The sequence contains all three mutual events of that orbit, and their
durations come out as §7.5b predicts from geometry alone:

| event | window | duration |
|---|---|---|
| Dimorphos in Didymos's umbra | −6.41 to −4.94 h | 93 min |
| **Dimorphos's shadow on Didymos** | **−0.74 to +0.74 h** | **93 min** |
| Dimorphos in Didymos's umbra | +4.93 to +6.41 h | 93 min |

The two umbra passages sit half a Dimorphos orbit either side of the shadow
transit, as they must: the secondary is between Sun and primary at the study
epoch and behind it half an orbit away.

Two presentation decisions worth keeping. The scales are **fixed** across the
sequence -- autoscaling makes a cooling body look constant, which is the one
thing the movie exists to show. And the crop is a single box computed from
the whole sequence rather than per frame, so the bodies move within a fixed
field instead of appearing still while the frame moves around them. The
export writes full 1024x768 frames and the crop happens in a separate
compose step, so the sequence can be recomposed without re-rendering.

### Two bugs behind one symptom, and a check that was not a check

Reviewing the diffuse frames, an apparent extra eclipse of the secondary
showed up where the geometry has none. It turned out to be **two independent
faults**, and the first investigation found only the harmless one.

#### Fault 1: frame filenames sorted as strings

`FrameExporter` wrote `{export_dir}/{N}.png` unpadded, so any file browser,
viewer or shell glob sorts them 0, 1, 10, 100, ..., 109, 11, 110. In that
order the 40 true umbra frames scatter into **14 separate clusters**, so
stepping through the directory shows fourteen apparent eclipses rather than
two. Fixed: `FrameExporter` now writes `{N:06}.png`, which sorts correctly
and matches ffmpeg's `%06d`. The resume scan parses the stem as a number, so
directories from earlier runs still resume.

The tell that should have been noticed: the temperature and radiance frames,
written by the example itself with `f"{k:04d}.png"`, were correctly ordered.
Only the renderer's own exports misbehaved.

#### The check that was not a check

Having fixed the ordering, the frames were tested for spurious darkening by
sampling Dimorphos's *predicted* pixel position and taking the **peak**
brightness in a 44 px box. That returned 1.000 everywhere outside the umbra,
and the conclusion drawn was that nothing was wrong.

The metric was blind to the actual defect. Peak brightness stays 1.000 while
*half the body* is black -- one saturated pixel is enough. The evidence was
already in the same table: the *mean* column read 0.0837 at −3.69 h against
~0.16 either side, exactly half, and it was not read. Re-tested with a count
of lit pixels rather than a peak, the artefact is unmissable.

**The lesson is the metric, not the bug.** A statistic that cannot fall when
the defect is present is not evidence of absence. Peak was the wrong summary
for a partial occlusion; lit area is the right one.

#### Fault 2: the shadow frustum was centred on the wrong point

`fit_projection` sized the light's orthographic box from `bounds.radius()` --
the bounding-sphere radius -- for a good reason: that radius is invariant as
the light rotates, so the shadow map keeps a constant world-per-texel scale
instead of breathing every frame.

But the sphere is centred on the **bounds**, while the frustum is centred on
the light's **view axis**, which passes through whatever the frame is aimed
at. For a single body those coincide. For a binary they do not: with the
light aimed at Didymos, Dimorphos sits up to 1.15 km off-axis and its far
edge reaches 1.246 km against a half-width of 1.056 km.

Geometry outside the frustum is clipped and never writes depth, and samples
outside the map read as shadowed. So part of Dimorphos went black, with a
hard straight terminator -- the signature of a clip plane, not of topography.
Measured over one Dimorphos orbit at 1-minute cadence: **37 of 131 epochs
(28 %)** put part of the secondary outside the shadow map.

**This was not only a rendering fault.** `facet_shadow` reads the same shadow
map, so the thermophysical model received the same wrong lit fraction. The
rendered frames and the physics were wrong together, which is the hazard of
deriving a boundary condition from a rendering artefact -- and also the
reason it was visible at all.

#### Fixing it without losing resolution

The obvious repair -- grow `side` to `offset + radius` so the sphere is
covered -- works and was tried first. It is the wrong fix: at quadrature it
more than doubles the world-per-texel, coarsening the shadow map for *both*
bodies. Measured, it moved Didymos by up to 8 K at the study epoch, a body
that was never clipped in the first place. A correctness fix that degrades an
unrelated result is not a fix.

The right repair is to **offset the box rather than enlarge it**:
`Mat4::orthographic_rh` is given `centre ± side` instead of `±side`, with the
centre snapped to whole texels so the map does not shimmer as the box slides.
`side` stays `bounds.radius()`, so the texel scale is unchanged. After it:
0 of 131 epochs clipped, and Didymos differs from the original run by
**0.01 K on 9 facets** -- confirming it really was unaffected, rather than
assuming so.

#### What it changed

| | facets changed | worst ΔT |
|---|---|---|
| Didymos, at the study epoch | 9 / 10,000 | 0.01 K |
| Dimorphos, at the study epoch | 1,038 / 10,000 | −3.38 K |

Smaller than the visual artefact suggests, because the spuriously shadowed
facets were often near the terminator or already unlit, and the clipping
episodes are short against Dimorphos's thermal response. Every conclusion in
§7.7 survives; the numbers move by 1–3 K and the corrected table is the one
given there.

TIRI needs no substitute pointing over this window: it is nadir-pointed at
Didymos throughout, measured at 0.000 deg off-axis every hour.

---

## What this product still needs

Established after delivery, in `2026-08-31_view_factors/`:

- **Self and mutual heating are absent**, and they are not negligible on the
  secondary: +3.03 K in the mean on Dimorphos, +30 K on individual facets,
  with 6,578 of 10,000 facets moved by more than 1 K. Didymos is unaffected
  (+0.07 K mean, +2.0 K peak), so **the eclipse conclusions above, which are
  about the primary, stand.**
- **The Dimorphos shape model used here is a defective decimation.** One facet
  carries a self view factor of 0.996. Re-cutting from the clean 3.1M original
  with `preservenormal` removes it.
- Thermal roughness is still absent (`2026-08-27_conduction_solvers/` section 8).

Re-running the product with heating enabled is the outstanding task. Note that
it needs the phase-1 spin-up regenerating if the shape models are replaced,
since the saved state is indexed by facet.
