# View factors and radiative heating

Self- and mutual heating in the thermophysical model, from a GPU hemicube.

Started as a review of `view_factor_facets` inside
`2026-08-27_conduction_solvers/` section 9, and grew past what belonged there:
a new GPU kernel, a new module, four bugs, and a correction to a shape-model
claim. Moved out on 1 September; that note keeps a pointer, and the original
chronological write-up is preserved under "Detail" below.

---

## Status at a glance

| | state |
|---|---|
| Near-field view factors (CPU reference) | **done**, validated on two closed forms |
| GPU hemicube, self view factors | **done**, 0.20 ms/facet, closure 1.00001 |
| Mutual view factors (multi-body) | **done**, follows `(R/d)^2` across the orbit |
| Radiative heating in the TPM | **done**, `HEATING` ablation, monotonicity exact |
| Effect measured and decomposed | **done**, see below |
| Rebuild cadence chosen | **done**, every 5 steps, measured |
| Pre-flight recommendation tool | **done**, `examples/hera_didymos/heating_preflight.py` |
| Shape-model defect | **understood**: a decimation artefact, recipe fixed |
| Orbit-phase parametrisation | **not done** -- the one real optimisation left |
| Multiple scattering | **not done**, deliberately; bounded below |
| Re-run of the GIS3D product with heating | **not done** |

---

## 1. Why this was needed

`view_factor_facets` had a proximity guard returning **zero** when two facets
were closer than a threshold -- that is, exactly where the view factor is
largest. Neighbouring facets therefore contributed nothing to self-heating,
which is most of what self-heating is. It was also `O(N^2)` with no occlusion
test at all: 1.8 years and 38 TB at 3.1M facets.

## 2. What was built

**A hemicube per facet.** Render the scene into the five faces of a half-cube
sitting on the facet, each facet writing its own index; the depth test has
already resolved occlusion exactly, and the pixel count weighted by the
per-pixel delta form factor *is* the view factor. One set of renders yields a
whole row, so the matrix costs `O(N)` render passes rather than `O(N^2)` pair
tests. The near-field problem dissolves: a close facet simply covers many
pixels, with no singular `1/d^2` and no threshold to pick.

**Multi-body, in one shared index space**, with a per-body offset. One row
carries the self and mutual terms together and splitting them is a column
slice. Occlusion is shared, which is the point: a mutual eclipse blocks mutual
heating with no extra machinery.

**`kalast.tpm.heating`** consumes it. Sparse (measured 0.31 % dense, mean 63
nonzeros per row), assembled in chunks so the dense form is never built --
10,000 x 20,000 would be 0.80 GB. Two first-order terms enter the surface
balance:

    eps_i     sum_j VF_ij eps_j sigma T_j^4     thermal re-emission
    (1 - A_i) sum_j VF_ij A_j S_j               scattered sunlight

### Validation

| check | result |
|---|---|
| Closure on a sealed box | row sums **1.00001** |
| Isothermal black cavity, eps=1 | absorbs 459.305 vs 459.300 emitted, **+0.001 %** |
| Same, eps=0.9 | **-9.999 %**, exactly the `1-eps` single-bounce never re-absorbs |
| Against closed forms | **0.07 %** on perpendicular squares; delta form factors close to 3.3e-05 |
| Mutual vs `(R/d)^2` | effective radius 0.384-0.392 km across the orbit, against 0.39 |
| Monotonicity `none <= self <= mutual` | **exact**, zero facets cooled, both bodies |

`examples/analytical/cavity_heating.py` runs the cavity checks.

### Against closed forms

Three configurations have exact answers, and all three are checked.

**1. The delta form factors themselves.** Before any geometry is involved, the
per-pixel weights over a whole hemicube must sum to exactly 1. They converge
as `1/resolution^2`, which is the expected order for the midpoint rule:

| face resolution | `sum(dF)` | error |
|---|---|---|
| 32 | 1.000530 | 5.30e-04 |
| 64 | 1.000132 | 1.32e-04 |
| 128 | 1.000033 | **3.31e-05** |
| 256 | 1.000008 | 8.27e-06 |

128 is the production setting. If these weights do not close, nothing built on
them means anything, so this is the first check and it needs no scene at all.

**2. Perpendicular unit squares sharing an edge**, exact `F = 0.20004`:

| method | F | error |
|---|---|---|
| point-to-point, unsubdivided | 0.30339 | +51.66 % |
| **the old proximity guard** | **0.12434** | **-37.8 %** |
| CPU subdivision, level 2 | 0.23083 | +15.39 % |
| CPU subdivision, level 4 | 0.20749 | +3.72 % |
| CPU subdivision, level 6 | 0.20202 | +0.99 % |
| **GPU hemicube, 128 px** | **0.19990** | **0.07 %** |

Two things fall out. The guard that motivated this work is **37.8 % low on a
configuration with a known answer** -- not a subtle bias. And the hemicube is
an order of magnitude more accurate than the CPU reference at its deepest
subdivision, while being far cheaper: this is the case where the "faster and
more accurate" claim is checked against a number rather than against itself.

**3. Coaxial parallel discs**, across separations:

| separation | exact | numerical | error |
|---|---|---|---|
| 2.00 | 0.06859 | 0.06910 | 0.75 % |
| 1.00 | 0.19982 | 0.20108 | 0.63 % |
| 0.50 | 0.41525 | 0.41609 | 0.20 % |
| 0.25 | 0.63204 | 0.63259 | 0.09 % |
| 0.10 | 0.82699 | 0.83130 | 0.52 % |

This is the near-field case the guard was there to dodge: the exact value rises
to 0.83 as the discs approach, exactly where the guard returned zero.

**4. A sphere, for the mutual term.** A flat facet facing a sphere of radius
`R` at distance `d` sees `F = (R/d)^2`. Swept across the orbital range the
measured mutual view factor tracks it with an implied effective radius of
0.384-0.392 km against Didymos's 0.39 km -- see section 5, where the same
sweep is what exposed the far-plane bug.

On the CPU pairs, reciprocity `A_a F(a->b) = A_b F(b->a)` holds to 7e-8, since
those are area-to-area integrals. The hemicube's 2.9e-2 is a different thing
and is explained above.

Reciprocity is asserted in its **aggregated** form,
`sum_i A_i VF_ij = A_j rowsum_j`, to 2.9e-2. Per pair it is quantisation-
limited and mostly noise. The residual is **not** resolution -- flat from 64 to
256 px -- but scales with facet size, 3.9e-2 at 192 facets to 1.5e-2 at 2,352:
the hemicube samples facet `i` at its centre, and a point-to-area view factor
does not obey reciprocity with an area-to-area one. It is a distribution error
across columns; row sums stay exact to 1e-5.

## 3. What heating is worth

One Didymos rotation to the study epoch, against no heating, rebuild cadence 5:

