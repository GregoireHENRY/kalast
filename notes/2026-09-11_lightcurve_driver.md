# The light curve driver: the piece that turns two halves into a curve

`2026-09-11_HANDOFF_audit_and_photometry.md` closed with "the lightcurve
project for Eli has its pieces now and no driver." This is the driver.

`kalast.scattering` said how bright a facet is. `kalast._rs.shadowing` said how
much of it the Sun and the observer can see. Nothing summed them, so the engine
still could not produce a light curve -- and the disc integral existed only
inside `examples/analytical/shadow_quantisation.py` and `mutual_event.py`, hand
rolled in numpy, twice. `src/lightcurve.rs` puts it where a Rust caller and a
`.py` script reach the same one.

```python
from kalast.lightcurve import Spin, lightcurve
from kalast.scattering import Hapke
import kalast.mesh

mesh = kalast.mesh.Mesh(path="res/ico3.obj")
curve = lightcurve(mesh.positions * [1.6, 1.1, 1.0], mesh.indices,
                   Spin(pole_lon=1.48, pole_lat=-0.70, period=7.63),
                   sun=[1, 0, 0], observer=[0.94, 0.34, 0],
                   epochs=numpy.linspace(0, 7.63, 90),
                   law=Hapke(w=0.10))
curve.magnitude()          # relative magnitudes, ready to plot
```

`examples/lightcurve/main.py` is that, runnable, and needs nothing but `res/`.

## What it computes

```text
F = sum_f  r_f(mu0, mu, alpha) * mu0_f * mu_f * A_f * lit_f * vis_f
```

`F` is in mesh area units per steradian per unit incident irradiance, and
scaling it to a real flux -- `* SOLAR_CONSTANT / r_au^2 / delta^2` -- is left
to the caller, because it needs the mesh's length unit and two distances that
this module is not given. A light curve does not need any of it: it is a ratio,
and `magnitude()` takes the ratio.

Two entry points, because two different things were needed:

- **`flux(vertices, indices, sun, observer, law)`** -- one epoch, geometry the
  caller has already placed. This is the general one, and it is what a
  *binary* goes through: `mutual_event.py` builds two meshes per orbital phase
  and would now call this instead of summing by hand.
- **`lightcurve(..., spin, epochs, ...)`** -- one rotating body. The mesh is
  **not** rotated; the Sun and observer vectors are carried into the body frame
  instead, which is two vectors per epoch rather than every vertex and is
  exactly equivalent, since occlusion along a direction does not care what
  frame the direction is written in.

The spin convention is convex inversion's (Kaasalainen & Torppa 2001) --
ecliptic pole `(lon, lat)`, body-fixed mesh with the rotation axis along `+z`,
`M(t) = Rz(-phase) Ry(lat - pi/2) Rz(-lon)` -- so a published pole solution
goes in without translation. Radians throughout, which the literature is not:
`numpy.radians(85.0)` at the call site is deliberate.

### Two laws, not four

`Law` has two variants: Hapke, and the `c LS + (1-c) Lambert` mix, which *is*
Lambert at `c = 0` and pure Lommel-Seeliger at `c = 1`, exactly. So
`scattering::LommelSeeligerLambert` is new -- the parameter set beside the
existing formula, so that "a law" can be one argument -- and three of the four
laws in that module are one struct.

### The convex shortcut is not an approximation

Most shape-inversion models are convex by construction, and a convex body has
no facet occluding another: `mu0 > 0` already means fully lit. `shadowing` and
`visibility` can therefore be turned off with no loss at all, and that is the
difference between **11.6 ms and 0.021 ms an epoch** at 1280 facets. A fitting
loop evaluating a 90-point curve pays 1.9 ms instead of 1.0 s.

`tests/test_lightcurve.py` pins **both halves** of that claim: the two agree on
a sphere, and disagree on a cratered one. Pinning only the first would pass
just as well if the clipping never ran.

## The closed forms it is tested against

Each has an answer known independently of this code. That mattered: the sum is
where a wrong normal, a dropped area or a mirrored rotation hides, and none of
those is visible in the two halves that were already tested.

**1. A Lommel-Seeliger body at zero phase is exactly its projected area.** At
`alpha = 0`, `mu0 = mu`, so `r mu0 mu = w mu / (8 pi)` and the whole sum
collapses to `sum A mu` with every trace of the reflectance gone but a
constant. A triaxial ellipsoid rotating equator-on then has the closed-form
curve `sqrt(a^2 sin^2 + b^2 cos^2)` and amplitude `2.5 log10(a/b)`.

Measured: **752.575 mmag against 752.575 mmag** for a 2:1 body.

**2. A Lambert sphere follows the analytic phase function**,
`(albedo/pi) (2/3) [sin a + (pi - a) cos a]`.

**3. Occlusion is a no-op on a convex shape, and each pass separately is not on
a concave one.**

**4. Helmholtz reciprocity.** Every law here is symmetric in `mu0 <-> mu`, so
swapping the Sun and the observer cannot change the flux.

## Four things measuring it taught

