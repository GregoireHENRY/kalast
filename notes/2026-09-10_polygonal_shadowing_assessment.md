# Brož's polygonal shadowing: worth adding, not worth swapping in

New project item, raised 10 September: **lightcurve simulation of asteroids
from ground-based telescopes, for Eli.** Nothing in `notes/` mentioned it
before, so it is new rather than a continuation.

The question attached to it: Brož's "polygonal algorithm, partial
shadowing/visibility" — is it good, and how does it compare with what kalast
already does?

## The slides are not on this machine

Asked for `/Users/gregoireh/Documents/events-meeting/2024.02-houches/presentations/11_Broz.pdf`,
slides 48-52. That is the macOS path; nothing resembling it exists on the
Windows box, and a search for `*Broz*` and `*houches*` under the user tree
found nothing. **This assessment is from the underlying paper, not the
slides**, so if the slides present a variant or a later version, this may be
behind.

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

## What would settle it, and is not done

The decisive number is missing: **how large is the quantisation error, in
mmag, on a real synthetic light curve?** I have argued it is disqualifying
from the mechanism, not measured it.

It is measurable with what exists today — take a non-convex shape, rotate it,
and compute the disc-integrated flux twice: once with `facet_shadow`'s
quarters, once with a heavily supersampled ray-traced lit fraction as a stand
in for exact. The difference, in magnitudes, is the size of the prize. If it
comes out at 0.1 mmag the case collapses; if it is several mmag the case is
made.

That is the next thing to do before writing any Clipper2 code.

## Sources

- Brož et al. 2023, A&A, [arXiv:2306.04768](https://arxiv.org/abs/2306.04768) —
  the algorithms, §3 and §4.
- [Xitau](https://sirrah.troja.mff.cuni.cz/~mira/xitau/) — the tool.
- Vatti 1992, Comm. ACM 35, 56 — the clipping algorithm.
- [Clipper2](https://github.com/AngusJohnson/Clipper2) — the implementation.
- Delbo et al., *Asteroid thermophysical modeling*,
  [arXiv:1508.05575](https://arxiv.org/abs/1508.05575) — review context for
  where partial shadowing sits in TPM practice.