| | mean | p50 | p90 | p99 | max | facets >1 K |
|---|---|---|---|---|---|---|
| Didymos | +0.07 K | +0.02 | +0.16 | +0.72 | +2.05 | 54 / 10,000 |
| **Dimorphos** | **+2.92 K** | +2.15 | +7.33 | +11.74 | **+30.18** | **6,578 / 10,000** |

### Decomposed

Temperature contribution, K at the study epoch (isolated runs; the four
superpose to within 0.1 %):

| term | Didymos mean | Didymos max | Dimorphos mean | Dimorphos max |
|---|---|---|---|---|
| self re-emission | +0.047 | +1.778 | **+1.572** | **+27.733** |
| self reflection | +0.004 | +0.217 | +0.117 | +2.780 |
| mutual re-emission | +0.018 | +0.095 | **+1.195** | **+10.026** |
| mutual reflection | +0.000 | +0.049 | +0.143 | +1.261 |

**Thermal re-emission is ~90 % of it**; reflected sunlight is minor, as it must
be at albedo ~0.1. The two terms have different shapes: self re-emission has a
long tail (p50 +0.76, max +27.7) concentrated in concavities, while mutual
re-emission is bimodal (p50 +0.007, p90 +4.5, capped near +10) -- half the body
never sees the companion at all.

### Mutual heating is a night-side effect

On Dimorphos, mutual re-emission delivers **+2.34 K to the coldest quartile and
-0.001 K to the warmest**. Tidal locking points the Didymos-facing hemisphere
away from the sun at conjunction, so the term lands almost entirely where the
surface is cold -- and `dT = dF / (4 eps sigma T^3)` makes a given flux worth
four times as much at 200 K as at 320 K. This matters for thermal-inertia
retrieval, which is most sensitive on the night side.

### Why Didymos barely cares

Its entire mutual contribution is **+0.099 K on the worst facet of 10,000**.
The asymmetry is a size effect and nothing subtler: the view factor to a
companion goes as `(R/d)^2`, and Dimorphos's radius is ~0.085 km against
Didymos's ~0.39 km at the same 1.15 km separation -- about 20x in solid angle.
Dimorphos sees a big primary; Didymos sees a small secondary.

**Conclusion for production: Didymos needs self only. Dimorphos needs both.**
`examples/hera_didymos/heating_preflight.py` reaches that verdict in about a
minute at any epoch, for any pair, by converting each flux term through the
linearised `dT`. It reads roughly 2x high, since it ignores conduction into the
column, which is the safe direction for a screening test.

## 4. Cost, and the rebuild cadence

Self view factors are fixed in the body frame -- measured, they do not move at
all. The mutual ones must be rebuilt, and the reason inverts the obvious
argument: Dimorphos is tidally locked, so Didymos holds still in its sky and
the **solid angle barely changes**; but Didymos rotates underneath in 2.26 h,
so which of its facets fill that angle, day side or night side, turns over
completely. The row sum is nearly blind to this -- it moves 4.6 % between
rebuilds while the temperature behind it swings hundreds of kelvin.

**The cadence is set in geometry, not in steps.** A step count is meaningless
here: `dt` comes from the stiffest grid in the system, so the same number of
steps samples the geometry differently on a different mesh, body or depth grid.
What the rebuild chases is the scene turning over, so `VF_EVERY_DEG` is
degrees of the **fastest rotation in the system** -- for a tidally locked pair
that is the primary's spin, and taking the minimum spin period over the loaded
bodies gets it right without special-casing, since a locked secondary spins at
its orbital period.

At `dt = 55.94 s` against Didymos's 2.26 h spin, one step is **2.475 deg**.
Measured over one rotation against a rebuild every 4.95 deg, on a +2.92 K
effect:

| degrees | % of spin | steps here | ms/step | Dimorphos error: mean / p99 / max |
|---|---|---|---|---|
| 4.95 | 1.38 % | 2 | 2925 | reference |
| **12.4** | **3.44 %** | **5** | **1199** | **+0.014 / 0.136 / 0.420** <- default |
| 24.8 | 6.88 % | 10 | 601 | +0.058 / 0.519 / 6.623 |
| 61.9 | 17.2 % | 25 | 250 | +0.113 / 1.290 / 1.406 |
| 360 | 100 % | once | 56 | -0.631 / 4.731 / 5.231 |

**Every ~12 degrees of primary rotation**, or about 3.4 % of a spin, is 0.5 %
of the effect and costs about 26 min for the full 1,309-step segment. An earlier version of this note claimed 25 was converged;
that rested on comparing it against cadence 10, which is not itself converged,
and was wrong.

The 6.6 K maximum at cadence 10 is not a convergence failure but the shadow
quantisation described in section 5.

Throughput at 128 px faces: **0.20 ms per facet**, against 18.7 ms for a
readback-per-face prototype. 2 s for a 10,000-facet matrix, 20 s at 100k,
10 min at 3.1M -- from impossible to a coffee break.

## 5. Four bugs, and one that was not

**The hemicube far plane was sized from the requesting body.** `far =
mesh.bounds.radius() * 4` is 2.60 km for Didymos and reaches its companion, but
0.545 km for Dimorphos, whose primary sits at 1.15 km -- entirely outside.
What survived was a sliver, non-zero and so not obviously clipping. The mutual
term read 0.017 against a true 0.115 at the real separation and was **exactly
zero past 1.5 km**. Fitted to `scene_bounds()` now. This was the second time a
frustum was sized from one body's extent in a binary (`b0a0ff4` was the shadow
pass). Aggregated reciprocity would have caught it, and now exists.

**An occluded window stopped the simulation dead.** `get_surface_texture`
returns `Occluded` when the window is covered, and the frame handler took that
as a reason to `return` -- before `before_render`, `after_render` and
`simulation.update()`. Not throttling: no steps, no iterations, wall time still
accruing. A cadence sweep sat at 0 % progress for 18 minutes. Everything the
simulation needs is offscreen, so `render` now takes an `Option<SurfaceTexture>`
and a frame without one is a complete frame minus the blit and the present.
**Old results are unaffected** -- the skipped frames did no work -- but any
timing taken while focus was lost is a lower bound, not a measurement.

**The hemicube reallocated its scratch buffers every call.** 20 MB each at
batch 256 over a 20,000-facet index space, eight calls per rebuild. A sweep
halted at exactly 38 rebuilds, twice, at 46 % CPU with the main thread idle.
Kept on the struct now; cadence 5 costs 175 s where cadence 10 cost 601 s
before.

**`os._exit` was discarding buffered stdout**, so a piped run lost everything
`save()` printed. Latent in the example before this work.

**Not a bug: an apparent monotonicity violation.** One Dimorphos facet read
5.35 K *below* the unheated run, which cannot happen when the added flux is
non-negative. Tracing it, the heating fluxes agreed between runs to 0.5 % and
the whole difference was `facet_shadow` returning a lit fraction of 0.750
against 0.500 **for identical geometry**. `SAMPLES_PER_FACET` is 4, so the lit
fraction is quantised to `{0, 0.25, 0.5, 0.75, 1}` and one sample flipping at a
knife-edge terminator facet is worth 139 W/m2. This is a pre-existing bound on
per-facet precision near the terminator, about +/-6.6 K on this mesh, and has
nothing to do with heating. A facet-level monotonicity check must tolerate one
flipped sample.