**The first version of the concave test had a dead Sun pass and passed
anyway.** It used a 6-crater sphere and asserted only that occlusion *changed*
the flux. It did -- by 6 mmag -- and the whole 6 mmag came from the observer
pass: at that Sun direction the shape self-shadowed **nothing**, weighted lit
fraction 1.000000 to six figures. So a completely dead Sun-side clipping would
have passed. The fix is to test each pass on its own, which now reads 2.0 mmag
for the Sun and 24.9 mmag for the observer at `alpha = 70 deg` on a 20-crater
shape. *A test that a thing "has an effect" does not say which thing.*

**An identity and a discretisation error look the same until you refine.** The
ellipsoid gives both at once, and they behave completely differently:

| | ico1 (80) | ico2 (320) | ico3 (1280) | ico4 (5120) |
|---|---|---|---|---|
| amplitude error, mmag | 0.0003 | 0.0001 | 0.0003 | 0.0039 |
| curve-shape scatter | 2.83e-3 | 8.80e-4 | 2.40e-4 | 5.78e-5 |

The **amplitude** is exact at 80 facets: an ellipsoid is a linear map of a
sphere and an icosphere's projected area is isotropic to ~1e-7, so there is
nothing left to converge and the residual is f32 rounding, which is why the
5120 row is the *worst* of the four. The **curve shape** is not exact at any
resolution -- a polyhedron's projected area is its own, not the smooth
ellipsoid's -- and converges as `N^-0.94`. A fixed tolerance on the second
would have been set wherever 2.4e-4 happened to land, i.e. it would have
measured the mesh. Same lesson as the conduction reference and the polygon
clipper, arriving a third time from a new direction.

**Five deliberate breaks, and two of them were each caught by exactly one
test.**

| break | caught by |
|---|---|
| facet area dropped from the weight | 4 checks + the Rust sphere test |
| emission cosine `mu` dropped | 6 checks + the Rust sphere test |
| `lit` factor dropped | the Sun-pass check, and reciprocity |
| rotation mirrored in time | **only** `rotation_is_prograde` (Rust) |
| both clippings run along the Sun | **only** reciprocity |

The last two are the argument for having written them. A mirrored rotation is
invisible to every curve check here, because the ellipsoid curve is symmetric
in `phi -> -phi` -- so the light curve of a real target would come out
*time-reversed* with every test green. And clipping twice along the Sun still
changes the flux, so "occlusion has an effect" sees nothing; only the
`mu0 <-> mu` symmetry does.

**A conservation-style identity earns its place by being cheap and blind.**
Reciprocity took four lines and caught two independent bugs it was not aimed
at. The view-factor work said the same thing in August about a different
identity.

## Two things found on the way, both pre-existing

**Every generated stub's numpy annotations were invalid.** `resolve()` in
`tools/gen_stubs.py` rewrote unknown names to `object` identifier by
identifier, so `numpy.ndarray` became **`numpy.object`** -- an attribute numpy
removed in 1.24. A type checker rejects it, which is precisely the failure
`resolve` exists to prevent, and it stood in **53 places across 9 committed
stub files**. Fixed by resolving a dotted name by its root.

**`test_stubs`' hand-maintained case list is now loud rather than silent.**
Open item 4 of the handoff: a class absent from the list is simply not looked
at, which is how `Hapke` shipped a stub naming a method it does not have.
`test_every_generated_stub_class_is_covered` now fails when a class appears in
a generated stub and in neither the case list nor an explicit `UNCOVERED` set.
It immediately found two classes my own count had missed -- two different
`Body` pyclasses in two files -- and it documents **16 pre-existing classes
that nothing checks**, each needing a live object this test cannot cheaply
build. That is a backlog that can now shrink but not grow.

It does not construct objects automatically, which is what would remove the
list entirely. It removes the *silence*, which is the part that bit.

### And two binding papercuts

Both found by writing the example rather than by reasoning:

- `indices` now takes `(m, 3)` **or** flat `(3m,)`, because flat is what
  `kalast.mesh.Mesh.indices` hands back. `lit_fractions` gets this too.
- `vertices` now takes any float dtype. `mesh.positions * [1.6, 1.1, 1.0]` is
  float64, and feeding it back gave
  `TypeError: argument 'vertices': 'ndarray' object is not an instance of
  'ndarray'` -- a message that names the real problem nowhere.

Together these are why the example needs no `.obj` loader of its own. Three
scripts in this repo carry one; the engine has had `kalast.mesh.Mesh` all
along.

## Cost

Median of 5, release, this Mac, per epoch -- both directions clipped and the
sum:

| facets | exact | convex |
|---|---|---|
| 320 | 2.7 ms | 0.006 ms |
| 1280 | 11.6 ms | 0.021 ms |
| 5120 | 48.3 ms | 0.081 ms |
| 20480 | 201.9 ms | 0.329 ms |

**These do not contradict the 7 / 34 / 156 ms in
`2026-09-10_polygonal_shadowing_implemented.md`, they are a different
machine.** `lit_fractions` alone, one direction, measures 1.4 / 5.7 / 24.1 ms
here against that note's 7 / 34 / 156 -- a flat ~5x, and this driver is two of
those calls plus a sum that does not register. Anything quoted from either note
should say which machine it came from.

