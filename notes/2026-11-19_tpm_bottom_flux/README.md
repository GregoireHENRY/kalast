# A heat flow at the bottom of the column (plan item E10)

19 November 2026. The user asked on 18 November for "an option to have bottom flux like for the
simulation of the Moon", and on 19 November that CPU and GPU be level before Paper I is submitted
("i want gpu and cpu before 1st submission yes. thats one of the selling point also"). So this item
is done on both at once.

## What it is

Paper I, equation 19: `k dT/dz = Q_b` at the base, depth counted downward. In its first-order form
the base node sits above the node over it by `Q_b d / k`, `d` the distance between the two and `k`
the conductivity between them. `Q_b = 0` is the adiabatic bottom that kalast had.

- **CPU**: `tpm::core::bottom_flux(t, q, thermal, dz)`, `kalast.tpm.core.bottom_flux` from Python.
  For a `Ground`, `k` is the harmonic mean of the last two layers, which is also right for a graded
  column (its conductivities are scaled by the layers' widths). `q = 0` gives `bottom_adiabatic` to
  the bit. A `q` that is not finite, or a ground that does not conduct, is refused, and an error
  changes nothing.
- **GPU**: `GpuTpm::set_bottom_flux(q, spacing)`, called once. The shader's conduction pass already
  recomputed the node above the base to copy it; it now adds `params.bottom_rise = q spacing / k`.
  The parameters' buffer is kept and can be written (it was created once and dropped).

## Tests

- Rust, `tpm::core::tests::bottom_flux_*` (2): `q = 0` is the adiabatic bottom; the steady profile
  `T = T0 + q z / k` stays where it is under 200 steps of conduction, for the Moon's 0.018 W/m2;
  a `Ground` of the same material gives the same base; the refusals.
- Python, `tests/test_tpm_bottom_flux.py` (4): the binding; and the GPU holding the same steady
  state through 200 full steps (surface balance and conduction) within 2e-3 K in float32, where
  the same column with the adiabatic base moves by more than 0.1 K.

## Not done here

- The lunar case of Hayne et al. (2017) needs k(T) and c(T) as well (E9). It is the verification
  case P1.25 of the paper's plan.
- No example uses it yet.

## State of the engine items

E1 done (18 Nov). E4: the solar constant done (19 Nov); Newton's tolerance and the surface gradient
on graded grids open. **E10 done, CPU and GPU.** Open: E9, E11, E2, E8, E6, E3 and E13 for the
items above, E12, E7.