## 6. The shape models

**Correction to a claim made on 31 August.** `Mesh::inward_facing_facets`
reported 22 reversed facets on the decimated 10k Dimorphos mesh, and that was
taken at face value past the warning in its own docstring. Tested by flipping
all 22 and re-measuring with the hemicube:

| | self VF before | after flipping |
|---|---|---|
| the 22 flagged | mean 0.271, max 0.996 | **mean 1.000, max 1.000** |

Flipping sent every one of them to a self view factor of exactly 1 -- looking
straight into the body -- and their solar incidence from +0.58 to -0.58, with
the number facing the sun falling from 20 to 2. **Twenty-one of the 22 were
correct to begin with**: real concavities, exactly the overhang case the
heuristic said it would misjudge. Facet 243 is not a winding error either;
flipping moved it 0.9957 -> 1.0000, worse both ways.

So the claim that 22 facets sat at night temperature in every Dimorphos run
was wrong. **One did.**

**The hemicube is the detector.** Self view factor > 0.5 is ground truth and
assumes nothing about shape; genuine concavities top out at 0.351.
`heating.pathological_facets` does this. The heuristic over-reports ~20x and
also misses cases (3 real on the 100k, of which it flags 2).

### It is a decimation artefact

| mesh | flagged |
|---|---|
| Didymos 10k / 100k / full 3.1M | 0 / 0 / 0 |
| Dimorphos 10k | 22 (1 real) |
| Dimorphos 100k | 21 (3 real) |
| **Dimorphos full 3.1M** | **0** |

The source models are clean. MeshLab's `preservenormal` defaults to False and
its tooltip reads "try to avoid face flipping effects", so the defaults the
existing meshes were cut with permit exactly this. Re-cutting from the 3.1M
original, with hemicube self VF as ground truth and surface area as fidelity:

| recipe | max self VF | facets > 0.5 | area error |
|---|---|---|---|
| as shipped | 0.9957 | 1 | +0.02 % |
| + planarquadric | 0.5096 | 2 | +2.07 % |
| **+ preservenormal, preservetopology, qualitythr 0.6** | **0.3189** | **0** | **+0.06 %** |

Applied to the 100k as well: max self view factor **0.369, zero facets above
0.5**, against 3 pathological in the mesh it replaces.

**Done on 1 September**: all four decimated models (both bodies, 10k and 100k)
were re-cut with the recipe and put in place under their original filenames.
The previous files are kept beside them as `*.meshlab_default_backup.obj`, so
reverting is a rename. Note that **Didymos gained nothing** -- its existing
models already flagged zero and matched the 3.1M area to 0.003 % -- so
replacing them costs a phase-1 re-run for no benefit, and reverting those two
is reasonable.

`examples/mesh/decimate.py` carries the recipe. `planarquadric` also removes
the flips but costs 2 % of the surface area, so the recipe reaches it with a
tighter quality threshold instead.

**Replacing a shape model invalidates the phase-1 spin-up**, whose saved state
is an array indexed by facet. Re-decimating to the same target leaves the facet
*count* identical and every position different, so the count check that was
there could not catch it and the run would have proceeded against the wrong
geometry. `tpm_phase2.py` now fingerprints the facet positions and refuses to
start against a mismatched restart state.

**So the current spin-up states are stale**, the meshes having been replaced.
Phase 1 must be re-run for both bodies before the next production segment.

## 7. What is still to do

- ~~Synodic-phase parametrisation~~ **built, measured, and turned off.** It
  does not work here, and the reason is worth keeping.

  A table of 30 mutual view-factor sets over one synodic period replaces every
  rebuild in a run of any length -- 522 hemicube passes become 60 for a
  1,309-step segment. The indexing is right: the entry chosen matches the
  run's geometry to 5.7 deg in direction and 7.2 deg in orientation, exactly
  as designed.

  But it costs Dimorphos **0.66 K in the mean, 5.7 K at p99 and 19 K at
  worst**, against a heating effect of 2.92 K -- a 23 % error on the quantity
  being computed. Doubling to 60 phases changes nothing (0.688 K, max 18.1 K),
  so the floor is not table density. The pair does not recur synodically as
  cleanly as the geometry suggests; the ~5.7 deg residual measured below is
  a real wobble, and no sampling removes it.

  **The reasoning that led here was wrong in an instructive way.** 5.7 deg
  looked acceptable because the rebuild *cadence* tolerates 12 deg. But a
  cadence error is staleness that is zero at every rebuild and averages out,
  while this is a **persistent offset that never returns to zero**. Two
  tolerances measured in degrees are not the same tolerance.

  `heating.SynodicTable` is kept and `VF_TABLE` defaults off. The assumption
  it rests on -- spin axis along the orbit normal, circular orbit, rigid
  locking -- would hold for a pair without Dimorphos's post-DART libration.

- **Superseded: the parametrisation as originally imagined.** The
  remaining real optimisation, and the part of the original plan's item 4 that
  survives. Caching the self block separately buys nothing, since one hemicube
  pass produces both blocks and the mutual one must be rebuilt regardless.

  **The right variable is not orbital phase.** An earlier version of this note
  said a run longer than one orbit could index a table by orbital phase; that
  is wrong. Didymos turns 5.0299 times per orbit -- not an integer -- so the
  pair never repeats on the orbit alone. What repeats is
  `psi = orbital phase - spin phase`, whose period is the **synodic period,
  2.821 h**, four times shorter than the orbit. If the spin axis is aligned
  with the orbit normal and the orbit circular, the relative geometry depends
  on `psi` alone.

  Measured against the kernels: the separation is **constant** at 1.1510 km, so
  the orbit is circular, and the configuration recurs at multiples of the
  synodic period to **0.5-3.5 deg in direction and 0.7-5.7 deg in orientation**.
  Refitting the period barely moves it (-0.6 s) and the residual oscillates
  rather than converging, so it is a real wobble -- Dimorphos's post-DART
  libration is the obvious candidate -- not a period error.

  **That residual is inside the tolerance already accepted**: the rebuild
  cadence is 12 deg. So a table of ~30 entries over one synodic period, about
  162 s to build, would replace every rebuild in a run of any length. At
  cadence 5 the rebuilds are most of the cost of a segment, so this takes a
  1,309-step run from ~26 min to about 4. It needs an end-to-end temperature
  check against direct rebuilds before adoption, not just the geometric
  argument above.
- **Re-run the GIS3D product** with heating on -- Dimorphos self + mutual,
  Didymos self only. See `2026-08-31_gis3d_tiri_product/`.
