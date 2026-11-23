# 2026-11-18 — `core.flux_bc`: the surface balance from an absorbed flux

The first item of the engine work that the two papers need (E1 of the plan in
`~/projects/paper-tpm/notes/2026-11-17_plan.md`, section 3.0).

## Why

`solar_bc(t, dau, cosi, prop, dz)` takes a cosine of incidence per facet and computes the sunlight
itself. Whatever else a surface absorbs, the sunlight that other surfaces scatter to it and their
thermal radiation, had no way in: the one script that had self- and mutual heating
(`examples/hera_didymos/tpm_phase2.py`) and the roughness prototype passed it as an "equivalent
cosine", the flux divided by `S (1 - A) / r^2`. That works only while the albedo is not 1 and hides
what the boundary condition is.

## What

`kalast::tpm::core::flux_bc(t, flux, thermal, dz)`, and `kalast.tpm.core.flux_bc(t, flux, prop, dz)`
from Python: each surface temperature solved for

```text
Q  -  e sigma T0^4  +  k (-3 T0 + 4 T1 - T2) / (2 dz)  =  0
```

with `Q` the flux the facet absorbs (W/m2), one per facet. In the notation of Paper I,
`Q = (1 - A)(E + G_V) + e G_IR`: direct sunlight, scattered sunlight, and thermal radiation from
other surfaces, each already multiplied by what the surface absorbs of it.

- `solar_bc` and `flux_bc` share one routine (`surface_balance`); `solar_bc` is `flux_bc` with
  `Q = S (1 - A) max(cos i, 0) / r^2`, and its results have not changed by a bit.
- With a `Ground`, each facet's own emissivity and top-layer conductivity are read; the albedo is
  not, since the flux is already absorbed.
- A flux that is negative or not finite is refused, naming the facet.

## The GPU

`GpuTpm::step(flux)` already takes an absorbed flux per facet from the host, and `step_sun` computes
the sunlight in the shader. So both sides now accept an absorbed flux. What is still missing on the
GPU is the sunlight computed in the shader plus an extra flux in the same step, which comes with the
exchange on the GPU (E2, E3).

## Tests

- Rust, `tpm::core::tests`: `flux_bc_given_the_sunlight_is_solar_bc` (to the bit, for a body's
  properties and for a `Ground`), `flux_bc_balances_the_flux_it_is_given` (the residual of the
  balance is within what Newton's 0.1 K leaves; only the surfaces change),
  `flux_bc_refuses_a_flux_that_cannot_be`. The 21 tests that were there pass unchanged.
- Python, `tests/test_tpm_flux_bc.py`: the binding.

## Still to do for the papers (plan, section 3.0)

| | Item | State |
|---|---|---|
| E1 | the boundary condition takes an absorbed flux | done on the CPU; the GPU as described above |
| E2 | radiative exchange in Rust (view-factor product, multiple scattering between bodies, thermal reflections) | Python only (`kalast/tpm/heating.py`) |
| E3 | the GPU model level with the CPU's | open |
| E4 | Newton's tolerance, the solar constant (1369 in `util.rs`, 1361 in the paper), the surface gradient on graded grids | open; the constant waits for the user's answer |
| E5 | tests: GPU against CPU, several bounces, band radiance, roughness | open |
| E6 | roughness in the boundary condition, switchable | a private prototype |
| E7 | thermal emission in the renderer; the disc-integrated thermal flux | scripts |
| E8 | the exact view factor between two triangles | open |
| E9 | conductivity and heat capacity varying with temperature | unused Python helpers |
| E10 | a heat flux at the lower boundary | open |
| E11 | implicit and semi-implicit solvers, and RKC, in Rust, CPU and GPU | Python, tests only |
| E12 | the Python thermal modules as wrappers | open |
| E13 | lit fractions and view factors on the CPU without a GPU; layered ground on the GPU | open |

The user's rules for all of it (18 November): everything in Rust, Python only wraps; the CPU and the
GPU offer the same features and differ in speed.

## 19 November: the solar constant (part of E4)

The user agreed to 1361 W/m2 in the paper and in kalast ("agreed", 19 November). `SOLAR_CONSTANT` in `src/util.rs` and the roughness model's default in `src/tpm/roughness.rs` and its binding were 1369. The older value comes from radiometers that read high; the solar cycle moves the true value by about 0.1 %, which the model leaves out. The 24 tests of the thermal core pass unchanged: they use the constant by name. The rest of E4 (Newton's tolerance, the surface gradient on graded grids) is open.
