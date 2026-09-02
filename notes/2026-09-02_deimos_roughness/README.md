# Deimos radiance, and what closes the gap

The simulated TIRI radiance for the Mars swing-by came out **34 % low** against
the calibrated product. This is what that turned out to be.

## The gap

At the best-resolved frame -- 12:08:36, 977 km, Deimos 57 px across, the only
one where the observed disc average is not badly diluted by background:

| | disc-averaged wide-band radiance | brightness temperature |
|---|---|---|
| observed (JAXA/VITO/ROB) | 21.07 W m^-2 sr^-1 | 290.9 K |
| modelled, smooth | 14.00 | 269.0 K |
| | **ratio 0.664** | **-21.9 K** |

Two features of the disagreement pointed away from a simple offset:

- The modelled disc average is **flat with range** (14.1-15.6 over a factor of
  eight in distance), as conservation of radiance along a ray demands. The
  observed figure rises 2.96 -> 21.07 over the same span, so it is dominated by
  background dilution at small angular size and only the closest frames compare
  at all.
- The modelled **peak** pixel is 302 K while the observed disc *average* is
  291 K. The real disc is nearly isothermal where the model has a hot subsolar
  point and a cold limb: **too much thermal contrast, not too little heat**.

## Thermal inertia is not the answer, and moves the wrong way

| inertia | disc average |
|---|---|
| 10 | 14.27 |
| 20 | 14.00 |
| 30 | 13.74 |

Raising it *lowers* the disc average -- more heat is carried to the night side
at the expense of the dayside being viewed near zero phase. The whole plausible
range spans 0.5 W m^-2 sr^-1 against a shortfall of 7.

## Roughness is, and at the value already published for Deimos

The Kuehrt spherical-crater beaming correction, ported to Rust and validated
(see `2026-08-27_conduction_solvers/` section 8.2 for why this route). Sweeping
crater coverage, expressed as RMS slope, at a 75 degree emission bound:

| RMS slope | coverage | disc average | sim/obs | BT gap |
|---|---|---|---|---|
| 0 (smooth) | - | 14.00 | 0.664 | +22.30 K |
| 20 | 0.166 | 15.43 | 0.732 | +17.29 |
| 30 | 0.374 | 17.22 | 0.817 | +11.42 |
| 40 | 0.664 | 19.73 | 0.936 | +3.82 |
| 43 | 0.768 | 20.62 | 0.979 | +1.27 |
| **46.6** | **0.90** | **21.76** | **1.033** | **-1.91** |

**The last row is not a fit.** Giese and Kuehrt (1990), *Crater Deimos:
Interpretation of IR Measurements*, derive a crater density of **0.9 for nearly
hemispherical craters** for Deimos from Viking observations -- the paper that
introduced this very model. That is an RMS slope of 46.56 degrees, and applying
it takes the simulation from 34 % low to 3 % high, a residual of -1.9 K.

So the roughness that reconciles a 2026 TIRI observation with this
thermophysical model is the roughness the same authors measured for the same
body in 1990, through the same crater model. Nothing was tuned to reach it.

## What this is not, yet

- **One frame.** The other five wide-band frames are dilution-dominated and
  cannot test it.
- **Sensitive to the emission bound.** At 85 degrees instead of 75, even a
  20 degree RMS slope overshoots; Deimos's disc average moves by a factor of
  three between the two. That bound is not a free parameter to be chosen for
  agreement, and until the sensitivity is understood the 46.6 degrees is a
  consistency check rather than a retrieval.
- **The spin-up is ~1 K short** of converged at the ephemeris limit
  (`tpm_deimos.py`), which is inside the residual.

## The bound, and why it is not a detail

The correction is a **flux** ratio. `R * flat` converges as emission
approaches grazing, exactly, because the vanishing `cos(e)` cancels -- that was
measured. A rendered pixel carries **radiance**, where it does not cancel,
since the projected area is already accounted for by which pixels a facet
covers. Applied per pixel with no bound the disc average went from 14 to
63 W m^-2 sr^-1, three times the observation.

The model assumes a projected area of `cos(e) A`, while a rough facet at
grazing presents walls whose projected area does not vanish, so it is simply
not valid there. This is why the ROB sphere maps in `roughness-kuehrt/pres4`
thresholded emission at 75 and 85 degrees.

## Still to compare against

The bibliography carries three independent checks this has not yet been put
through, all of which bear directly on it:

- **Davidsson and Rickman (2014)** and **Davidsson et al. (2015)** -- the
  reference treatment of 1D conduction with a 3D analytical roughness
  correction, and thermal emission from rough surfaces. Partial comparison
  code exists in `examples/old/roughness_davidsson`.
- **Rozitis et al. (2024)**, pre-impact Didymos TPM: roughness in the **upper
  boundary condition** rather than as a geometric correction -- section 8.3's
  route, not 8.2's -- with a figure 2(a) to compare against. Partial code in
  `examples/old/didymos_comparison_rozitis2024`.
- **Mueller's model radiances** for this same swing-by, as CSV in the old
  `hera_mars_swingby` work. That is a like-for-like comparison on the exact
  observation, and the most direct of the three.

**Hamm et al. (2022)** applied this same crater routine to Ryugu MARA data,
with Kuehrt as a co-author, and is the closest precedent for using it this way.