- ~~Multiple scattering~~ **done**. `heating.absorbed` takes `bounces`,
  running a Neumann series on the body's square self block -- light bounces
  inside a concavity, and the cross-body term is both weaker and not square.
  Verified against the closed form for a two-facet cavity.

  Measured over one rotation against 12 bounces, a single bounce errs by
  **0.013 K mean and 0.37 K at worst on Dimorphos**, 0.0001 / 0.006 K on
  Didymos; five bounces are converged to four decimals. The earlier estimate
  that it might be worth ~30 % in a deep concavity was a bound on the *flux*
  at one facet, and that facet's absolute heating turns out to be modest.

  It costs almost nothing -- 179 s against 175 s for a whole segment, since
  the rebuilds dominate and a bounce is one sparse matvec -- so the default is
  now 5 rather than leaving a known approximation in for no saving.
- **Re-run the phase-1 spin-up** for both bodies. The meshes were replaced on
  1 September, so the saved states no longer match; `tpm_phase2.py` will now
  refuse to start rather than run against the wrong geometry.
- **The 2.0 km sweep anomaly**: at that separation the swept mutual
  distribution drops ~4x below its neighbours at 1.5 and 2.5 km while the
  maximum stays on the `(R/d)^2` curve. Outside this system's orbital range
  (1.10-1.28 km), so it affects nothing here, and unexplained.
- **Cadence 1 hangs** on the final rebuild. Cadence 2 and up complete; not
  chased, since 5 is the working point.

---

# Detail

What follows is the original chronological write-up, as it was made. It
contains the derivations and the intermediate reasoning; where it disagrees
with the sections above, the sections above are current -- in particular the
reversed-facet count and the rebuild cadence.

# 9. Revisiting view factors

§7.2, §7.3 and §8.3 all reduce to view-factor bookkeeping, so the current
implementation is on the critical path three times over. It has problems worth
fixing before building on it.

## 9.1 What is there now

`mesh.rs::view_factor_facets` computes the differential form

```
VF = cos(theta_a) cos(theta_b) / (pi d^2)
```

per unit area, guarded by two tests: both facets must face each other
(`theta < 90 deg`), and

```rust
if distance_a2b < face_b.area.sqrt() {
    return 0.0;
}
```

## 9.2 Three problems

**(a) The proximity guard returns zero where the view factor is largest.**
The differential form assumes `d >> facet size`, and diverges as `d -> 0`, so
a guard is needed. But returning **0** is the worst available answer. Adjacent
facets have centroid separation of order `sqrt(area)` — exactly the threshold —
so the guard fires on precisely the neighbours that dominate self-heating
inside a concave region. Applied to §7.3 or §8.3 as written, most of the
self-heating would silently vanish, and the model would run and produce
plausible-looking, systematically cold results. The existing `TODO:
subdivide the facet and recompute` is the right fix; until then the failure is
biased, not merely approximate.

**(b) No occlusion test.** The function is pure geometry: it never asks whether
the two facets can actually see each other. The angle test catches facets that
face away, but not an intervening ridge or crater rim between two facets that
do face each other — which is the defining situation for concave terrain, and
therefore for every case where self-heating matters. Radiative exchange
between two facets separated by solid rock is currently counted in full.

**(c) `acos` then `cos`.** `angle_between` computes an angle whose cosine is
then taken. For unit vectors that round trip is exactly `dot(u, v)`. It costs
two transcendentals per pair and loses precision at small angles, over an
O(N^2) loop.

## 9.3 The GPU route: hemicube

(b) is the expensive problem — an exact visibility test between facet pairs is
`O(N^2)` ray casts, which is the same wall we hit with per-facet shadowing
before the shadow map replaced it (`2026-08-26_facet_shadow_query/`). The
resolution is the same, and it is the classic radiosity technique:

Place a **hemicube** at facet *i*, render the scene into it with each facet
writing its own **index** rather than a colour, and read the buffer back. Then

- every facet visible from *i* appears, and the depth test has already
  resolved occlusion exactly — no separate visibility pass;
- the number of pixels a facet covers, weighted by the per-pixel delta form
  factor (a fixed function of hemicube geometry), *is* its view factor;
- one render pass yields the entire row `VF[i, :]`, so the whole matrix costs
  `O(N)` passes rather than `O(N^2)` pair tests;
- the near-field problem (a) dissolves: a close facet simply covers many
  pixels, with no singular `1/d^2` and no threshold to choose.

kalast already has every piece: render-to-texture, a depth pass, per-facet
attributes, and GPU readback with the blocking/async trade already measured.
The facet-index render is the same shape as the facet-shadow compute pass,
and the **shadow proxy** finding applies directly — the occluder written into
the hemicube can be a decimated mesh, which was measured to shift results by
9 pixels in 1,040,400 (`2026-08-26_shadow_mesh_comparison/`).

## 9.3b Steps 1 and 2 done: the CPU reference

`view_factor_scalar_cos` takes cosines directly, and `view_factor_facets` now
computes them as dot products. The old path called `angle_between` and then
`cos` on the result -- two transcendentals to recover a number already in
hand, and `acos` has unbounded derivative at 1, so the round trip lost the
most precision exactly where facets face each other squarely and the view
factor is largest.

`view_factor_triangles(tri_a, tri_b, ratio, max_level)` is the near-field fix.
It returns the dimensionless `F(A->B)` by evaluating the double area integral
on a uniform subdivision, refining a pair while its separation is below
`ratio` times its own size. Refinement is keyed on the **larger** of the two
areas: a small facet beside a large one still needs the large one split, and
the original guard tested only `area_b`.

### Validated against closed forms

`examples/analytical/view_factors.py`, the same discipline section 3 applied
to conduction. Unit squares, each as two triangles, so the area weighting the
mesh code will use is exercised too.

**Parallel coaxial unit squares** at several separations:

| separation | exact | numerical | error |
|---|---|---|---|
| 2.00 | 0.06859 | 0.06910 | 0.75 % |
| 1.00 | 0.19982 | 0.20108 | 0.63 % |
| 0.50 | 0.41525 | 0.41609 | 0.20 % |
| 0.25 | 0.63204 | 0.63259 | 0.09 % |
| 0.10 | 0.82699 | 0.83130 | 0.52 % |

**Perpendicular unit squares sharing an edge**, the hard case -- the shared
edge puts sub-pairs at arbitrarily small separation, which is precisely what
the point-to-point form cannot represent. Exact `F = 0.20004`:

| ratio | max_level | numerical | error |
|---|---|---|---|
| 0 (none) | – | 0.30339 | **+51.7 %** |
| 2 | 2 | 0.23083 | 15.4 % |
| 4 | 3 | 0.21479 | 7.4 % |
| 6 | 4 | 0.20749 | 3.7 % |
| 8 | 5 | 0.20385 | 1.9 % |
| 10 | 6 | 0.20202 | 1.0 % |

Convergence is first order -- halving the error costs a level -- because of
the edge singularity. `ratio = 6, max_level = 4` gives 3.7 % for a bounded
cost and is the default.

### What the old guard was doing, measured

Reproducing `distance < sqrt(area) -> 0` on that same perpendicular pair
gives `F = 0.12434` against an exact 0.20004: **−37.8 %**. So section 9.2(a)
was right that it deletes rather than approximates, and the size of the
deletion is now a number. The unsubdivided form errs the other way, +51.7 %,
so the guard was not correcting a known bias -- it replaced one large error
with a different large error of opposite sign.

