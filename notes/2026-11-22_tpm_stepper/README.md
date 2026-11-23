# A step by any scheme, and a ground that changes with temperature (plan items E11 and E9, CPU and GPU)

22 November 2026. The user asked on 18 November: "can rust core CPU tpm also implement implicit, semi
explicit / semi implicit, variable spacing, non uniform properties? same for GPU and no more python
only implementation, only wrapper"; and of k(T), cp(T): "should be in rust core impl". On 19 November:
"i want gpu and cpu before 1st submission yes". Both sides are done: the CPU first (commit 6c92452),
the GPU twin after it the same day.

## Progress

- [x] `Laws`: conductivity and heat capacity that change with temperature, in a `Ground`
- [x] the explicit functions read them (`flux_bc`, `heat_conduction`, `bottom_flux`)
- [x] `Stepper`: explicit, RKC, backward Euler, Crank-Nicolson, BDF2, in Rust
- [x] Python wrapper, stubs, 8 Python tests against the Python solvers it replaces
- [x] 15 Rust tests
- [x] the same on the GPU (`shaders/tpm_stepper.wgsl`, `src/tpm/gpu_stepper.rs`), and CPU against GPU on each scheme
- [ ] `kalast/tpm/implicit.py`, `explicit.py`, `nonuniform.py` turned into wrappers (item E12)

## What there is

`src/tpm/stepper.rs`, `kalast.tpm.core.Stepper` from Python.

```python
stepper = core.Stepper("crank-nicolson")           # or explicit, rkc, backward-euler, bdf2
stepper.step(t, flux, ground, dt, dz, bottom_flux=0.018)
```

One call is one step of every column: the surface from the flux it absorbs, the layers by
conduction, the base from the heat flow that enters it. `"explicit"` is `flux_bc`,
`heat_conduction` and `bottom_flux`, to the bit.

The columns are the ones `core` steps, a `Ground`'s included: properties per layer and per facet, a
graded column's widths carried by its properties. Nothing new is asked of a script but the scheme.

### The laws (E9)

`Ground.laws`, `Laws { radiative, heat_capacity }`; from Python `ground.radiative_conductivity` and
`ground.heat_capacity_polynomial`.

- `k(T) = k_c (1 + chi (T / 350)^3)` (Mitchell & de Pater 1994; Hayne et al. 2017, equation A4), `k_c`
  the ground's `conductivity`.
- `c(T) = a0 + a1 T + ... + a4 T^4` in place of the ground's `heat_capacity`.
- `Laws::MOON` holds the values of Hayne et al. (2017, table A1), read in the paper itself: `chi` 2.7,
  `c0..c4` = -3.6125, 2.7431, 2.3616e-3, -1.2340e-5, 8.9093e-9.
- Each step reads them at the temperatures of the step before; RKC at each stage's own.
- The conductivity between two layers is the harmonic mean of the two, each at its temperature. The
  surface balance takes the surface's, with its slope in Newton's method.
- `stability_maxdt_between(dz, s, t_min, t_max)` bounds the explicit step over a range of
  temperatures (the largest conductivity, the smallest heat capacity).
- Without laws every function takes the code path it took: the 26 tests of `core` pass unchanged,
  the bit-for-bit ones included.

### The implicit schemes (E11)

A column is a tridiagonal system. The surface balance is not linear, and the layers under it are
unknowns of the same step. The system is eliminated from the base up, which leaves each layer's
change as a line of the change of the layer above, `d_i = P_i d_{i-1} + S_i`. The surface's gradient
`-3 T0 + 4 T1 - T2` is then a line of `T0` alone, and the balance one equation in one unknown, solved
by Newton's method. The layers follow from the top down. One elimination a step, surface and layers
at the same time level.

- Backward Euler, Crank-Nicolson, BDF2. BDF2 takes steps of unequal length (the variable-step
  formula) and starts with one backward-Euler step.
- `newton_iterations=1` is the balance linearised about the step before: the semi-implicit form of
  Schorghofer & Khatiwala (2024). Over a day at 400 steps it is 0.002 K from the balance solved in
  full.
- Every pass goes along a layer across the facets, as `core`'s explicit step does.

### RKC

