# Brož's polygonal shadowing: worth adding, not worth swapping in

New project item, raised 10 September: **lightcurve simulation of asteroids
from ground-based telescopes, for Eli.** Nothing in `notes/` mentioned it
before, so it is new rather than a continuation.

The question attached to it: Brož's "polygonal algorithm, partial
shadowing/visibility" — is it good, and how does it compare with what kalast
already does?

## The slides, and where to get them

First asked for as `/Users/gregoireh/Documents/events-meeting/2024.02-houches/presentations/11_Broz.pdf`
-- the macOS path, which does not exist on the Windows box. They are on the
ROB cloud, the same host `README.rst` points at for `res/`:

    https://cloud-as.oma.be/index.php/s/dG9oeb422NZcGtD

That link serves a **532 MB zip of the whole `2024.02-houches/` folder**, not
one file; `curl -sL -o houches.zip <url>/download` then
`unzip -j houches.zip '2024.02-houches/presentations/11_Broz.pdf'` gets the
36 MB deck (96 pages) without unpacking the videos beside it.

Read, and it agrees with the paper -- slide 48 is the algorithm exactly as
described below. What the slides add over the paper is at the end of this
note.

Primary reference: **Brož et al. (2023), A&A**, *"2021 occultations and
transits of Linus orbiting (22) Kalliope. I. Polygonal and 'cliptracing'
algorithms"*, [arXiv:2306.04768](https://arxiv.org/abs/2306.04768), sections 3
and 4. Implemented in their modelling tool
[Xitau](https://sirrah.troja.mff.cuni.cz/~mira/xitau/).

## What the algorithm actually does

Two separate algorithms, and only the first is about shadowing.

### The polygonal light curve algorithm (§3)

Each facet is projected **twice**, and clipped in each projection:

1. **Along the asteroid→Sun vector** `s`, giving basis
   `ŵ = s, û = (−sin l, cos l, 0), v̂ = −û × ŵ`. Facets are clipped against
   each other in this plane; what survives is the **illuminated** part.
2. **Along the asteroid→observer vector** `o`, same construction. What
   survives that is the **visible** part.

The clipping is analytic 2D polygon intersection — Vatti (1992), via the
[Clipper2](https://github.com/AngusJohnson/Clipper2) C++ library — and the
result is back-projected onto the facet's own plane with

    z = (d − ax − by) / c,     (a,b,c) = n̂,  d = n̂·c

so the surviving area is measured on the real facet, not in projection. The
area comes out of a signed sum over the polygon's points, and **the sign test
matters**: it is what makes small-polygon-inside-large-polygon (annular
eclipse) come out right.

Facets with `µi ≤ 0 ∧ µe ≤ 0` are dropped up front, and bounding-box tests
cull the pairs before any clipping is attempted.

The output per facet is a **real-valued lit-and-visible area**, exact up to
the shape's own discretisation.

### Cliptracing (§4)

A different problem: synthetic *images*, not light curves. A pixel is treated
as a polygon in the sky plane and facets are clipped by it, so each pixel's
contribution is exact rather than an inside-triangle test at the pixel centre.
Their Fig. 4 is a side-by-side against plain ray tracing — the ray-traced
image is visibly noisy, the cliptraced one smooth.

Relevant to kalast only if we ever fit AO/resolved imagery. Not relevant to
Eli's ground-based photometry, where the asteroid is a point source.

### What they claim

- Light curves precise to **< 0.1 mmag**, and smooth.
- That precision holds at **42 nodes per sphere** — a very coarse shape. Fig. 2
  shows 42, 272 and 1123 nodes essentially on top of each other.
- Annular eclipses exact even at coarse discretisation (their Fig. 3).
- Partial eclipses, partial transits and partial visibility all handled by the
  same mechanism, because they are all just polygon intersections.

**They publish no timings.** No CPU seconds, no facets-per-second, no
complexity statement, and no comparison against ray tracing for *speed* — the
ray-tracing comparison in Fig. 4 is about image quality only. Every
performance statement below is my inference from the method, not their
measurement.

## How it compares with what kalast has

| | kalast today | polygonal |
|---|---|---|
| mechanism | 8192² shadow map + compute pass | analytic 2D polygon clipping |
| lit fraction per facet | **quantised to {0, ¼, ½, ¾, 1}** (`SAMPLES_PER_FACET = 4`) | exact real number |
| partial visibility at the limb | **not computed at all** | exact |
| mutual events (binary) | via the same map, same quantisation | exact, incl. annular |
| tuning constants | 3, fitted per scene (slope, floor, normal offset) | none |
| measured error | 250 false-lit of 2048 facets over 15 Sun angles; worst angle 1.39 % | shape discretisation only |
| cost | one GPU render + one compute pass, O(N) | CPU pairwise clipping, bounding-box culled |
| scales to | 3.1M facets at tens of it/s | ~10⁴ facets, realistically |

Two of those rows are the whole argument.

**The quantisation is disqualifying for photometry.** kalast samples four
points per facet — three vertices and the centroid — so a facet is 0, 25, 50,
75 or 100 % shadowed and nothing between. That is entirely reasonable for the
thermophysical model, where a facet's temperature responds to insolation
averaged over a rotation and the quantisation error averages out. It is the
wrong instrument for a light curve, where the observable *is* the summed lit
area as a smooth function of rotation phase, and a staircase in each facet's
contribution is a staircase in the curve. Brož gets < 0.1 mmag; a quarter of a
facet is nowhere near that.

**kalast has no partial-visibility treatment at all.** A facet is either
included in the disc integral or not. For a single body at low phase that
costs little. For **mutual events between two bodies** — which is Hera's
business as much as Kalliope's — the partial coverage during ingress and
egress *is* the signal, and the shadow map quantises exactly the quantity
being measured.

Against that, the polygonal method is CPU work whose cost grows with pairs of
facets. Bounding-box culling makes that far better than N², but it is
nonetheless the wrong tool for a 3.1M-facet thermophysical run, where the
shadow map does one rasterisation pass and is done. **These are different
regimes, not competitors.**

## Recommendation

**Implement it, as an additional path for photometry. Do not replace the
shadow map.**

- For lightcurve inversion the shapes are convex-inversion, SAGE or ADAM
  models — 10³ to 10⁴ facets. The polygonal method is comfortable there and
  the shadow map's quantisation is not.
- It removes three fitted constants from the answer. The shadow-map bias is
  scene-dependent and has been wrong once already in a way that mattered:
  Deimos reported 0.55 % of facets shadowed against a true ~46 %, because the
  scene-wide bias was applied to a small body
  (`src/app/facet_shadow.rs`). A method with no bias constant cannot fail that
  way.
- kalast already computes disc-integrated flux as a direct facet sum rather
  than by summing a render — `tiri_deimos_photometry.py` does exactly that,
  for the documented reason that a 35-pixel render of Deimos disagreed with
  itself by 11 % between sub-pixel phases. The polygonal method drops into
  that sum: it replaces `lit` and adds a visibility weight. **The architecture
  already fits.**

### The bigger gap is probably not shadowing

For *optical* ground-based lightcurves the model needs a bidirectional
scattering law — Brož's Eq. (18), `Iλ = f(fL, µi, µe, α) Φi`, with Lambert,
Lommel-Seeliger or Hapke. kalast has **none of that in Rust**: `grep` for
Lambert/Hapke/Lommel across `src/` returns only `roughness.rs`, which is the
Kuehrt crater correction and a different thing. `tiri_deimos_photometry.py` is
*thermal* — it integrates radiance from temperatures, which is TIRI's problem,
not a V-band photometer's.

So the order of work for Eli is probably: **scattering law first, polygonal
shadowing second.** Exact lit areas feeding a missing reflectance model buys
nothing.

### Implementation notes

- Rust bindings to the same library Brož uses exist:
  [`clipper2`](https://crates.io/crates/clipper2) (FFI to the C++), and a pure
  Rust port, [`clipper2-rust`](https://crates.io/crates/clipper2-rust).
  Alternatives are `geo-clipper` and `rust-geo-booleanop` (Martinez-Rueda).
- **Clipper2 works in i64 internally** and takes f64 at its surface, precisely
  for robustness. That is worth knowing next to today's `Float = f32`
  decision: it does not conflict, but the clipping must be done in f64 and
  converted at the boundary. Doing polygon clipping in f32 invites exactly the
  degenerate-case failures the i64 design exists to avoid.
- This belongs in Rust with a Python binding, per the project rule — a Rust
  example and a `.py` script should reach it identically.

## Measured: the quantisation costs 0.7 to 40 mmag

The argument above was made from the mechanism. Here is the number, from
`examples/analytical/shadow_quantisation.py`.

A cratered icosphere, rotated through 24 phases at 30 deg phase angle, Lambert
scattering, disc-integrated three ways off one ray tracer so that only the
sampling differs: a converged barycentric reference, the 4 points
`facet_shadow` actually uses, and that plus binary per-facet visibility. The
crater depth and width set how much of the illuminated area is shadowed, which
is the independent variable — a convex body self-shadows nowhere and would
measure nothing.

| shape | facets | shadowed | q4 rms | q4 peak | q4+bv rms |
|---|---|---|---|---|---|
| mild | 320 | 6.0 % | 4.55 ± 0.30 | 10.90 | 5.21 |
| moderate | 320 | 10.1 % | 18.11 ± 1.05 | 28.62 | 17.96 |
| strong | 320 | 23.2 % | 40.61 ± 2.40 | 74.78 | 39.73 |
| mild | 1280 | 5.8 % | 1.33 ± 0.08 | 3.52 | 2.35 |
| moderate | 1280 | 10.6 % | 3.92 ± 0.23 | 8.65 | 4.08 |
| strong | 1280 | 24.4 % | 10.75 ± 0.55 | 21.47 | 10.87 |
| **mild** | **5120** | **5.7 %** | **0.68 ± 0.03** | 1.92 | 0.91 |
| moderate | 5120 | 10.5 % | 1.21 ± 0.06 | 3.32 | 1.75 |
| strong | 5120 | 24.1 % | 3.88 ± 0.14 | 8.10 | 4.35 |

mmag, against the converged reference. The ± is how far that reference still
moves between 153 and 231 samples per facet — the residual error on the
yardstick itself, which is well under the signal in every row.

**The case is made.** Even the best row — 5120 facets, only 5.7 % of the
illuminated area shadowed — costs **0.68 mmag rms**, seven times Brož's
0.1 mmag. A moderately cratered 1280-facet shape, which is an ordinary
lightcurve-inversion model, costs 3.9 mmag rms and 8.7 mmag peak. That is not
a subtle effect; it is larger than the features people fit shapes to.

### Refining the mesh does not rescue it

The error falls with facet count as roughly `N^-0.5` to `N^-0.85` — the
exponents over the last 4x step are 0.48 (mild), 0.85 (moderate), 0.73
(strong). Extrapolating the *best* case, 0.68 mmag at 5120 facets, down to
0.1 mmag needs a factor 6.8, which is **50,000 to 240,000 facets** depending
on the exponent. For a shape model that is one to two orders of magnitude
beyond the 10³–10⁴ these methods produce, and it buys a lit-area estimate that
polygon clipping gets exactly at 320.

### A correction to what this note said earlier

I claimed partial visibility at the limb was a second, comparable gap. **On
this test it is not**: `q4+bv` sits within a factor of two of `q4` everywhere
and is sometimes *better*, so for a single body at 30 deg phase, binary
visibility costs little next to the shadow quantisation. The claim was about
mutual events — ingress and egress of one body across another — and that case
is **not tested here**. It remains plausible and unmeasured; treat the
single-body result as the only one with evidence behind it.

### What this measurement is not

- It isolates the *sampling*. The shadow map's own bias and resolution error
  sit on top, and are bounded separately by `tests/test_facet_shadow.py`.
- Lambert, not Hapke. The absolute mmag would shift with a real scattering
  law; the ratio between the two curves is what matters and both use the same.
- Gaussian dimples on an icosphere, not a real shape. The independent variable
  is "fraction of illuminated area shadowed", which is measurable on any
  shape, so a real target can be placed on this table rather than guessed at.

## What the slides add over the paper

Three things worth having.

**The clippings are a sequence of three, not two.** The paper splits them
across two sections and it reads as two separate algorithms; slide 48 and
slide 52 make the structure plain:

| | clip against | gives |
|---|---|---|
| 1st | the Sun projection | partial **shadowing** |
| 2nd | the observer projection | partial **visibility** |
| 3rd | the pixel, as a polygon | partial **flux contribution** (cliptracing) |

Each with a back-projection onto the facet plane. Slide 48's own summary of
the benefit is `'killed' d. errors` -- discretisation errors killed, which is
the honest claim: the shape's own discretisation remains, everything the
sampling used to add does not.

**The scattering law is Hapke.** Slide 51, "exact light curve / scattered
light / Hapke law". That settles what the exact areas are for: they feed a
bidirectional reflectance, and getting the areas exact while the reflectance
is missing or crude buys nothing. It also names the target for kalast, which
has no scattering law at all -- Hapke rather than Lambert or Lommel-Seeliger.

**The lineage is eclipsing-binary stars.** Slide 48 cites Prša et al. (2016),
i.e. Phoebe2, and the paper says the approach is Phoebe2's "but complicated by
the fact that we have to compute not only the visibility, but also non-convex
shadowing, which is critical for asteroids". So this is a mature stellar
technique carried into asteroids, with self-shadowing as the new part. Worth
knowing: the polygon-clipping half has been exercised for years on binaries.

**And `Xitau` is readable.** Slide 54: source at
`http://sirrah.troja.mff.cuni.cz/~mira/xitau/`, Fortran 90 with older F77
parts, all published models with input and output, and -- the useful bit --
"always use the corresponding-date version", the polygonal one being
`xitau_20240124_POLYS`. So the reference implementation can be read directly
rather than reconstructed from the paper.

**Still no timings.** Searched the whole 96-page deck for CPU, speed, cost and
performance. The only CPU figure is "1 week on 100 CPUs" on slide 61, and that
is about mapping local minima in the *dynamical* fit for Kleopatra's
satellites -- nothing to do with the cost of the polygonal algorithm per light
curve point. So the performance column of the comparison above remains
inference from the method, in both sources.

## Sources

- **Brož, M., Ďurech, J., Ferrais, M., Lee, H.-J., Kim, M.-J., Roh, D.-G.,
  Yim, H.-S., Jehin, E., Burdanov, A., de Wit, J., Fatka, P., Hanuš, J. &
  Carry, B. (2023).** *2021 occultations and transits of Linus orbiting (22)
  Kalliope. I. Polygonal and 'cliptracing' algorithms.* **A&A 676, A60.**
  [doi:10.1051/0004-6361/202346326](https://doi.org/10.1051/0004-6361/202346326),
  [arXiv:2306.04768](https://arxiv.org/abs/2306.04768). The algorithms are §3
  (light curves) and §4 (cliptracing).
- [Xitau](https://sirrah.troja.mff.cuni.cz/~mira/xitau/) — the tool.
- Vatti 1992, Comm. ACM 35, 56 — the clipping algorithm.
- [Clipper2](https://github.com/AngusJohnson/Clipper2) — the implementation.
- Delbo et al., *Asteroid thermophysical modeling*,
  [arXiv:1508.05575](https://arxiv.org/abs/1508.05575) — review context for
  where partial shadowing sits in TPM practice.
- Prša et al. 2016 — Phoebe2, where the polygon-clipping approach comes from.
- The slides themselves: `11_Broz.pdf`, Les Houches, February 2024, on the ROB
  cloud at the link above.