### Reciprocity

`A_a F(a->b) == A_b F(b->a)` is an identity, so any deviation is
implementation error rather than discretisation. Measured mismatch: exactly 0
for the parallel pair, 7e-8 for the perpendicular one. Cheap, and it is the
check that will guard the hemicube.

### The cost, which is why the hemicube exists

A subdivided pair costs 5.93 us through the Python binding. For the full
matrix:

| facets | full N^2 matrix | dense storage |
|---|---|---|
| 10,000 | 0.2 h | 400 MB |
| 100,000 | 16.5 h | 40 GB |
| 3,100,000 | 15,828 h (1.8 yr) | 38 TB |

10,000 facets is tolerable as a one-off, and since the self-view-factor
matrix is fixed in the body frame it need only be computed once per shape
model -- see 9.4 step 4. But it is a reference, not a method: it has no
occlusion test, and storage alone rules out the full-resolution meshes. Both
problems are what the hemicube answers.

## 9.3c Mutual view factors: what actually has to be recomputed

Self view factors are fixed in a body's own frame, so they are computed once
per shape model. Mutual view factors depend on the relative pose of the two
bodies, which changes continuously -- so the natural assumption is that they
must be rebuilt every timestep, for both directions. Neither part is true.

### One matrix serves both directions

Reciprocity, `A_i F_ij = A_j F_ji`, is an identity, not an approximation. So
the Dimorphos-to-Didymos matrix is the area-scaled transpose of the
Didymos-to-Dimorphos one: compute one pose, get both directions. Validated to
7e-8 in `examples/analytical/view_factors.py`.

### The two directions are wildly unequal in importance

The view factor from a flat facet to a sphere of radius `R` at distance `d`,
facing it squarely, is exactly `(R/d)^2`. With a 1151 m separation:

| direction | max view factor |
|---|---|
| Didymos facet -> Dimorphos | `(85/1151)^2` = **0.55 %** |
| Dimorphos facet -> Didymos | `(390/1151)^2` = **11.5 %** |

A factor of 21, because the primary is 4.6x the secondary's radius and the
separation is only 3x its own. What that does to a facet, re-equilibrating
`T -> (T^4 + F T_other^4)^(1/4)`:

| facet | F | effect |
|---|---|---|
| Didymos, warm (300 K) | 0.0055 | **+0.75 K** |
| Didymos, night (120 K) | 0.0055 | +10.4 K |
| Dimorphos, warm (300 K) | 0.1148 | +13.3 K |
| Dimorphos, night (120 K) | 0.1148 | **+84.3 K** |

### The geometry each body sees is completely different

Sampling the direction to the companion in each body's own fixed frame over
one orbit:

| seen from | companion wanders | sub-companion point |
|---|---|---|
| Didymos | **178.2 deg** | lon sweeps the full -177 to +180 |
| Dimorphos | **0.0 deg** | lat 0.0, lon 0.0, fixed |

Dimorphos is tidally locked, so **Didymos never moves in its sky**. The
sub-Didymos point is stationary to the precision of the kernels.

### What follows for the implementation

- **Self VF, both bodies**: fixed in the body frame. Computed once per shape
  model, reused across every timestep. Unchanged by any of this.
- **Dimorphos <- Didymos** (the term that matters): the *direction and
  distance* to Didymos are permanently fixed in Dimorphos's frame. The only
  thing that varies is which of Didymos's facets are where, i.e. Didymos's
  spin phase -- one angle, period 2.26 h. Tabulate on it once and reuse
  forever. The solid angle Didymos subtends from each Dimorphos facet never
  changes at all.
- **Didymos <- Dimorphos** (the small term): the geometry does sweep the full
  360 deg, with a synodic period of 2.82 h. But it is a sub-Kelvin effect on
  the day side, so a coarse tabulation is ample.

So nothing needs recomputing per timestep. The parameterisation is by
*measured phase angle*, not elapsed time: indexing on time inherits the error
in the nominal spin and orbit constants, which drifts the pose by 3-6 deg
within one synodic period.

A caveat on how clean that parameterisation is. Binning 2,160 epochs over
three days into 72 five-degree phase bins, the worst spread *within* a bin is
72 m in position (6.25 % of the separation) and 6.5 deg in orientation -- so
the pose is not quite a single-valued function of one angle. The residue is
the sub-Dimorphos latitude wandering +/-2.8 deg, because the mutual orbit is
not exactly in Didymos's equatorial plane. For the 0.55 % direction that is
irrelevant. For the 11.5 % direction it does not arise, since that geometry
is fixed outright.

### This is a first-order gap in the delivered Dimorphos temperatures

Worth stating plainly, because it affects the GIS3D product. Dimorphos is
tidally locked with an 11.5 % view factor to a primary whose day side runs
near 340 K, and its *sub-Didymos hemisphere sees that permanently*. Direct
insolation alone -- which is what both the spin-up and phase 2 use -- omits a
term that could reach tens of kelvin on that hemisphere and up to ~84 K on
its coldest facets.

That does not affect Didymos, whose corresponding term is sub-Kelvin on the
day side, so the primary in the delivered FITS is unaffected. But the
Dimorphos pixels should be read as a lower bound on its night-side
temperatures until mutual heating is in. The FITS headers already carry
`HEATING = 'none'`; this quantifies what that costs and where.

## 9.3d The hemicube works, and is both faster and more accurate

`examples/analytical/hemicube.py`, step 3 of the plan. A hemicube is placed at
a facet, the scene is rendered into its five faces with each facet writing its
own index, and each pixel contributes its delta form factor to whichever facet
it shows. Occlusion is resolved by the depth test, so it comes free rather
than as a separate O(N^2) visibility pass.

### Closure first

Before any geometry, the weights must sum to 1 over the hemicube -- if they
do not, nothing downstream means anything:

| face resolution | sum of delta form factors | error |
|---|---|---|
| 32 | 1.000530 | 5.3e-4 |
| 64 | 1.000132 | 1.3e-4 |
| 128 | 1.000033 | 3.3e-5 |
| 256 | 1.000008 | 8.3e-6 |

Second order in resolution, as the midpoint rule implies.

### Against the closed form

The perpendicular unit squares sharing an edge -- the configuration where the
point-to-point form is worst -- with the emitter subdivided into 128 sample
points and area-averaged:

| method | F | error |
|---|---|---|
| exact | 0.20004 | – |
| **hemicube, 128 px faces** | **0.19990** | **0.07 %** |
| CPU subdivided, ratio 6 level 4 | 0.20749 | 3.7 % |
| point-to-point, unsubdivided | 0.30339 | 51.7 % |
| the old `sqrt(area)` guard | 0.12434 | −37.8 % |

**Fifty times more accurate than the CPU reference it was built to
reproduce**, which is the happy case for a validation: the thing being tested
beat its own reference, and the closed form is what says so.

