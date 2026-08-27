# Conduction solvers: variable spacing, implicit stepping, and what the analytical tests found

Written while preparing a Didymos thermophysical run that has to resolve both
the diurnal wave (2.26 h) and the seasonal one (700 d). That combination is
what forced the work: it cannot be done on a uniform grid at a reasonable
cost, and reaching for the non-uniform machinery already in kalast turned out
to give **12 K errors** because the grid builder and the solver had never been
used together.

**Headline: `nonuniform.column()` and `core::conduction_1d` did not compose.**
Adding a variable-spacing stencil brings a 16-node geometric grid to 0.48 K,
against 0.32 K for an 81-node uniform grid — 5x fewer nodes at comparable
accuracy. A working implicit solver adds a further 9x in timestep.

![sinusoidal validation](sinusoidal.png)

---

## 1. Why a uniform grid does not work here

Didymos with Γ=320 J m⁻² K⁻¹ s⁻¹ᐟ²: `k = 0.0632 W/m/K`, `D = 3.90e-8 m²/s`.

| wave | period | `ls1` | `ls2pi` |
|---|---|---|---|
| diurnal | 2.26 h | **1.01 cm** | 6.32 cm |
| seasonal | 700 d | 86.7 cm | **5.45 m** |

The column must reach several metres for the seasonal wave while resolving a
1 cm diurnal wave at the surface. Uniformly, at 10 nodes per diurnal skin
depth, that is ~5,400 nodes — and the explicit stability limit is set by the
*thinnest* layer, so `dt <= 13 s`, about 600 steps per rotation where ~100
would resolve the diurnal cycle. Over two solar orbits (14,867 rotations)
that is millions of unnecessary steps.

A geometric grid solves the depth problem: **41 nodes reach 8.9 m** with a
1 mm first layer.

## 2. The trap

`kalast.tpm.nonuniform.column()` builds exactly that grid.
`kalast.tpm.core.conduction_1d` implements

```rust
t_mid + d * dtpdx2 * (t[i-1] - 2 t[i] + t[i+1])
```

which is the **equal-spacing** second difference, second-order only when every
layer has the same thickness. On a geometric grid it is inconsistent — and it
fails silently, since nothing checks that the grid it is handed is uniform.

Both pieces shipped; nothing in the repository used them together, so nothing
had ever exercised the combination.

## 3. Validation against the analytical damped wave

`examples/analytical/sinusoidal.py`. A half-space forced sinusoidally at the
surface has the closed-form solution

```
T(z,t) = Tm + Ta exp(-z/ls) sin(z/ls - 2 pi t/P)
```

Run with Didymos's real properties and spin period, Dirichlet-forced at the
surface so the test isolates the conduction scheme from the radiative
boundary. Domain 8 `ls1` deep, four periods, error taken over six snapshots
of the final period.

| case | nodes | dt | max err | mean err |
|---|---|---|---|---|
| uniform grid, 10 nodes/`ls` | 81 | 5.18 s | **0.315 K** | 0.018 K |
| uniform grid, 4 nodes/`ls` | 33 | 32.37 s | 2.455 K | 0.085 K |
| geometric grid, **uniform stencil** | 16 | 7.46 s | **12.092 K** | 3.324 K |
| geometric grid, **variable stencil** | 16 | 8.95 s | **0.478 K** | 0.123 K |
| geometric grid, **implicit**, dt = spin/100 | 16 | 81.36 s | 2.170 K | 0.526 K |
| geometric grid, implicit at the explicit dt | 16 | 8.95 s | 0.638 K | 0.157 K |

Reading the table:

- Row 3 is the trap: **25x worse** than row 4 on the identical grid. The only
  difference is the stencil.
- Row 4 is the fix: **0.48 K in 16 nodes**, against 0.32 K in 81 uniform
  nodes. Comparable accuracy at a fifth of the nodes — which is what makes a
  seasonal column affordable.
- Row 6 is the consistency check that matters for trusting the implicit
  solver: at the *same* timestep it lands at 0.64 K against the explicit
  0.48 K. Backward Euler is first-order in time where the explicit scheme is
  effectively second-order at this step size, so slightly worse is exactly
  right. Agreement here means the two schemes are solving the same equation.
- Row 5 is the trade the implicit solver actually buys: **9.1x the timestep**
  for 2.17 K instead of 0.48 K.

## 4. What was added

### `core::conduction_1d_nonuniform` (Rust)

```
d2T/dz2 ~ 2/(h- + h+) [ (T+ - T)/h+ - (T - T-)/h- ]
```

with the two coefficients precomputed per interior node, mirroring how
`conduction_1d` takes `dt/dx²`:

```
coef_lo = 2 dt / (h- (h- + h+))        coef_hi = 2 dt / (h+ (h- + h+))
```

For equal spacing both collapse to `dt/h²` and it reduces *exactly* to
`conduction_1d`, so the uniform path is unchanged.

### `kalast/tpm/routine.py` — was empty

Holds what turns a grid into solver inputs, so these are not re-derived (and
mis-derived) per script: `uniform_coefficients`, `nonuniform_coefficients`,
`nonuniform_max_dt` (stability is `h- h+ / D`, set by the tightest node), and
`resolution_report` / `print_resolution_report` giving nodes per skin depth
and depth in skin depths.

### `kalast/tpm/implicit.py` — was not runnable