The first-order damped Runge-Kutta-Chebyshev recursion that `kalast/tpm/explicit.py` has, in Rust.
Its reach is `2 w0 / w1` exactly (the Python module finds the same number by scanning the
polynomial). The step is refused if its stages do not reach, with the stages it takes.

## The GPU twin

`src/tpm/gpu_stepper.rs`, `shaders/tpm_stepper.wgsl`, `kalast._rs.tpm.gpu.GpuStepper` from Python.

```python
gpu = GpuStepper(ground, layers, facets, dz, scheme="bdf2")
gpu.set_bottom_flux(0.018)
gpu.upload(t)
gpu.step(flux, dt)          # or step_sun(sun_pos, au, dt), step_surface(temperature, dt)
t = gpu.download()
```

- The same arithmetic as the CPU, one thread a facet, each going up and down its own column: the
  elimination, the surface, the layers; or the stages of RKC, one dispatch each. The explicit step
  is the stage pass once, in `core`'s order (surface, layers, base).
- It takes what the CPU takes: one material or a `Ground` -- a property one a layer for every
  facet is sent as one column, one a layer and a facet in full --, its laws, each facet's emissivity
  and albedo, the heat flow at the base. This is also item E13's "the GPU model takes layered
  ground".
- A step's parameters are a block a dispatch, written at once and bound by offset, so a step of
  any number of stages is one submission.
- The three working arrays are handed round from a stage to the next (three bind groups). The
  implicit schemes use them for `P`, `S` and BDF2's step before.
- Eight storage buffers in the stepping pass, the number a device has to allow; the sunlight is a
  shader of its own (`tpm_stepper_sun.wgsl`) for that reason.
- The compensated sum holds on the GPU: wgpu compiles without fast math. After 6,400 steps of a
  wave the GPU is within 0.0005 K of the CPU, and its base at the mean.
- The GPU cannot refuse a step. A step past what the explicit scheme or RKC reaches is refused on
  the host from the ground as given; with laws the limit is the script's to keep.
- `GpuTpm` (the explicit scheme on one material, by coefficients) is left as it was: the Hera
  scripts use it.

Measured, GPU against CPU (`KALAST_MEASURE=1 cargo test --lib tpm::gpu_stepper -- --nocapture`):

| | explicit | RKC (3) | backward Euler | Crank-Nicolson | BDF2 |
|---|---|---|---|---|---|
| one material, 400 steps of a day (K) | 0.00006 | 0.00009 | 0.00008 | 0.00009 | 0.00008 |
| graded ground, each facet its own, the Moon's laws, 0.018 W/m2 at the base (K) | 0.0028 | 0.0027 | 0.0015 | 0.0015 | 0.0033 |
| a surface given, 200 steps (K) | 0 | 0.00011 | 0.00003 | 0.00006 | 0.00003 |

The sunlight computed on the GPU against the same flux given to the CPU: 0.00006 K.

Tests: 6 in Rust (`tpm::gpu_stepper::tests`), 5 in Python (`tests/test_tpm_gpu_stepper.py`). They
say so and check nothing where no GPU answers.

## Two things found on the way

1. **Single precision drifted.** The first version solved for the temperatures. At 1,600 steps a
   period the base of a column was 0.2 K off after four periods, and growing with the number of
   steps: each temperature is rounded to 3e-5 K at every step. The schemes now solve for the
   step's *changes*, which are small numbers held to their own seven digits, and add them with a
   compensated sum (the `Stepper` keeps what the sum cannot hold, as `core`'s `Remainder` does for
   the explicit step). After that Crank-Nicolson and BDF2 sit on the layers' own error at every
   step length, and the orders below are measured in float32.
2. **The Python BDF2 starts with a short step.** `kalast/tpm/implicit.py` takes its first step
   with the BDF2 matrix on the temperatures alone, which is a backward-Euler step of 2/3 `dt`
   labelled `dt`: a first-order error at the first step, 8 K on a column far from balance. The Rust
   solver takes a full backward-Euler step. The comparison test gives the Python solver a proper
   first step. To settle when the Python modules become wrappers (E12).

Also: `tests/test_conduction.py`'s 0.400 K for the explicit scheme is the surface node itself, one
step behind the analytical surface (`omega dt Ta` is 0.40 K), not the conduction. With the surface
left out the explicit scheme is within 0.06 K. To keep in mind for the paper's verification section.

## Measured