### And faster

18.7 ms per hemicube in this prototype -- five renders and five blocking
readbacks over PCIe, the slowest possible arrangement. Even so:

| method | 10,000-facet matrix | occlusion | error |
|---|---|---|---|
| CPU subdivided pairs | 12 min | **none** | 3.7 % |
| hemicube prototype | **3.1 min** | exact | 0.07 % |

The prototype is dominated by readback latency: it copies a 128x128 integer
buffer to the CPU and does the weighting in numpy, five times per facet.
Accumulating on the GPU in a compute pass and reading back once per facet --
or once per batch -- is the obvious next step, and the facet-shadow work
already measured what that saves.

### Why this also settles the near-field problem

There is no `1/d^2` to diverge and no threshold to choose. A facet that is
very close simply covers many pixels of the hemicube, and its view factor is
the sum of their weights, bounded by construction at 1. The subdivision
machinery in `view_factor_triangles` exists to give a CPU reference and to
handle cases with no renderer available; the hemicube does not need it.

## 9.3e The hemicube on the GPU: 0.20 ms per facet

`src/app/hemicube.rs`, `shaders/hemicube.wgsl` and
`shaders/hemicube_accumulate.wgsl`, exposed as
`sim.request_hemicube(body, facets, resolution, batch)` /
`sim.hemicube()`.

The Python prototype read each face back and weighted it in numpy: five
blocking PCIe round trips per facet, 18.7 ms. Here the atlas never leaves the
device. Five faces render into one integer atlas within a single render pass
(five viewports, so it is cleared once), a compute pass scatters the delta
form factors into a per-facet accumulator, and only the finished rows come
back, once per batch.

### Fixed point, because WGSL has no atomic float add

The accumulation is a scatter -- many pixels land on the same facet -- so it
needs an atomic. Weights are summed as `u32` at a scale of `2^30`, which is
safe by construction rather than by luck: the delta form factors over a whole
hemicube sum to exactly 1, so no facet's total can exceed `2^30` and overflow
is impossible. At the other end the smallest weight on a 128 px face is
~8.7e-6, still ~9,300 in fixed point.

### Validated on a closed box

The strongest available test, because the answer is forced rather than
tabulated: **inside a sealed cavity every direction hits geometry, so every
row sum must be exactly 1.** Any shortfall is leakage -- depth clipping, a bad
near plane, missing weight.

| quantity | measured | required |
|---|---|---|
| row sums | 1.00001 (all 12 facets) | 1 |
| self term `VF[i,i]` | 0.000000 | 0 |
| reciprocity `\|A_i F_ij - A_j F_ji\|` | 2.5e-2 | 0 |

Closure to 1e-5 and no self-view. The reciprocity residue is not an error in
the pass: the hemicube evaluates the view factor from a *point*, the facet
centroid, and point-to-area factors do not satisfy finite-area reciprocity
exactly. It is the same single-sample discretisation that gives 2.83% on the
perpendicular squares with one hemicube per triangle against 0.07% with 128
sample points. Area-averaging over sub-samples is the knob.

### A false alarm worth recording

The first closed-box run returned row sums of exactly 0, which looked like a
serious leak. It was the **test geometry**: the box was written with inverted
winding, so every facet normal pointed outward and each hemicube correctly
looked away into empty space. Confirmed by driving the already-validated
`facet_id` pass with the same camera and finding zero lit pixels, and by
printing facet 0's normal as `(0, 0, -1)` when the floor of a box should face
`+z`.

Two things worth keeping from it. A test whose expected answer is a hard
constant is worth more than one with a tabulated reference, because "0 where 1
is required" is unambiguous. And when a new pass returns nothing, checking the
*input geometry* before the pass costs a minute -- the normals were printable
all along.

### Throughput

At 128 px faces on the 10,000-facet Didymos mesh:

| hemicubes | wall | per hemicube |
|---|---|---|
| 64 | 0.05 s | 0.74 ms |
| 256 | 0.05 s | 0.21 ms |
| 1,024 | 0.21 s | 0.20 ms |

**0.20 ms per facet**, against 18.7 ms for the readback-per-face prototype --
a 94x saving, all of it latency that never needed to be paid. Extrapolated:

| facets | full self-VF matrix | previously (CPU pairs) |
|---|---|---|
| 10,000 | **2 s** | 12 min |
| 100,000 | **20 s** | 16.5 h |
| 3,100,000 | **10 min** | 1.8 yr |

The full-resolution shape models move from impossible to a coffee break. And
since self view factors are fixed in the body frame, that is once per shape
model, not once per run.

### Didymos really does self-view very little

On the 10k mesh the row sums come out at mean 2e-4, max 1.1e-2 -- a facet
sees at most 1% of its hemisphere filled by the rest of the body. That looked
wrong until the closed box confirmed the machinery: it is simply what a
near-convex body gives, since a strictly convex surface has *zero*
self-view-factor everywhere. Self-heating on Didymos is a small term, and the
places it is not small are the concavities, which is where the row sums are.

Worth separating from an earlier result that sounds related but is not:
self-*shadowing* affected 4,105 facets and cost up to 12.4 K (section 7.7).
Blocking sunlight needs only a grazing sun angle over a gentle slope;
self-*viewing* needs genuine concavity. They are different geometric
questions and there is no contradiction between a large one and a small
other.

## 9.3f Mutual view factors, and a shape-model defect they exposed

The hemicube now renders **every loaded body** into one shared facet index
space, exactly as the facet-id pass does, with a per-body offset supplied as
a dynamic-offset uniform. One row therefore carries the self view factors and
the mutual ones together, and splitting them is a slice --
`sim.hemicube()` returns the offsets to do it with.

Occlusion is shared too, which is the point: the depth test resolves the
*other* body as readily as the body's own terrain, so a mutual eclipse blocks
mutual heating without any extra machinery.

Re-validated through the multi-body path on the closed box: row sums
1.00001, self term 0. Unchanged, as it should be with one body loaded.

### At the study epoch

| | hemicubes | self VF row sum | mutual VF row sum | facets seeing the companion |
|---|---|---|---|---|
| Didymos | 10,000 in 2.8 s | mean 0.0013, max 0.043 | mean 0.0013, max 0.0097 | 4,039 |
| Dimorphos | 10,000 in 2.7 s | mean 0.034, max 0.996 | mean 0.0134, max 0.116 | 5,268 |

The Dimorphos mutual row is **corrected from an earlier version of this table**
that read mean 0.0029, max 0.064 over 2,108 facets. Those were a clipped
remnant -- see "the far plane was sized from the wrong body" below.

The Didymos mutual maximum, 0.0097, exceeds the `(R/d)^2 = 0.0055` bound
quoted in 9.3c. That bound is not violated: it uses the centre-to-centre
separation, and a Didymos facet on the near side sits ~390 m closer, giving
`(85/761)^2 = 0.0125`. The measured value is below *that*, as it must be.
Worth noting because the discrepancy looks alarming until the bound is
restated for the right distance.