The module held a partial port of multiheats that **could never have
executed**: `flux_bc_implicit` and `bc_up_implicit` were module-level
functions taking `self` and dereferencing `self.temp` / `self.cond` /
`self.dx`, which do not exist; `flux_bc_implicit` called `bc_up_implicit` with
two arguments against a seven-argument signature; and no routine solved the
system at all. Nothing in the repository called any of it.

Replaced with a working backward-Euler solver on a variable-spacing grid:
`banded_matrix(z, D, dt)` assembles the tridiagonal system once (constant
while grid, diffusivity and `dt` hold), `step_dirichlet(ab, T, T_surface)`
takes one step via `scipy.linalg.solve_banded`. Unconditionally stable, so
`dt` follows accuracy rather than the thinnest layer.

**Not yet implemented: the radiative surface boundary.** It is non-linear in
`T` (absorbed flux against `sigma e T⁴` plus conduction) and needs a Newton
iteration wrapped around the solve. Until that exists the implicit path is
usable only with a prescribed surface temperature — which is what the
validation above uses, and is *not* what a thermophysical run needs. Use the
explicit path for radiative runs.

## 5. Examples brought out of `old/`

Both were written against an API that has since moved — `diffusivity` and
`skin_depth_1` migrated from `tpm.core` to `tpm.properties` — and both split
one calculation across several modules imported by bare name, so they only
ran from inside their own directory.

- `examples/old/sinusoidal/{setup,tpm,main}.py` -> **`examples/analytical/sinusoidal.py`**,
  one self-contained script, now covering five solver/grid combinations
  rather than one.
- `examples/old/analytical/{setup,dirichlet,neumann}.py` ->
  **`examples/analytical/slab_relaxation.py`**, both boundary conditions in
  one script.

### Slab relaxation

![slab relaxation](slab_relaxation.png)

Complements the periodic test by exercising the *transient* response and the
zero-flux boundary a TPM column uses at depth. Fourier-series solutions for a
finite slab, `L=0.1 m`, `D=5.44e-8 m²/s`, 100 nodes, `dt=3.75 s`:

| boundary | max abs error per snapshot (5 min, 1 h, 4 h, 10 h, 40 h) |
|---|---|
| Dirichlet (`T=0` both faces) | 1.315, 0.076, 0.052, 0.017, 0.000 K |
| Neumann (zero flux both faces) | 0.158, 0.149, 0.136, 0.074, 0.070 K |

The Dirichlet case starts at 1.3 K because the initial condition is
discontinuous at the faces — a step the truncated series resolves with
Gibbs ringing while the grid resolves it by diffusing — and converges to zero
as the sharp corner decays. The Neumann case holds ~0.1 K throughout, which
is series truncation at 100 modes rather than solver error; it conserves heat
and relaxes to the mean of the initial profile, as it should.

## 6. Consequences for the Didymos run

The intended progression, in order:

1. **Analytical** — done, this note.
2. **Explicit, coarse uniform grid** — the reference implementation, and worth
   timing even though it is slow: the runtime is a number worth having in a
   paper, and it is the baseline the other two are judged against.
3. **Explicit, non-uniform grid** — now correct and validated. Fewer nodes for
   the same accuracy, but the timestep is still capped by the thinnest layer.
4. **Implicit** — the timestep gain, once the radiative surface boundary is
   written.

### Measured, on the real problem

`examples/hera_didymos/tpm.py`, 10,000 facets, two solar orbits
(2023-03-23 -> 2027-01-21, 14,867 rotations), 4 nodes per diurnal skin depth,
column reaching one seasonal `ls2pi` (5.45 m). Benchmarked over 200 steps and
extrapolated:

| grid | nodes | dt | ms/step | steps | total |
|---|---|---|---|---|---|
| uniform | 2,168 | 32.4 s | 66.1 | 3,736,551 | **68.6 h (2.9 d)** |
| geometric | 34 | 55.9 s | 46.3 | 2,162,356 | **27.8 h (1.2 d)** |

The geometric grid is 2.5x faster end to end — fewer nodes *and* a larger
stable timestep, since the stability limit is `h- h+ / D` and the geometric
grid's second layer is already thicker than the uniform spacing.

**But look at the per-step column: 64x fewer nodes bought only 1.4x.** The
cost is not the conduction arithmetic, it is the per-facet Python call
overhead — roughly 4.6 us x 10,000 facets = 46 ms, with the node work nearly
free on top. The TPM loops over facets in Python, calling into Rust once per
facet per step.

So the solver choice is second-order here. Vectorising that loop — stepping
all facets as one array operation, or moving the loop into Rust — is worth
more than everything measured above: the geometric case moves ~340,000 values
per step, which numpy would handle in a millisecond or two rather than 46.
Until that is done, the 28-hour figure stands regardless of which conduction
scheme is used, and the implicit solver's timestep advantage would be largely
wasted.

### Coverage trap, worth recording

The first attempt failed with `SPKINSUFFDATA` at 2023-03-23. `hera_plan_local.tm`
loads only the Hera proximity-phase Didymos ephemeris
(`didymos_flp_000007_260701_270701_v01.bsp`, 2026-07-01 -> 2027-07-01), which
cannot reach a two-orbit spin-up. `didymos_hor_000101_500101_v01.bsp`
(Horizons, 1999-2050) is in the same directory but not in the meta-kernel;
`tpm.py` furnishes it explicitly, after the meta-kernel so it takes precedence
for Didymos throughout and the spin-up does not cross an ephemeris boundary.
