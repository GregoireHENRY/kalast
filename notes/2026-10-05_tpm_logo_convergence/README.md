# 2026-10-05 — `tpm_logo.py`'s convergence; float32 freezes a graded column's depths

Asked whether the logo's run converged. It had not, and the fix asked for --
start warmer and run longer -- turned up something else: below about 5 m a
graded column cannot settle in float32.

## The criterion

With an insulated bottom and one conductivity, no heat crosses a settled
column on average over a period, so its mean temperature is the same at every
depth: the bottom sits at the surface's mean. `tpm.py`'s 202-spin run: 279.1 K
at both ends of the equator's column, 134.2 K at the north pole's.

## The logo at 3.31 years from 0 K

Changes at the same point of the orbit, an orbit apart:

| | surface mean, last orbit | bottom (11.37 m) | bottom over the last orbit | surface over the last orbit |
|---|---|---|---|---|
| equator | 223 K | 95 K | +34 K | +0.6 K |
| north pole | 159 K | 67 K | +24 K | +0.5 K |
| south pole | 124 K | 47 K | +19 K | +3.1 K |

The column is 2 pi yearly skin depths deep, 11.4 m at a diffusivity of
1.57e-7 m2/s, and its slowest mode, 4 L^2 / (pi^2 kappa), decays in 5.0
orbits: held at its mean from 0 K, the bottom is 66 % short of it at 3.31
orbits (58 % in the run), 2.2 % at 20, 0.3 % at 30.

`tpm_plot.py`'s "moved ... at most over the last 10 samples" is, for the
logo's sample every 10 spins, 100 spins apart: the season moving, not the run
settling. Left as it is; the orbit-to-orbit measure was offered.

## The change

Each column starts at `core.effective_temperature(1.0, <cos i / r^2>, ...)`,
the mean over 100 points of the orbit and 24 turns of the spin through
`facet_incidence` (0.05 s): the temperature that radiates the orbit's mean
absorbed sunlight. At convergence e sigma <T^4> is that, and <T> <= <T^4>^(1/4),
so it is an upper bound of the yearly mean: 233.7 K at the equator, 176.2 K at
both poles -- an orbit's mean sunlight is the same at plus and minus a
latitude whenever its summer falls, Kepler's second law. The run settled near
225, 165 and 136 K: 8, 11 and 40 K below the start. The other estimate, each
day's own temperature averaged over the orbit, is a lower bound, polar nights
counted at 0 K: 225.9, 139.4 and 71.3 K.

And `years = 30.31`, the view's 0.31 kept. 24,750 samples, 47 MB in
`out/sphere/`; 1457 s in a background window here.

## The result: the surface settled, the depths frozen

The surface at the logo's point of the orbit against an orbit before: -0.85 K
at most after one orbit (the south pole), -0.42 after 5, -0.10 after 10, then
within 0.09 K either way. But the column's yearly mean, which should be the
same at every depth, rises below 1 m: at the equator 225.3 K at the surface,
227.7 at 1.8 m, 230.2 at 4.6 m and 233.7 at 11.4 m -- the start, untouched in
30 orbits; at the south pole 136.0 up to 143.8 K. None of it moved after about
10 orbits.

float32. `Float` is f32 by default (`src/lib.rs`), the temperatures too. A
graded layer `w` thick steps by about `kappa dt / w^2` times its second
difference, and float32's spacing at 233 K is 1.5e-5: an increment under half
of it rounds away. The deepest layer that conducts, 1.73 m thick (the last,
2.08 m, copies it), steps by 4.6e-6 a kelvin of difference with the one above
at the logo's 80.6 s, so it moves only for a difference over about 1.6 K, the
layers above it for less. The column settles into a staircase and stops: at
the equator 0.625 K between the two deepest, a step of 2.9e-6 K that never
lands.

Shown on the equator's column as the run left it, its surface held, 200,000
steps (0.24 orbits) of kalast's `heat_conduction` against the same
conservative scheme in float64:

| depth (m) | start | kalast, f32 | float64 |
|---|---|---|---|
| 4.56 | 233.616 | 231.630 | 231.910 |
| 5.48 | 232.568 | 232.349 | 232.779 |
| 6.57 | 232.376 | 232.552 | 233.069 |
| 7.89 | 233.097 | 233.097 | 233.196 |
| 9.47 | 233.722 | 233.722 | 233.370 |
| 11.37 | 233.722 | 233.722 | 233.370 |