### The far plane was sized from the wrong body

The first multi-body numbers had Dimorphos seeing Didymos at mean 0.0029, max
0.064. They should have been challenged on sight: Dimorphos is a 180 m body
1.15 km from a 780 m one, so a facet on the near side has a good fraction of
its sky filled by the primary. `(R/d)^2 = 0.115` is the number to expect, and
0.064 is half of it.

The cause was in the hemicube frustum. `window.rs` sized it from the
*requesting body's* model-space bounds:

```rust
let radius = mesh.bounds.radius();   // this body, not the scene
let far = radius * 4.0;
```

For Didymos that is 2.60 km and reaches Dimorphos comfortably. For Dimorphos
it is **0.545 km**, and Didymos sits at 1.15 km -- entirely beyond it. What
survived was the thin sliver of the primary that happened to fall inside,
which is why the number was not zero and so did not look like clipping.

Measured by sweeping the separation and comparing against `(R/d)^2`, holding
everything else fixed:

| separation | mutual VF max, before | after | `(R/d)^2` |
|---|---|---|---|
| 0.9 km | 0.198 | 0.204 | 0.188 |
| 1.0 km | 0.134 | 0.159 | 0.152 |
| **1.151 km (actual)** | **0.017** | **0.115** | **0.115** |
| 1.5 km | 0.000 | 0.060 | 0.068 |
| 3.0 km | 0.000 | 0.014 | 0.017 |

Before the fix the term collapses as the companion crosses the far plane and
is **exactly zero past 1.5 km**. After it, the falloff follows `(R/d)^2` and
the implied effective radius stays at 0.384-0.392 km across the whole orbital
range 1.10-1.28 km, against Didymos's 0.39 km. At the study epoch the
Dimorphos mutual mean rises 4.6x and the facets registering the primary at all
go from 2,108 to 5,268.

`far` is now fitted to `Simulation::scene_bounds()` -- the same scene fit the
shadow pass was given in `b0a0ff4`, and for the same reason: **in a binary,
one body's extent says nothing about where the other one is.** That is twice
this exact assumption has been wrong in this codebase.

Didymos's own numbers are unchanged, since its far plane already reached. An
asymmetry between the two directions was the visible symptom, and reciprocity
would have caught it: `A_i VF_ij = A_j VF_ji` fails badly when one direction
is clipped and the other is not. It is cheap and it is still not asserted
anywhere.

One loose end: at a separation of 2.0 km the swept distribution drops ~4x
below its neighbours at 1.5 and 2.5 km, while the maximum stays on the
`(R/d)^2` curve. Every other distance has mean, median and max within a few
percent of each other. It is outside this system's orbital range so it does
not affect anything here, but it is unexplained.

### A defective shape model, found by an impossible number

Dimorphos's self view factor peaked at **0.996** -- a facet seeing 99.6 % of
its own hemisphere filled by its own body. Deep concavities exist, but not
that deep on a decimated ellipsoid.

The cause is in the mesh: **22 of Dimorphos's 10,000 facets have inward-
pointing normals**, worst dot with the outward radial direction −0.958, i.e.
aimed almost straight into the body. A hemicube on such a facet looks inward
and sees the interior. Correlating directly:

| | count | mean self-VF | max |
|---|---|---|---|
| inward-facing normals | 22 | 0.271 | 0.996 |
| outward | 9,978 | 0.034 | 0.351 |

The single facet above 0.5 is one of the 22. Excluding them, the maximum is
0.351 -- a real concavity. Didymos's mesh has none.

**This is quietly destructive beyond view factors.** The thermophysical model
clamps `cos(incidence)` at zero, so a reversed facet never receives sunlight
at all and sits at night temperature permanently. It has been doing so in
every Dimorphos run so far, including the delivered FITS -- 22 facets,
0.114 % of the surface area, so the effect on any integrated quantity is
negligible, but the individual pixels are wrong.

`Mesh.inward_facing_facets()` now reports them, in Rust so it is usable on
the 3.1M meshes. It compares each normal against the outward radial
direction from the mesh centroid, which assumes a roughly star-shaped body --
true of small asteroids, and it will misjudge a deep overhang, so it is a
list to inspect rather than a verdict.

The general lesson is the one from the closed box again: **a number that
cannot physically occur is the most informative test available.** Nothing
about 0.996 required a reference value to interpret.

### Correction: it is one facet, not 22, and flipping does not fix it

The paragraphs above are wrong about the count, and the error was the
heuristic being trusted past the warning attached to it. Tested directly by
flipping the winding of all 22 and re-measuring with the hemicube:

| | self VF before | after flipping |
|---|---|---|
| the 22 flagged | mean 0.271, max 0.996 | **mean 1.000, max 1.000** |
| every other facet | mean 0.034, max 0.351 | unchanged |

Flipping sent all 22 to a self view factor of **exactly 1** -- looking
straight into the body -- and their solar incidence from +0.58 to -0.58, with
the number facing the sun dropping from 20 to 2. So 21 of the 22 were correct
to begin with: real concavities, which is precisely the "deeply overhanging
facet" case the detector's own docstring said it would misjudge.

The remaining one, facet 243, is not a winding error either. Flipping it moved
its self view factor from 0.9957 to 1.0000 -- worse in both orientations. It
is a normal-sized facet sitting at 110 % of the median surface radius, so not
buried; something is locally degenerate about the mesh there, a pocket or a
doubled surface, and no reorientation repairs it.

**The hemicube is the detector.** A self view factor above 0.5 is ground truth
and assumes nothing about shape, and a genuine concavity on these meshes tops
out at 0.351. `heating.pathological_facets` does this. Measured:

| mesh | heuristic flags | self VF > 0.5 | agreeing |
|---|---|---|---|
| Dimorphos 10k | 22 | **1** (243) | 1 |
| Dimorphos 100k | 21 | **3** (5580, 5603, 66473) | 2 |

On the 100k the heuristic also *misses* one. It is a cheap pre-filter, nothing
more, and `flip_facets` is deliberately not wired to it.

### Where they come from: decimation, not the shape models

| mesh | facets | flagged |
|---|---|---|
| Didymos 10k / 100k / full | 10k / 100k / 3,145,728 | 0 / 0 / 0 |
| Dimorphos 10k | 10,000 | 22 |
| Dimorphos 100k | 100,000 | 21 |
| **Dimorphos full** | **3,145,728** | **0** |

**The source models are clean.** Every Didymos model at every resolution
flags zero, and so does the full-resolution 3.1M Dimorphos. The defect is
introduced by decimating Dimorphos, and the right repair is upstream -- a
better decimation from the 3.1M original -- not a per-facet patch. Until then
it is 1 facet of 10,000, 0.01 % of the surface area, and the honest handling
is to know which one it is.

The revised impact: the claim above that 22 facets have sat at night
temperature in every Dimorphos run is wrong. One has. The other 21 receive
sunlight normally and always did.

## 9.3g Heating wired in, and what it is worth

