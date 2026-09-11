# Hapke's macroscopic roughness, and why the estimate was wrong

`theta_bar` was the last known gap in the photometry, and
`2026-09-11_lightcurve_driver.md` closed by saying the decision about it
"wants a deliberate decision, not drift". Decided: **implemented**, Hapke
(1984), chapter 12 of *Theory of Reflectance and Emittance Spectroscopy*,
eqs. 12.45-12.55.

## The estimate it replaces, which was wrong

Before implementing it I placed it by analogy against the other Hapke
parameters, since those *could* be measured: tripling `w` moves a normalised
rotation curve 3-5 mmag rms, deleting the opposition surge 1.4. `theta_bar`
is one parameter of the same model, so -- the argument went -- a few mmag, at
or under the noise of ground-based relative photometry.

**That was wrong, and by an order of magnitude.** Measured, now that it exists:

| | alpha=0 | alpha=20 | alpha=60 | alpha=90 |
|---|---|---|---|---|
| rms vs `theta_bar = 0`, mmag | 0.52 | 23.06 | 28.00 | 27.47 |
| curve amplitude, mmag | 401.3 | 505.4 | 564.4 | 560.3 |
| amplitude with `theta_bar = 0` | 402.7 | 440.6 | 489.5 | 496.7 |

`theta_bar = 30 deg`, a 1.6 : 1.1 : 1.0 body, pole (85, -40). At an ordinary
observing geometry it changes the light curve **amplitude by 15 %** -- which
is not an offset a relative light curve divides out, it is the quantity an
axis ratio is fitted to. 23 mmag rms is well above ordinary photometric noise.

Why the analogy failed: `w` and `b0` change the reflectance fairly uniformly
over the body, so they mostly move the curve's *level*. Roughness changes it
by an amount that depends on the local slope distribution relative to the
illumination, which is concentrated near the limb and terminator -- and the
share of the disc that is limb and terminator changes as an elongated body
turns. It is structurally a different kind of parameter, and reasoning by
analogy could not see that.

*Every other term in this photometry was settled by measuring it. This one had
been settled by arguing about it, and the argument was wrong.*

On a **disc-integrated sphere** the phase-curve effect is much larger still,
and behaves exactly as the mechanism says it should -- nothing at opposition,
growing without limit toward grazing:

| alpha | 10 deg | 20 deg | 30 deg | 40 deg |
|---|---|---|---|---|
| 0 | 0.1 | 0.3 | 0.6 | 1.1 |
| 20 | 13.2 | 48.4 | 99.1 | 164.4 |
| 60 | 23.2 | 100.6 | 224.3 | 393.1 |
| 100 | 58.4 | 271.7 | 600.3 | 989.0 |

mmag of darkening against `theta_bar = 0`, by `theta_bar` across the columns.
So anyone fitting an absolute phase curve without roughness is absorbing up to
a magnitude into something else.

## What the correction is

Two parts, neither a fudge factor. The effective cosines `mu0e` and `mue`
replace `mu0` and `mu`, because a tilted facet inside the unresolved relief is
not lit or viewed at the angle the mean surface implies; and a shadowing
function `S` handles relief hiding relief.

Both need the **azimuth** `psi` between the planes of incidence and emergence,
not just the phase angle. `reflectance` is given `(mu0, mu, alpha)`, so `psi`
is recovered from `cos alpha = mu0 mu + sin i sin e cos psi`. `Hapke::
roughness_terms` returns `(mu0e, mue, S)` for anyone comparing against another
implementation.

## The tests, and the two blind spots they had first

The recorded objection to implementing it was "a page of case analysis with no
closed form to test against". The first half is true. The second is not, and
the identities turned out to be sharper than a table would have been -- but
**both of the first two versions of them were blind**, each in a different way,
and only breaking the code showed it.

**Reciprocity cannot see an inverted branch condition.** Hapke writes `mu0e`,
`mue` and `S` differently for `i <= e` and `i > e`, and those branches exist to
make the model reciprocal. So reciprocity is the obvious test, and it is a good
one: it catches a wrong term inside a branch instantly (worst asymmetry
9.0e-01 when the shadowing denominator is given the wrong ratio). But swapping
the two branches *wholesale* leaves it **completely green**, because the
branches are each other's mirror image -- exchanging them preserves the very
symmetry they were built to provide.

What does catch it: **`S = 1` at zero azimuth**, which holds on the `i <= e`
branch and not the other, where `S = mue mu0 / (mu0e mu)`.

**And that test had a numerical blind spot of its own.** Its first version used
`i` of 0.3, 0.2 and 0.05 rad -- near-normal incidence, where `cot i` is large,
`E2(i)` underflows, the correction terms drop out of *both* branches and `S`
comes to 1 either way. So it passed against inverted branches too. It
discriminates only where both angles are far enough from normal for `E2` to
survive: at `i = 0.8, e = 1.1` the inverted version reads `S = 1.067`.

That is a new way for a test to be unable to fail, and worth having beside the
others: not a tolerance too loose, not a conservation law satisfied at a
convenient parameter value, but a whole test sitting in a region where the term
it probes has underflowed.

The rest of the battery: `theta_bar -> 0` reduces to the smooth formula, tested
by convergence and bounded by the f32 floor -- it reaches *exactly* zero by
1e-3, so a strictly-decreasing assertion fails on correct code; continuity
across `i = e`; roughness vanishing at opposition as `w` vanishes, which pins
the azimuth recovery; and monotone darkening away from opposition, which is
what makes the reduction test non-vacuous.

## Open

- **The mix still has no phase function**, so a fit spanning a range of `alpha`
  wants Hapke. Unchanged.
- **Nothing reads real photometry.** The forward model is complete now; a fit
  needs observed curves, a chi-squared and a minimiser.
- **A binary still needs its geometry placed per epoch** by the caller.
- `bond_albedo()` is the smooth-surface closed form. Roughness changes the
  Bond albedo and that is not reflected there.