Graded columns only: on equal layers at the stable step `r` is near 0.5, and
the smallest the uniform examples reach, `tpm_variable_2.py`'s dust at 0.055,
still resolves a few 1e-4 K. The surface feels it less: 8 K over 10 m carries
0.16 W/m2, about 0.1 K at the equator's surface -- estimated so here, and
measured once fixed (below) at 0.1 K there but 0.7 K a year at the south
pole's, where a cold surface radiates an excess away less readily. The deep
columns of a graded ground are wrong by a few kelvin, set by where they
started.

## Open

- `tpm_plot.py`'s convergence measure for a run with an orbit: one orbit back
  rather than 10 samples, offered.

## 6 October: the remainder carried

Of the three fixes offered, the user turned down a longer step for the thick
layers ("not a good solution wrt to spin 2.26h") and, between float64 and the
remainder, chose the remainder: the same memory as float64 -- a second float
a node -- but float32's SIMD width, nothing for `Properties` columns or the
surface's solve, the same scheme a GPU TPM would need (Metal has no float64),
and no change to scripts.

`src/tpm/core.rs`: a `Ground` holds a `Remainder` (a `RefCell`, so `&ground`
steps it), which `Ground::view` hands to `GroundView` as `remainder`; the
Python `Ground` holds its own and passes it in `heat_conduction`. In
`conduction_ground` each layer goes through `step_layer::<CARRY, MEASURE>`:
where carried, `step += rest; next = v + step; rest = step - (next - v)`
(Fast2Sum: `|v| >= |step|` for temperatures). A remainder is kept per
temperature array, keyed by the address of its values, four at most (one
`Ground` for several bodies), reset for a new array or shape.

Only the layers that need it carry: those whose smallest share of the
differences around a node, `r (k_up + k_down)`, is under 1/16 -- below a
graded column's first ten layers or so in the logo; none of a column of equal
layers. Measured on every step, that smallest share cost a tenth of the step
on equal layers (one more reduction a node); it is measured every 64 steps
instead, the first step carrying everywhere, so a conductivity or `dt`
changed mid-run is followed within 64 steps. The thin layers keep float32's
own rounding: within 1.6e-3 K of float64 on the logo's column.

The three checks asked for:

1. The 200,000 steps on the frozen equator column, against the same scheme in
   float64: 9.47 m now 233.722 -> 233.370 K, float64 233.370 (it did not move
   before); every layer below 1 m within 1e-5 K, the largest difference
   anywhere 1.6e-3 K, between 5 and 34 cm.
2. The logo's 30.31 years again (1478 s): the yearly mean below 10 cm the same
   at every depth, within 0.021 K at the equator (225.56-225.57 K), 0.016 K at
   the north pole (164.7 K), 0.095 K at the south pole (134.85-134.96 K, still
   settling from a start 41 K above); the frozen run's spreads were 7.9, 10.1
   and 8.2 K. The frozen depths had kept the surface warm: on the year's mean
   +0.11 K at the equator, +0.26 K at the north pole, +0.68 K at the south
   pole, +1.14 K at most; at the logo's moment +0.79 K at the south pole.
   The warm start was 8, 11 and 41 K above where the columns settle.
3. The conduction step, old and new alternated in one process (a temporary
   test, removed; the machine loaded by the user's kalast, so medians of 20
   rounds, three runs): the logo's column, 36 x 960, +5 to +16 %; at 5120
   facets +6 to +9 %; equal layers of a per-node `Ground`, 122 x 5120, -4 to
   -1 % -- nothing. Carried in every layer it was +31 %, +7 % and +13 %.

Tests: `a_graded_columns_thick_layers_settle_as_in_float64` (a ratio-2 column
a third of a kelvin warmer at the bottom, 40,000 steps: float64 moves it
0.04 K or more, the `Ground` follows within 1e-3 K, float32 alone not at
all), `a_ground_carries_each_arrays_remainder_apart` (one `Ground`, two
arrays, the same to the bit as a `Ground` each). Library 307 of 307; 21 of
21 in `tpm::` without default features.