`KALAST_MEASURE=1 cargo test --lib tpm::stepper -- --nocapture --test-threads=1` prints these.

The wave of a surface swinging by 100 K, four periods, the largest error of a layer over the last:

| | steps a period | × explicit limit | error (K) |
|---|---|---|---|
| equal layers, 10 a skin depth: backward Euler | 200 | 3.1 | 0.450 |
| Crank-Nicolson | 200 | 3.1 | 0.0437 |
| BDF2 | 200 | 3.1 | 0.0449 |
| explicit | 800 | 0.8 | 0.0594 |
| RKC, 2 stages | 400 | 1.6 | 0.239 |
| graded, 18 layers: backward Euler | 200 | | 0.378 |
| Crank-Nicolson | 200 | | 0.152 |
| BDF2 | 200 | | 0.155 |

0.0437 K and 0.152 K are what the layers allow: the second-order schemes stay there at any step.

Orders, against the same columns at 3,200 steps a period (errors at 25, 50, 100 steps):

| | errors (K) | order |
|---|---|---|
| backward Euler | 3.13, 1.59, 0.80 | 0.98, 0.99 |
| Crank-Nicolson | 0.092, 0.023, 0.0058 | 2.00, 1.99 |
| BDF2 | 0.42, 0.10, 0.025 | 2.04, 2.02 |

A day of sunlight (a 76 K swing), against the explicit scheme at 3,200 steps a day: backward Euler
0.41 K, Crank-Nicolson 0.042 K, BDF2 0.036 K, RKC 1.17 K, each at 200 steps.

An eclipse that cuts the light at once, 400 steps a day: backward Euler 4.6 K, Crank-Nicolson
2.6 K, BDF2 3.3 K from the reference, all at the instant of the cut, and 1.4, 0.75 and 0.98 K at
1,600 steps. None rings: through the eclipse the surface only cools.

A heat flow through `k(T)`, which has a closed form (`k_c (T + chi T^4 / (4 350^3))` grows by
`q z`): 0.0108 K with 41 layers, 0.0027 K with 81, a ratio of 4.

A column that radiates nothing, with the Moon's `c(T)`, heated from 200 K to 295 K at the surface:
it gains the heat it absorbs within 0.03 % by every scheme.

Against the Python solvers in float64, layer by layer over a day (`tests/test_tpm_stepper.py`):
within 0.002 K for backward Euler, Crank-Nicolson, BDF2 and RKC.

## Tests

- Rust, `tpm::stepper::tests` (15): the explicit scheme is `core`'s three calls; every scheme
  follows the wave; the orders; a constant flux settles; a day; an eclipse; one Newton iteration;
  the heat flow through `k(T)`; the heat gained with `c(T)`; laws that change nothing; a view with
  gaps; BDF2 on unequal steps; RKC's reach; the refusals; a heat flow at the base.
- Python, `tests/test_tpm_stepper.py` (8): the binding, the comparisons with `implicit.py` and
  `explicit.py`, the laws set from Python, the refusals.
- `core`'s 26 pass unchanged. `tests/test_stubs.py` fails as before on the shape classes and
  `TelescopeConfig`, none of them touched here.

## Not done

- Per-facet lit fractions and view factors reaching the GPU model by themselves (item E3): the
  GPU takes a flux or `set_lit`, as before.
- A benchmark of the GPU's schemes against the CPU's: not measured yet.
- `Properties` has no laws: a ground that changes with temperature is a `Ground` (one of one
  material costs a value an array).
- The lunar case of Hayne et al. (2017), which E9, E10 and E11 now allow, is a verification run of
  the paper (plan P1.25), not an engine item.
- The crate does not build in float64 (`use_f64` fails in the app), so nothing here was checked
  that way.

## State of the engine items

E1 done. E4: the solar constant done; the surface gradient on graded grids needs nothing in the
`Ground` form (its three top layers are even), and Newton's 0.1 K is the last correction, not the
error, which is under 1e-4 K; both to say in the paper. E10 done, CPU and GPU. **E9 and E11 done, CPU and
GPU**, and with them E13's layered ground on the GPU. Open: E2, E8 (the exchange), E6 (roughness in
the boundary condition), E3 (lit fractions and the exchange reaching the GPU model), E12 (the Python
modules as wrappers), E5 with each, E7 (Paper II).