## What the law is worth, which bears on `theta_bar`

Measured with the new driver, since it is the first time it could be: a
1.6 : 1.1 : 1.0 body, pole (85, -40), full rotation at 72 epochs, every curve
normalised to its own median so only the *shape* is compared. `rms` and `peak`
are against pure Lommel-Seeliger, in mmag.

| law | amplitude, alpha=0 | rms | peak | amplitude, alpha=60 | rms | peak |
|---|---|---|---|---|---|---|
| Lambert | 656.6 | 90.20 | 139.68 | 614.7 | 46.64 | 66.52 |
| Lommel-Seeliger | 402.9 | -- | -- | 487.3 | -- | -- |
| LS+L mix, c=0.9 | 492.9 | 31.98 | 47.88 | 533.1 | 16.91 | 24.58 |
| Hapke w=0.10 | 404.9 | 0.87 | 2.95 | 490.7 | 1.29 | 1.81 |
| Hapke w=0.35 | 412.0 | 3.30 | 6.78 | 501.4 | 5.28 | 7.47 |
| Hapke, no opposition surge | 406.8 | 1.40 | 2.24 | 491.0 | 1.38 | 1.95 |

Two things, and the second is the surprise.

**The limb-darkening family dominates, and it is not a detail.** Lambert makes
the *same body* look 63 % more elongated than Lommel-Seeliger does -- 656.6
against 402.9 mmag. A shape fitted with the wrong family gets the wrong axis
ratio, not a slightly noisier one. Even the weight inside the standard mix
matters: `c = 0.9` instead of 1.0 moves the amplitude 22 %.

**Within a family, everything else is a few mmag.** Hapke at an asteroid
albedo is Lommel-Seeliger to **0.87 mmag rms** -- which is the physics, since
Hapke's single-scattering term *is* Lommel-Seeliger-like and multiple
scattering is negligible at `w = 0.1`. Tripling the albedo to `w = 0.35` costs
3 to 5 mmag. Deleting the opposition surge entirely costs 1.4.

So the parameters of the model, as opposed to the choice of model, move a
normalised rotational curve at the mmag level -- at or under the noise of
ordinary ground-based relative photometry.

## Open

- ~~**`theta_bar`**~~ **Implemented** the same day --
  `notes/2026-09-11_hapke_roughness.md`. And the placement below turned out to
  be **wrong by an order of magnitude**: reasoning by analogy with `w` and `b0`
  said a few mmag, and measuring it says 23 mmag rms and a **15 % change in the
  light curve amplitude** at 20 deg phase. The analogy failed because roughness
  acts where the limb and terminator are, and how much of the disc that is
  changes as an elongated body turns -- so it moves the curve's *shape*, where
  `w` and `b0` mostly move its level. The original text is kept below because
  the reasoning is the instructive part.

- **`theta_bar`, and what the table above does and does not say about it.**
  It is the only known gap left in the photometry. The integral refuses a
  non-zero one rather than dropping it, and `tests/test_lightcurve.py` pins
  that it still refuses from inside the disc integral.

  The table places it by analogy, not by measurement: `theta_bar` is one
  parameter of Hapke's model, like `w` and `b0`, and at fixed phase angle its
  effect on a *relative* rotational curve is a change in effective limb
  darkening -- the same kind of perturbation those two make, which is 1 to 7
  mmag. **That is an argument, not a number, and the honest reading is that
  the last un-measured term in the photometry is the one still missing.**
  Everything else here was settled by measuring it.

  Where it would stop being second-order: an **absolute phase curve** over a
  wide range of `alpha`, where roughness is the dominant term at large angles
  and is not divided out by normalisation; **disc-resolved** data, where
  per-facet radiance near the limb is the observable; and simply **loading a
  published parameter set**, since the field quotes `theta_bar` of 20-30 deg
  routinely and kalast cannot accept one today.

  The objection recorded for skipping it was that it is "a page of case
  analysis with no closed form to test against". Half of that has since
  dissolved: Hapke's 1984 formulation is *constructed* to preserve Helmholtz
  reciprocity, the `i <= e` and `i > e` branches existing for that reason --
  and reciprocity is already a check in `tests/test_lightcurve.py`, where it
  caught two bugs it was not aimed at. A wrong branch in a case analysis is
  exactly what it detects. With `theta_bar -> 0` reducing to the smooth case,
  continuity across the `i = e` boundary, and `S <= 1`, there is a real test
  battery here without a closed form.
- **The mix has no phase function**, so a fit spanning a range of `alpha` wants
  Hapke, or an empirical phase function applied outside. Stated in the docs,
  not implemented.
- **Light-time and aberration are the caller's**, which in practice means
  SPICE. The driver takes directions.
- **A binary needs the caller to place the geometry per epoch** and call
  `flux`. That works today and is what `mutual_event.py` does by hand; what is
  missing is the convenience that takes two bodies and an orbit.
- **Nothing reads real photometry yet.** A fit needs an observed curve,
  chi-squared and a minimiser. The forward model is what exists now.