`kalast.tpm.heating` consumes the rows and `tpm_phase2.py` gains a `HEATING`
ablation beside `SHADOW_MODE`: `"none"`, `"self"`, `"mutual"`. Two first-order
terms enter the surface balance,

    eps_i     sum_j VF_ij eps_j sigma T_j^4     thermal re-radiation
    (1 - A_i) sum_j VF_ij A_j S_j               scattered sunlight

with `S_j` the sunlight *incident* on facet `j` before its own albedo, since
that is what it reflects onward. `"self"` and `"mutual"` differ only in which
bodies are allowed to contribute -- the rows are identical and the companion's
columns are left at zero, so the ablation costs nothing to provide.

Insolation is now computed for every body before any of them steps. With the
bodies feeding each other's boundary condition, stepping one first would
advance it against a stale companion -- the same class of mistake as the frame
bug in 7.7.

### What it is worth

One Didymos rotation to the study epoch, against no heating at all:

| | mean | p90 | p99 | max | facets >1 K |
|---|---|---|---|---|---|
| Didymos | +0.07 K | +0.18 | +0.79 | +2.00 | 54 / 10,000 |
| Dimorphos | **+2.29 K** | +4.85 | +11.74 | **+30.18** | **6,578 / 10,000** |

Dimorphos splits +1.69 K self and +0.60 K mutual. Didymos is negligible, which
its 0.0013 self row sums predicted -- the term was never going to matter on a
near-convex body, and measuring it confirms the row sums were telling the
truth rather than hiding a bug.

**Not one facet on either body cooled**, in any of the three nested ablations.
Heating adds a non-negative flux, so `none <= self <= mutual` must hold facet
by facet; it is free to check and it passed exactly. This is the same free
monotonicity that caught the frame bug in 7.7.

### The rebuild cadence, and why one is needed

Self view factors are fixed in the body frame. The mutual ones are not, and
the reason is worth stating because the obvious argument gets it backwards:
Dimorphos is tidally locked, so Didymos sits still in its sky -- but Didymos
**rotates underneath in 2.26 h**, so which of the primary's facets are in view,
and whether those are day side or night side, turns over completely every
rotation. The solid angle barely moves; the temperature behind it swings by
hundreds of kelvin.

Measured against a rebuild every 10 steps:

| cadence | ms/step | Dimorphos mean error |
|---|---|---|
| once | 56 | -0.689 K |
| every 25 | 250 | +0.055 K |
| every 10 | 601 | reference |

Holding the rows for a whole rotation costs 30 % of the term being modelled.
Every 25 steps costs 2 % of it for a fifth of the price, and is the default.
The full 1,308-step segment is about 5.5 minutes. One hemicube pass yields
both blocks, so the static self block is rebuilt alongside the mutual one at
no extra cost -- there is no saving in caching it separately.

### A monotonicity violation that was the shadow map, not the heating

At a cadence of 10 a single Dimorphos facet came out 5.35 K **below** the
unheated run, which cannot happen when the added flux is non-negative. It was
deterministic and reproduced exactly.

Tracing that facet step by step, its heating flux agreed between the two runs
to 0.5 % -- 8.406 against 8.447 W/m2. The entire difference was `facet_shadow`
returning a lit fraction of 0.750 in one run and 0.500 in the other, **for the
same geometry at the same epoch**, worth 139 W/m2 of direct sunlight.

`SAMPLES_PER_FACET` is 4, so the lit fraction is quantised to
`{0, 0.25, 0.5, 0.75, 1}`. The facet sat exactly on the terminator, where one
sample flipping moves the flux by a quarter of full insolation. Runs whose
frame sequences differ -- and inserting rebuild frames changes the sequence --
can land on either side of that knife edge.

So this is a pre-existing bound on how precisely any single facet's
temperature can be trusted near the terminator, about +/-6.6 K on this mesh,
and it has nothing to do with heating. Two things follow: a facet-level
monotonicity check has to tolerate one flipped sample, and a per-facet
temperature quoted at the terminator carries that bar whether or not anyone
draws it.

### What this means for the delivered FITS

The 7.7 product declares mutual and self heating absent, and now there is a
number for what that omission was worth: **negligible on Didymos, +2.3 K in
the mean on Dimorphos and up to +30 K on individual facets**, with 6,578 of
10,000 facets moved by more than 1 K. Combined with the far-plane clipping in
9.3f, which suppressed Dimorphos's mutual term by 4.6x, the secondary's
temperatures in that product are systematically low.

Didymos is unaffected on both counts, so the eclipse conclusions in 7.7 --
which are about the primary -- stand.

## 9.4 Plan

1. Fix (c) immediately — replace `angle_between().cos()` with a dot product.
   Pure win, no behaviour change beyond precision.
2. Fix (a) honestly in the CPU path: subdivide when `d < k sqrt(area)` and
   sum the sub-pairs, rather than returning zero. Slower but correct, and it
   provides the reference the GPU path must reproduce.
3. Build the hemicube pass. Validate against (2) on a small mesh, then against
   analytically known configurations — two coaxial parallel discs and
   perpendicular unit squares both have closed-form view factors, which makes
   this testable the same way §3 tested conduction.
4. Exploit the invariance: for a tidally locked pair the relative geometry
   repeats every orbit, and self-view-factors are fixed in the body frame
   permanently. Compute once, reuse across 2.16M timesteps.

Worth noting the reciprocity check `A_i VF_ij = A_j VF_ji` and the closure
check `sum_j VF_ij <= 1` are cheap and catch most implementation errors — the
current code satisfies neither by construction, since a zeroed near-field pair
breaks reciprocity only if the guard fires asymmetrically, which it does
whenever the two facets differ in area.

### Status

1-3 done (9.3b, 9.3d, 9.3e). 4 turned out not to be the win it looks like: the
self block is indeed invariant, but one hemicube pass produces the self and
mutual blocks together, so caching the self block separately saves nothing --
and the mutual block has to be rebuilt anyway, every 25 steps as 9.3g
measures. The invariance that does pay is the orbit-scale one, not yet
exploited: for a tidally locked pair the *relative* geometry repeats every
orbit, so a run longer than 11.37 h could reuse a table indexed by orbital
phase rather than rebuilding.

Both checks are now applied. Closure holds at 1.00001 on the closed box.
Reciprocity is asserted in `examples/analytical/cavity_heating.py` in its
aggregated form, `sum_i A_i VF_ij = A_j rowsum_j`, because per pair it is
quantisation-limited and mostly noise. Neither was in place when the far-plane
bug of 9.3f went in, and the aggregated form would have caught it.

Still open:

- **Dimorphos's one pathological facet** (243 on the 10k, three on the 100k).
  Not a winding error -- see the correction in 9.3f -- so it needs a better
  decimation from the clean 3.1M original rather than a flip. 0.01 % of area.
- **Multiple scattering** is dropped. Below 0.4 % on Didymos, ~30 % in the one
  Dimorphos concavity whose row sum reaches 0.35.
- **The 2.0 km sweep anomaly** from 9.3f, outside this system's range and
  unexplained.
