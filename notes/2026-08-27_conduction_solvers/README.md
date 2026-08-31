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

Replaced with `implicit.Solver(z, D, dt, scheme=...)`, offering three schemes
on the same variable-spacing discretisation:

| scheme | order | stability | note |
|---|---|---|---|
| `backward-euler` | 1 | L-stable | most forgiving; the safe fallback |
| `crank-nicolson` | 2 | A-stable | most accurate per step; can ring (below) |
| `bdf2` | 2 | L-stable | production default; two-step, self-bootstrapping |

BDF2 is the recommendation because it is the only one that is second-order
*and* L-stable — accuracy without needing an argument about which boundary
condition is in use. It needs two levels of history, so it cannot start
itself; the solver bootstraps with one backward-Euler step, which costs one
order locally and nothing asymptotically. `reset()` discards the history,
which is required after restarting from a saved state.

#### The radiative surface boundary — now implemented

This was the gap. The thermophysical boundary

    F − σε T₀⁴ + k(−3T₀ + 4T₁ − T₂)/(2dz) = 0

is non-linear in `T₀`, and unlike the explicit path it cannot be applied
after the solve: `T₁` and `T₂` in that expression belong to the *new*
profile, which depends on `T₀` through the solve. The naive fix — lag the
coefficients, solve, then correct the surface — is only first-order accurate
in the coupling and gives back much of what the implicit scheme was for.

The structure that avoids it: only the surface row is non-linear, and only
the surface node couples into the interior system. So eliminate it. The
interior obeys

    interior = U + T₀ · V

where `U` solves the interior system with the surface clamped to zero and `V`
is its response to a unit surface temperature. `V` depends only on the grid,
so it is computed **once** in the constructor. `U` is one banded solve per
step, batched across every facet at once — `scipy.linalg.solve_banded` takes
a matrix of right-hand sides, so 10,000 columns cost one call. The surface
then reduces to a **scalar** Newton iteration per facet,

    R(T₀) = F + H − σε T₀⁴ + G·T₀,     G from V (constant), H from U (per facet)

which is a handful of vectorised array operations. This is exact rather than
lagged: surface and interior are converged together at the new time level.
Without it the alternative would have been 10,000 separate banded solves per
step, one per facet, since a full Newton makes the matrix facet-dependent.

`step_radiative` reproduces the explicit path to 0.10–0.21 K at a timestep
both can take, which is the consistency check that the coupling is right.

#### Crank-Nicolson ringing: measured, not assumed

The textbook warning is that Crank-Nicolson is A-stable but not L-stable —
its amplification factor tends to −1 rather than 0 for stiff modes, so a mode
with `D·dt/h² ≫ 1` is sign-flipped each step instead of damped. On this grid
the first layer is 1.2 mm, so that ratio is already 14.5 at `dt = P/15`.

Stepping the *prescribed* surface temperature 300 K → 100 K, first interior
node, first eight steps at that dt:

```
backward-euler    146.3  126.1  119.7  116.4  114.4  112.9  111.8  111.0   reversals: 0
crank-nicolson    162.2  111.0  126.7  110.0  118.3  109.2  114.3  108.6   reversals: 10
bdf2              155.2  124.1  118.0  115.5  113.8  112.5  111.6  110.8   reversals: 0
```

Unambiguous. But stepping the *flux* instead — an eclipse ingress, the case
one would actually worry about in this model — **none of the three rings**,
at any dt tried out to 61× the explicit limit. The reason is structural: with
the radiative boundary the surface node is algebraic, not time-integrated. It
is re-solved from the flux balance every step, and that re-anchoring damps
the mode the Dirichlet test excites.

So the warning is real but narrower than it reads: it applies to
Dirichlet-forced runs, and Crank-Nicolson is safe for radiative ones. Worth
having measured rather than inherited — the assumption would have ruled out
the most accurate scheme for no reason.

### `kalast/tpm/explicit.py` — new, and the honest result

The other half of the same question: keeping the scheme explicit — no linear
algebra, trivially vectorised — how much larger can the timestep be made?

Three things that do **not** help, recorded so they are not retried:

- **Higher-order explicit RK.** What limits the timestep for diffusion is how
  far down the negative real axis the stability region reaches, not order.
  Forward Euler reaches |z| = 2; classical RK4 reaches 2.79, for four times
  the work. A net loss.
- **Higher-order spatial stencils.** Better resolution per node, but a wider
  eigenvalue spread, which tightens the same limit.
- **DuFort-Frankel.** Explicit and unconditionally stable, which sounds like
  the answer. It is not: its stability is bought with *consistency*, not
  accuracy — the truncation error carries a term in `(dt/h)²`, so it
  converges to the heat equation only as `dt/h → 0`. On a grid whose first
  layer is a millimetre, a timestep large enough to be worth having makes
  that term large and the scheme quietly solves a different equation. Not
  implemented, deliberately.

What does help is **super-time-stepping**: `s` substeps of deliberately
unequal length — some individually unstable — whose composite amplification
polynomial is a shifted Chebyshev polynomial, stable over a stretch of the
negative real axis growing like `s²`. Implemented as first-order RKC in
Verwer's damped form; the stability boundary is computed numerically from the
recursion rather than quoted, so it stays correct if the damping changes:

```
 s     beta   dt vs forward Euler   net speedup after s stages
 1      2.0            1.0x               1.00x
 3     16.5            8.3x               2.76x
10    181.9           91.0x               9.10x
20    727.1          363.6x              18.18x
```

**And it loses.** Measured at 2,000 facets over 4 spins, against a
time-converged reference on the same grid:

| scheme | dt [s] | steps | stages | wall [s] | max err [K] |
|---|---|---|---|---|---|
| explicit forward Euler | 9.0 | 3636 | 1 | 0.47 | 0.588 |
| explicit RKC | 89.7 | 363 | 3 | 0.15 | 4.489 |
| implicit backward Euler | 81.4 | 400 | – | 0.08 | 0.805 |
| implicit Crank-Nicolson | 81.4 | 400 | – | 0.13 | **0.006** |
| implicit BDF2 | 81.4 | 400 | – | 0.10 | 0.023 |
| **implicit BDF2, coarse** | **325.4** | **100** | – | **0.02** | **0.393** |

RKC is 3× faster than forward Euler and stable out to 200× its limit, so the
stability claim holds — but it is beaten outright by the implicit path, which
at the same timestep is both faster and two orders of magnitude more
accurate. A tridiagonal solve batched across facets simply costs less than
three explicit stages.

The last row is the headline for the whole section: **19× faster than the
explicit path and still more accurate**, because the timestep is no longer
tied to a 1.2 mm surface layer.

RKC is kept because it is the first choice in exactly the case this is
heading towards: any problem where the solve stops being cheap. Lateral
conduction or an FEM discretisation (§8.5) gives a matrix that is no longer
tridiagonal, and a GPU implementation has no banded solver at all — in both,
RKC keeps its `s²` timestep with nothing but array arithmetic.

#### Order of accuracy, and a harness bug worth recording

Verifying the schemes' orders initially gave 1.0 for all three, and identical
max errors. That was the *test*, not the schemes: snapshots were compared
against the analytical solution at the requested time, but a snapshot lands
up to `dt` past its target, and at an amplitude of 100 K over the spin period
that phase offset alone is ~6 K at `dt = P/100` — first order in dt and
identical for every scheme, so it swamped everything being measured.

Comparing at the time actually reached fixes it, but then Crank-Nicolson and
BDF2 sit on the grid's *spatial* error floor (~0.37 K) at every dt worth
using, so the measured order still means nothing. Isolating the temporal
error requires a reference on the **same grid** stepped to convergence:

```
steps per 4 periods         100          200          400          800
backward-euler         3.14e+00     1.60e+00[1.0] 8.05e-01[1.0] 4.04e-01[1.0]
crank-nicolson         9.20e-02     2.30e-02[2.0] 5.76e-03[2.0] 1.45e-03[2.0]
bdf2                   3.93e-01     9.50e-02[2.0] 2.31e-02[2.0] 5.53e-03[2.1]
```

1, 2, 2 as they should be. Two lessons: a convergence test that compares
against the wrong time measures the sampling, and one run on a grid too
coarse measures the grid.

### Why the production run still uses explicit forward Euler

This deserves spelling out, because the general advice — heard at
conferences, and correct as general advice — is that implicit schemes are
faster for stiff diffusion, and the benchmark in this very section measures
BDF2 at **19x** the explicit path. Yet `tpm.py` and `tpm_phase2.py` both run
explicit forward Euler. That is not an oversight, and the reconciliation is
worth understanding.

**The 19x is measured on a grid the production run does not use.** The
sinusoidal example resolves the diurnal wave with 10 nodes per skin depth,
giving a 1.21 mm first layer. Stability scales as `h²`, so that grid caps the
explicit timestep at 22.4 s. The production grid deliberately uses only **4**
nodes per skin depth, giving a 3.02 mm first layer — and stability at `h²`
means 2.5x thicker buys 6.2x the timestep:

| grid | first layer | explicit limit | accuracy needs (spin/100) | implicit headroom |
|---|---|---|---|---|
| sinusoidal example, 10 nodes/ls | 1.21 mm | 22.4 s | 81.4 s | **9.1x** |
| production, 4 nodes/ls | 3.02 mm | 139.8 s | 81.4 s | **1.45x** |

On the production grid the explicit stability limit (139.8 s) is *already
above* what accuracy requires (81.4 s). Stability has stopped being the
binding constraint — the forcing is. So an unconditionally stable scheme has
almost nothing left to buy: at best 1.45x, for the cost of a linear solve.

**A second constraint closes the remaining gap.** The eclipse window is 93
minutes and an individual facet's ingress is much shorter. At dt = 55.9 s the
run puts ~100 steps across that window; at the 325 s timestep that made BDF2
look 19x faster, it would put 17, smearing the very feature the deliverable
exists to show. For phase 2, accuracy against a *transient* sets dt, and no
scheme evades that.

**So the correct statement is conditional**, and both halves matter:

- Implicit wins decisively **when the grid is fine enough that stability, not
  accuracy, sets the timestep.** That is the regime the advice describes, and
  it is real — 19x here.
- The coarse-grid choice made earlier had already escaped that regime, by
  trading spatial resolution for a thicker surface layer.

**Where the implicit work pays off is therefore not speed but freedom.** The
coarse grid is not free: at 4 nodes per skin depth the sinusoidal validation
errs by 2.46 K against 0.40 K at 10 nodes. Explicitly, buying that accuracy
costs 6.2x the steps. With BDF2 it costs **nothing in time** — refine the
surface layer and keep dt at whatever the forcing demands. Implicit decouples
spatial refinement from temporal cost, which is exactly what the roughness
work (§8.3) needs, since sub-facet columns require a far finer near-surface
grid than the facet-scale model does.

The practical decision for now: keep explicit forward Euler for phase 1 and
phase 2, because on this grid it is within 1.45x of optimal and it is the
scheme the results were validated with. Switch to BDF2 when the grid refines
— and the switch is a one-line change, which is the point of having built it.

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
all facets as one array operation — is worth more than everything measured
above.

**Measured**, 10,000 facets on the 34-node geometric grid, Newton surface
solve plus conduction:

| | ms/step | 2,162,356 steps |
|---|---|---|
| per-facet Python loop | 20.04 | 12.0 h |
| vectorised numpy | **1.49** | **0.89 h** |

**13.5x**, and that is the conduction core alone — the per-facet insolation
terms vectorise trivially too, so the full step should fall from ~46 ms to
under 2 ms and the two-orbit run from ~28 h to about an hour. `routine.py`
gains `step_surface_newton` and `step_conduction` for this.

This also changes the case for the implicit solver: its advantage is a larger
timestep, which only pays once the per-step cost is not dominated by Python
overhead. Vectorise first, then re-evaluate.

**A bug the vectorisation exposed.** `math::cosine_incidence` clamps negative
cosines to zero, but `core::radiation_sun` did not. Every existing caller went
through the former, so the gap was invisible — until a vectorised inner loop
computes the dot product directly and passes it straight in. A night-side
facet would then receive *negative* insolation, which does not merely drop a
term: it actively drives the surface below its radiative balance, and the
error is largest exactly where the diurnal wave is coldest.

Fixed at source rather than in the caller: `radiation_sun` and
`radiation_sun_reflected` both clamp now, with tests. The reflected form
matters for §7.2 — unclamped, a shadowed facet would have *removed* energy
from whatever it illuminates, and that term is about to be built.

### Coverage trap, worth recording

The first attempt failed with `SPKINSUFFDATA` at 2023-03-23. `hera_plan_local.tm`
loads only the Hera proximity-phase Didymos ephemeris
(`didymos_flp_000007_260701_270701_v01.bsp`, 2026-07-01 -> 2027-07-01), which
cannot reach a two-orbit spin-up. `didymos_hor_000101_500101_v01.bsp`
(Horizons, 1999-2050) is in the same directory but not in the meta-kernel;
`tpm.py` furnishes it explicitly, after the meta-kernel so it takes precedence
for Didymos throughout and the spin-up does not cross an ephemeris boundary.

---

# 7. Required before the production run

The surface boundary condition in `examples/hera_didymos/tpm.py` is currently
**direct insolation only**:

```python
sflux = kalast.tpm.core.radiation_sun(d_sun / AU, cosi, prop.albedo)
```

`cosi < 0` gives night, but nothing else darkens or warms a facet. Three terms
are missing, and for a *binary* asteroid at a *mutual event epoch* they are
not small. The 2027-01-21 target epoch was chosen precisely because Dimorphos
transits — so the one thing that makes the scene interesting is the one thing
the model does not yet contain.

## 7.1 Eclipse shadowing, via the shadow map

Dimorphos casts a real shadow on Didymos (and vice versa). A facet in umbra
loses its entire solar term while its temperature keeps evolving, which is the
signature a thermal camera actually sees during a mutual event.

kalast already has the mechanism: `config.access_shadow_map = True` plus
`sim.facet_shadow(body)` returns the occluded fraction per facet, validated
to 0.013% on the absorbed-flux integral (`2026-08-26_facet_shadow_query/`).
The boundary condition becomes

```python
lit = 1.0 - frac[ii]          # 0 fully shadowed, 1 fully lit
sflux = radiation_sun(...) * lit
```

The quarter-step resolution (4 samples/facet) matters here: penumbral facets
at the shadow rim get 0.25/0.5/0.75 rather than a hard flip.

**Structural consequence.** The query reads the GPU shadow map, so the TPM has
to run *inside* the render loop rather than headless — `before_render` sets
the epoch and body transforms from spice, `after_render` reads the shadow
fractions and takes the conduction step. This is exactly what the two-callback
split was built for (`2026-08-26_facet_shadow_query/` §1), but it does mean
the two-orbit run drives 2.16M rendered frames.

Cheaper alternative worth measuring first: mutual events occupy a small
fraction of the orbit. Detect the eclipse windows geometrically (angular
separation of the Sun and the companion as seen from each body, as in
`2026-08-25_pcf_shadow_comparison/`) and only pay for shadow queries inside
them, using `lit = 1` elsewhere.

### Result — run, and measured

Implemented as `examples/hera_didymos/tpm_phase2.py`. It restarts from the
three-orbit spin-up on the identical 34-node grid, loads both the 10k Didymos
and Dimorphos meshes so Dimorphos acts as an occluder, and runs the last six
Didymos rotations to 2027-01-21T05:36 UTC inside the render loop
(`before_render` places bodies from spice, `after_render` reads
`sim.facet_shadow(0)` and steps the TPM).

The whole segment is 873 steps and costs 4.8–6.3 s wall, so the geometric
eclipse-window optimisation sketched above is not needed *at this segment
length*. It stays on the list, because it is what makes a segment spanning
several Dimorphos orbits affordable — see §7.6.

#### An ephemeris trap that invalidated the first run

The first version furnished `didymos_hor_000101_500101_v01.bsp` alongside the
meta-kernel, copying `tpm.py`. That is correct for the spin-up, which starts
in 2023 outside mission coverage. It is wrong here: the Horizons file
provides the **same body id** (`-658030`) as the mission's
`didymos_flp_000007_260701_270701_v01.bsp`, and SPICE serves the
last-loaded file for a given id. So furnishing it replaced the mission
solution, and the two disagree by **106 km** on the Didymos position.

Dimorphos was therefore placed 106 km away instead of 1.19 km, on the
anti-sunward side, where it casts no shadow at all. The run still reported 64
shadowed facets, because `facet_shadow` returns *any* occlusion — those were
Didymos shadowing itself in its own concavities. The result looked plausible
and measured the wrong thing entirely.

The spin-up does not care (106 km against 1.5e8 km changes no Sun direction),
so `tpm.py` is unaffected. This segment cares to the metre, and the
meta-kernel covers 2026-07 to 2027-07, so it now furnishes nothing else.

**Generalisable lesson**: a supplementary SPK that overlaps a mission kernel's
body id silently overrides it. Nothing errors; the geometry is just wrong.
Worth a coverage/consistency assertion whenever two ephemerides for one body
are loaded together.

#### The study epoch is a dead-centre eclipse

With the mission ephemeris, at 2027-01-21T05:36 Dimorphos sits **1.151 km
sunward** of Didymos with a perpendicular offset from the Didymos–Sun line of
**1 metre**. The epoch was evidently chosen for exactly this. From Hera at
25.8 km, Dimorphos is in front of Didymos but *off* its disk (4707 arcsec
separation against a 3113 arcsec disk radius), so the image shows the primary
carrying a shadow spot, with the secondary beside it rather than in transit
across it.

#### Three-way ablation

`SHADOW_MODE` selects `none` (direct insolation only, phase 1 physics),
`self` (Didymos alone, so only its own concavities shadow it) or `mutual`
(Dimorphos loaded too). Differencing separates the two shadowing terms:

| term | facets changed | worst ΔT | disk-mean ΔT | worst band-radiance drop |
|---|---|---|---|---|
| self-shadowing (`self` − `none`) | 4,105 | −12.4 K | −0.16 K | −9.4 % |
| eclipse (`mutual` − `self`) | 3,980 | **−95.9 K** | −1.34 K | **−77.7 %** |
| both (`mutual` − `none`) | 4,240 | −95.9 K | −1.49 K | −77.7 % |

Radiance is integrated over TIRI's 8–14 µm band. At the study epoch itself
257 facets are still in shadow, and 383 carry a band-radiance drop above 5 %.

The worst facet cools from 343.8 K to 247.9 K — a 96 K drop, which is what
`B ∝ T⁴`-ish behaviour in the thermal infrared turns into a **78 % radiance
deficit**. This is not a subtle correction: the eclipse is the dominant
feature of the simulated image, and it is dramatically larger than the
topographic self-shadowing that accompanies it.

For scale, a facet fully shadowed at 343 K radiates ~706 W/m² against a
surface heat capacity of ~1.6e4 J/m²/K over one skin depth, so ~0.04 K/s —
and the shadow takes 10–20 minutes to cross a given point, since the relative
speed of the shadow over the surface is only ~0.1–0.5 m/s. A 96 K drop
follows.

#### What this segment does *not* yet produce

Only Didymos has a thermophysical state. Dimorphos is loaded purely as an
occluding mesh; its temperatures are never computed. Any image of the system
needs them, which is §7.6.

### 7.5b The two eclipses are not symmetric, and the scar outlives the event

An earlier version of this note reported both mutual eclipses as lasting
93.1 minutes. **That was an artefact of the measurement, not physics.** The
scan tested `perp < R1 + R2` for both cases — a criterion symmetric in the
two bodies, so identical durations were guaranteed by construction. A second
error compounded it: the shadow drift rate was estimated by differencing the
perpendicular offset across its own minimum, which returns ~0 by symmetry and
gave a nonsensical 459-minute totality.

Measured properly, with an umbra test for the secondary and a spot test for
the primary, and the drift rate taken away from the minimum (0.168 m/s, close
to Dimorphos's 0.183 m/s orbital speed, as it should be at a central
crossing):

| event | duration |
|---|---|
| Dimorphos totally inside Didymos's umbra | **57 min** (91 min including partial phases) |
| Dimorphos's shadow spot crossing Didymos's disk | 93 min |
| a *single Didymos facet* inside that spot | **~10–16 min** |

The asymmetry follows from the rotation states, and it runs the way intuition
says it should:

- **Dimorphos is tidally locked**, so its rotation period *is* its orbital
  period. During totality the whole sunlit hemisphere is dark at once and
  stays dark for the full 57 minutes — no facet gets relief by rotating out.
- **Didymos spins in 2.26 h** against Dimorphos's 11.37 h orbit, so the
  shadow spot sweeps across the surface at ~0.3 m/s rather than dwelling.
  Its 90 m radius takes 93 minutes to cross the disk, but any given facet
  spends only ~12 minutes inside it.

So the expected temperature drop is much deeper on the secondary. That run is
not done yet (§7.6), but the primary's numbers already bound it from below.

#### The trailing shadow: measured

Because the spot sweeps rather than dwells, a natural question is whether the
cooled track survives to the next rotation. Extending the segment three
Didymos spins past the study epoch and differencing `mutual` against `self`
— so topographic self-shadowing cancels and only Dimorphos's contribution
remains:

| spins after epoch | hours | worst ΔT | facets < −5 K | worst band-radiance drop |
|---|---|---|---|---|
| −0.25 | −0.59 | −46.3 K | 270 | −61.3 % |
| **0** | 0.04 | **−93.7 K** | 384 | **−78.9 %** |
| +0.25 | 0.58 | −83.6 K | **656** | −74.3 % |
| +0.50 | 1.12 | −27.4 K | 566 | −44.1 % |
| **+1.00** | 2.29 | **−10.2 K** | 201 | **−16.5 %** |
| +1.50 | 3.38 | −5.7 K | 6 | −11.2 % |
| +2.00 | 4.54 | −3.8 K | 0 | −6.5 % |
| +3.00 | 6.72 | −2.4 K | 0 | −4.5 % |

**The scar is still plainly visible one full rotation later**: −10.2 K and a
−16.5 % band-radiance deficit at +2.29 h, far above TIRI's radiometric
accuracy. It takes 1.87 spins to fall below 5 K anywhere, and after three
spins it is still −2.4 K / −4.5 %, having never risen above −1 K within the
6.7-hour window.

The facet *count* peaks a quarter-spin **after** the study epoch (656 facets
below −5 K against 384 at the epoch), because the spot is still crossing the
disk until 06:22 while the earliest-shadowed facets have not yet recovered.
The instantaneous shadow and the accumulated track are therefore two
different features, and the image at the study epoch contains both.

There is also a residue of the *previous* eclipse, 11.4 h (5 spins) earlier,
at the −1 K level on 13 facets just before ingress — so successive events
partially accumulate rather than fully resetting.

**Consequence for the deliverable**: the simulated image cannot be produced
from a snapshot of the instantaneous shadow geometry. It requires the
thermal history, which is what makes the in-loop segment necessary rather
than merely convenient.

### 7.6 Dimorphos, and choosing the segment length by the right clock

Six Didymos rotations was chosen as "a few rotations", and for the primary it
is defensible: the diurnal wave has a 2.26 h period, so six spins is ~6
e-foldings of the surface layer's memory and the eclipse signal is fully
developed.

It is the wrong clock for the secondary. Dimorphos is tidally locked to an
11.9 h orbit, so its rotation period *is* its orbital period, and 13.6 hours
of segment is **1.1 Dimorphos days** — barely one. Worse, the relevant
forcing for a tidally locked secondary includes the total eclipses it suffers
passing through the primary's shadow, whose cadence is also the orbital
period. Two or three Dimorphos orbits (24–36 h) is the minimum that gives its
surface a settled diurnal cycle plus a repeated eclipse history.

At 7 ms/step that is 3–4x the current segment and still seconds of wall time,
so the cost is not the obstacle — but it is where the geometric
eclipse-window optimisation starts to pay, since a longer segment spends a
larger absolute time outside any mutual event.

## 7.2 Mutual heating

Each body sees the other as an extended source of

- **reflected sunlight**, `~ albedo * F_sun * view_factor`, and
- **thermal emission**, `~ epsilon sigma T^4 * view_factor`.

At the Didymos-Dimorphos separation (~1.15 km, bodies ~800 m and ~170 m
across) the view factors are not negligible, and they peak exactly during the
mutual events being modelled.

The radiative kernels already exist in `tpm/core.rs` (both solar terms now
clamp `cosi` at zero — see the note at the end of §6):

```rust
radiation_sun_reflected(viewf, a, cosi, dau)      // reflected solar
radiation_sun_reflected_reuse(viewf, f, a)        // reflected, flux reused
radiation_emitted(viewf, t, e)                    // thermal IR, eps sigma T^4
```

and `mesh.rs` provides `view_factor_facets(face_a, face_b, trans_b2a)` plus
the scalar forms. So the missing piece is the per-timestep accumulation and
the view-factor matrix, not the physics kernels.

Cost scales as `n_facets(A) x n_facets(B)` per step if done naively — 10^8 for
two 10k meshes, far too slow. The standard treatment is to precompute the
view-factor matrix once in the body-fixed frames and re-use it, which works
here because Dimorphos is tidally locked: the relative geometry repeats every
orbit rather than every timestep.

## 7.3 Self-heating

Within one body, concave regions exchange radiation: a facet inside a
depression sees other facets of the same body, receiving both reflected solar
and thermal IR from them. This raises daytime temperatures in bowls and
crater floors and is the same physics as roughness (§8) but at facet
resolution rather than sub-facet.

**The near-field caveat is the crux here.** The view factor is a
point-to-point approximation valid for `d >> facet size`, which is exactly
what neighbouring facets violate — and neighbours are the dominant
contributors inside a depression. Subdividing neighbouring facets and summing
the sub-pairs is the honest treatment; see §9, where the current code instead
returns zero in that regime.

The view-factor machinery is otherwise shared with §7.2; the extra requirement
is a visibility test between facet pairs of the same body, since a view factor
is only valid if the two facets can actually see each other. `intersect_mesh`
does this exactly but is brute force; the shadow-map query does it from one
direction at a time. Precomputing the self-visibility matrix once per body is
the tractable route, again exploiting that it is fixed in the body frame.

**Ordering.** These three are the difference between "a 1D column model of a
sphere" and "a thermophysical model of a binary system at a mutual event". Do
7.1 first — it is a boolean multiplier on a term already present, uses
machinery that is built and validated, and it dominates the signal during the
transit. 7.2 and 7.3 both reduce to view-factor bookkeeping and can share one
implementation.

## 7.4 Two-phase strategy, and whether spin-up can omit them

The terms above are episodic or geometrically local, while the spin-up exists
to settle the *deep, orbit-averaged* field. That motivates splitting the run:

1. **Coarse spin-up.** Two orbits, 1D columns, direct insolation only. Save
   the full equilibrated column state.
2. **High-fidelity final segment.** Restart from that state a few diurnal
   cycles before the study epoch and switch everything on — eclipse and self
   shadowing, mutual and self heating, reflection and emission.

This is cheap where it can be and exact where it matters, and it makes each
effect separately measurable: run the final segment with terms enabled one at
a time and difference the result. That ablation *is* the quantification of how
much binary mutual effects contribute, which is a result worth publishing on
its own rather than a diagnostic.

**The catch to check first** is whether any facet's temperature is set by
something other than direct sun. On the Moon, permanently shadowed crater
floors are heated almost entirely by scattered light and thermal IR from their
own walls; omit self-heating there and the model does not make a small error,
it produces a qualitatively wrong, far-too-cold surface. Worse for a staged
run, such a facet could not be repaired in a short final segment: its
temperature is set by the *orbit-averaged* balance, so the deep field carries
the bias and needs a seasonal timescale to relax.

**Measured for Didymos.** Sampling one full orbit at 20,000 epochs
(step 0.84 h, incommensurate with the 2.26 h spin so rotation and orbital
phase are both covered), taking each facet's peak `cos i`:

| | |
|---|---|
| facets never facing the Sun | **0 of 10,000** |
| facets peaking below `cos i = 0.05` | **0** |
| min / median / max peak `cos i` | 0.273 / 0.923 / 1.000 |
| obliquity (spin axis vs orbit normal) | 165.4 deg |

The worst-illuminated facet still reaches `cos i = 0.27`, i.e. 74 deg
incidence, at some point in the orbit. There are no permanently shadowed
regions in the lunar sense, and the reason is the obliquity: at 14.6 deg from
the orbit normal (retrograde), the polar regions get a genuine seasonal sun
cycle rather than the Moon's near-1.5 deg permanent grazing.

**So self-heating is not required for the spin-up on Didymos.** Every facet is
directly forced, so self-heating is a modest systematic warm bias — bounded by
facet-scale concavity, which is limited at ~10 m resolution — rather than the
dominant term for any facet. Omitting it during phase 1 and enabling it in
phase 2 is defensible, and the ablation above measures what was left out.

**Residual caveat.** This tests facet *orientation*, not terrain blocking: a
facet can face the Sun and still be occluded by a ridge or boulder. Ruling
that out needs the shadow-map query rather than `cos i`, which is cheap now
that §7.1 exists — worth running once over an orbit before trusting the
staged result. The conclusion would only change if that turned up facets
shadowed for essentially all of the orbit.

**Dimorphos is a separate question.** It is tidally locked, so it keeps one
face toward Didymos permanently, and it is eclipsed regularly. Its illumination
statistics need computing separately before assuming the same conclusion
carries over.

## 7.5 Spin-up result and convergence

Two-orbit spin-up, 10,000 facets, 34-node geometric grid, vectorised:
**2,162,356 steps in ~55 min** (1.52 ms/step). Surface 88.3-346.3 K, mean
265.2 K.

Physically consistent where it can be checked independently: the study epoch
falls at **1.025 AU**, near perihelion, where subsolar equilibrium is 392.6 K.
The peak surface temperature is 346.3 K — below equilibrium, which is what
thermal inertia must do. The diurnal spread damps monotonically from 258 K at
the surface to 12 K at 6.17 m.

**The deep field is not converged, and the initialisation disguises it.** The
base node reported 200.11 K against a `T_INIT` of 200.0 — the number that
looks most reassuring is the least informative, because it is the initial
condition barely touched. Thermal information diffuses only

```
sqrt(D t) = sqrt(3.9e-8 * 1.21e8) ~ 2.2 m
```

in two orbits, while reaching the 6.17 m base needs `z^2/D ~ 31 yr`, about 16
orbits.

Measured by continuing a **third orbit** from the saved state and differencing
at the study epoch. The two runs are

| | |
|---|---|
| A | `out/hera_didymos/didymos_tpm_2orbit` — 2 orbits from a flat `T_INIT = 200 K` |
| B | `out/hera_didymos/didymos_tpm_3orbit` — restarted from A, one more orbit |

identical otherwise (geometric 34-node grid, 10,000 facets, vectorised,
`dt = 55.94 s`, direct insolation only). Note 700 d / 2.26 h = 7433.6
rotations, not an integer, so the restart lands ~0.6 rotation out of phase.
That injects a surface transient which re-equilibrates within hours — the
diurnal skin depth is 1 cm — so it is gone long before the study epoch a full
orbit later.

| | surface | z=1.42 m | z=3.56 m | z=6.17 m |
|---|---|---|---|---|
| change per orbit | +0.061 K | -0.069 K | +0.298 K | +0.540 K |

| surface statistic | value |
|---|---|
| max abs change | **1.69 K** |
| mean / median | 0.51 / 0.34 K |
| facets changing >1 K | 1,749 of 10,000 (17.5%) |

**Two different things are being conflated by the phrase "not converged", and
only one of them matters.** Diffusion reaches depth `z` in `t ~ z^2/D`:

| depth | in `ls1` | equilibration time | seasonal amplitude `exp(-z/ls1)` |
|---|---|---|---|
| 0.87 m | 1.0 | 0.32 orbit | 37% |
| 2.2 m | 2.5 | **2.05 orbits** | 7.9% |
| 6.17 m (base) | 7.1 | **16 orbits ~ 31 yr** | 0.08% |

After two orbits the column has equilibrated to ~2.2 m, which is already past
where the seasonal wave has decayed to 8%. So the **seasonal wave itself is
converged**; what is not is the near-isothermal reservoir below it, where the
wave is effectively dead and which acts only as a slowly drifting lower
boundary. Its measured leak to the surface is 0.06 K per orbit.

The irony is worth acting on: because `t ~ z^2`, **making the column deeper
slows convergence quadratically while adding almost nothing physically**. At
6.17 m we spend 16 orbits settling a layer that carries 0.08% of the seasonal
amplitude. A column at `DEPTH_IN_SEASONAL ~ 0.55` (about 3 m, 3.5 `ls1`, where
the wave is still 3%) would converge in roughly 4 orbits instead of 16 and
lose nothing measurable — the adiabatic base is an equally good approximation
either way once the wave has decayed.

**Whether 1.7 K matters depends on the observable.** In the TIRI band it is not
negligible: at 10 um and 300 K,

```
dB/B ~ (hc / lambda k T) dT/T = 4.8 * 1.7/300 ~ 2.7 %
```

worst case, ~0.8% typical. That sits below the uncertainty from thermal
inertia and unmodelled roughness, but it should be carried as a stated error
term rather than called converged.

**Use the 3-orbit state for phase 2** — it is strictly closer to equilibrium
at no extra cost now that it exists. And for any future run, initialising the
column at the estimated equilibrium temperature rather than a flat 200 K would
remove most of this: the deep layers are slowly walking toward a value that
can be estimated analytically in advance from the orbit-averaged insolation.

---

# 7.7 Both bodies, and the GIS3D TIRI product

## Dimorphos's own thermophysical state

`tpm.py` is now parameterised by `BODY`, so the same script spins up either
body. Dimorphos needs no different physics, only a different grid: the grid
follows the *diurnal* skin depth, and Dimorphos is tidally locked at 11.37 h
against Didymos's 2.26 h, so its skin depth is sqrt(5) larger. 29 nodes to
5.54 m against Didymos's 34 to 6.17 m, and a stability limit of 703 s against
140 s. Three solar orbits took 644,850 steps at 1.46 ms/step, ~16 minutes.

One trap: `DIMORPHOS.orbit_period` is its **11.9 h orbit around Didymos**,
not a heliocentric year. Using it for the seasonal skin depth would build a
column centimetres deep. The script takes `DIDYMOS.orbit_period` for both,
which is the physically correct shared heliocentric period.

A second, harder one: mission kernels cover 2026-07 to 2027-07, and both
`spkpos` and `pxform` for Dimorphos fail with `SPKINSUFFDATA` before that.
Didymos has a Horizons ephemeris and a rotation model back to 2023;
Dimorphos has neither, so a multi-orbit spin-up cannot be driven from kernels
alone. Two approximations bridge it:

1. The Sun's direction is taken relative to Didymos. The bodies are 1.19 km
   apart at 1.5e8 km, so the directions differ by 8e-9 rad.
2. Dimorphos's body-fixed frame is extended backwards as a uniform rotation
   about its own spin axis at the true rate, anchored to the real orientation
   at the study epoch. Being tidally locked, its rotation *is* uniform to
   good approximation. Measured against the kernels where both exist, the
   synthetic frame drifts **0.013 deg/day** — 1.6 deg over 120 days.

The rotational phase would not matter even if it were wrong: after thousands
of rotations the column keeps no memory of its initial phase, and what a
spin-up delivers is the deep seasonal field.

## A frame bug the ablation caught

The first `self` runs put Dimorphos alone in the scene but still placed the
Sun using *Didymos's* frame, because `before_render` assumed body 0 was
always Didymos. The shadow map and the TPM were then in different frames: the
facets the renderer reported as shadowed were not the facets the physics
thought were lit.

It announced itself through an impossibility — `self` came out **colder** than
`mutual` (215.4 K against 239.9 K), when mutual is self plus an extra
occluder and can only be colder or equal. Worth recording as a diagnostic
pattern: an ablation whose terms are nested gives a free monotonicity check,
and it caught a bug that a plausible-looking number would have hidden.

A second version of the same class: with only one body in `state`, the shared
timestep was drawn from that body's stability limit alone (281 s instead of
55.94 s), so an ablation difference would have measured the timestep as well
as the physics. The grids are now built for every body regardless of which is
stepped.

## Two-body ablation at the study epoch

| body | term | facets | worst ΔT | mean ΔT | facets < −5 K | worst radiance |
|---|---|---|---|---|---|---|
| Didymos | self-shadowing | 4,105 | −10.9 K | −0.15 K | 35 | −32.1 % |
| | **eclipse** | 3,980 | **−93.7 K** | −1.39 K | 384 | **−78.9 %** |
| | both | 4,240 | −93.7 K | −1.54 K | 413 | −78.9 % |
| Dimorphos | **self-shadowing** | 7,791 | **−116.2 K** | −4.60 K | 1,865 | **−92.2 %** |
| | eclipse | 9,213 | −11.0 K | −0.63 K | 1,234 | −26.9 % |
| | both | 8,780 | −115.2 K | −5.23 K | 2,907 | −89.8 % |

The two bodies are dominated by *different* terms, and the reason is the
epoch. At 05:36 Dimorphos is at conjunction — between Didymos and the Sun,
fully lit — so its −11 K is not a live eclipse but residual memory of its own
total eclipse six hours earlier, half a Dimorphos day back. What dominates
Dimorphos instead is self-shadowing, at −116 K: it is more irregular than the
primary and rotates five times slower, so a facet that falls into a
topographic shadow stays there long enough to lose more than a hundred
kelvin.

Evaluated three spins later the ranking shifts again — Dimorphos's eclipse
term reaches −37 K, because its second total eclipse (10:48–11:45) ends
forty minutes before. Which instant is chosen changes the answer completely,
which is the practical form of the §7.5b conclusion.

## Dimorphos spin-up: the staged plan

What was run is the first stage of a three-stage plan, and the later stages
are not done:

1. **Coarse seasonal initialisation** (done). Dimorphos is treated as
   co-located with Didymos with respect to the Sun -- the 1.19 km offset is
   8e-9 of the heliocentric distance -- but rotating on its own 11.37 h
   period. That gives the correct heliocentric distance history and the
   correct diurnal cycle, which is what sets the deep seasonal field. The
   synthetic tidally-locked frame described above is what makes it possible
   outside kernel coverage.
2. **Several correct orbits around Didymos** (not done). Once inside kernel
   coverage, run with the true relative geometry so the secondary's own
   eclipse history -- 57 minutes of totality every 11.37 h -- is imprinted
   before anything is measured. Section 7.5b shows those events leave
   residues that partially accumulate, so a state that has never seen one is
   not the right starting point.
3. **Phase 2** (done, but starting from stage 1 rather than stage 2). The
   binary mutual effects at high fidelity.

Stage 2 is the gap. The current phase 2 segment covers 20.4 h, which contains
two total eclipses of the secondary, so it is not starting cold -- but its
restart state carries none. Two or three Dimorphos orbits of stage 2 would
settle that, and at 7 ms/step it costs seconds. This is also where the
geometric eclipse-window optimisation starts to matter, since a longer
segment spends proportionally more time outside any mutual event.

## The facet-index buffer

`sim.request_facet_id()` / `sim.facet_id_map()`, backed by
`src/app/facet_id.rs` and `shaders/facet_id.wgsl`. The scene is rendered a
second time through the same view matrix into an `R32Uint` target holding
`1 + offset + facet` per pixel, 0 where nothing is drawn, and read back.

The alternative was to colour facets by radiance and read the colour image
back. That was rejected: colour quantises to 8 bits, is entangled with
lighting and tone mapping, and inverting a colormap to recover a physical
number is exactly the kind of step that silently loses accuracy in a product
whose pixel values *are* the deliverable. It would also have run straight
into the known per-body colour-mode bug, where `mesh.color_modes[:] = 1` has
no effect because the shader reads only the scene-wide `globals.color_mode`.

With an index buffer the mapping is exact and the lookup happens in numpy at
full float precision. Visibility comes free: the pass carries its own depth
buffer, so a facet absent from the map is one the instrument genuinely cannot
see — over the limb, outside the field of view, or hidden by the other body.

Implementation notes worth keeping:

- The facet index is `vertex_index / 3`, so the pass is only valid for
  **flattened** meshes where each facet owns its three vertices. Indexed
  meshes are skipped rather than given indices that look valid and are not.
- `@interpolate(flat)` on the index — interpolating it across a triangle
  would blend indices into meaningless values.
- Culling is off. A shape model with inconsistent winding would otherwise
  drop facets, and a missing facet is indistinguishable from an occluded one.
  Depth still decides what is in front.
- Bodies share one index space through a per-body offset supplied as a
  dynamic-offset uniform, so one readback resolves both which body and which
  facet.
- It reuses the renderer's existing view bind group rather than keeping a
  second camera uniform, which could silently disagree with what was drawn.

## Radiance, and a units error worth recording

`kalast/tpm/radiance.py`. Band radiance is a scalar function of temperature
once the filter is fixed, so it is tabulated once over 30-500 K and looked up
with `numpy.interp`; direct evaluation would mean a
`(n_facets, n_wavelengths)` Planck array per call. Interpolation error is
1e-4 relative, reported by the class rather than asserted.

Two reductions are defensible and differ by six orders of magnitude:

- **band-integrated radiance**, `integral B eps R dw`, in W/m2/sr;
- **band-averaged spectral radiance**, `integral B eps R dw / integral R dw`,
  in W/m2/sr/um.

**The first version of this work used the second, and it was wrong.** The
argument for it was that `Response_Fil-a..f` carried a factor 0.5 that
`Response_Fil-g` did not, so a quantity linear in `R` would misreport the
narrow filters -- and the normalised form cancels the response scale.

That factor does not exist. It came from dividing `Response_Fil-x` by
`Bolometer * Lens * Filter-x` at wavelengths where the denominator is nearly
zero, in columns quoted to four decimals, and taking the minimum of a
quotient that is quantisation noise there. Checked properly, on the median
rather than the extremes, the ratio is **1.000000 for all seven filters**:
`Response_Fil-x` *is* the product, an absolute throughput with peaks between
0.37 and 0.81.

The error announced itself observationally. Because a band *average* is
almost filter-independent, the wide band `g` came out no brighter than the
narrow `a` -- which is physically absurd for a 5x wider filter, and was
spotted on opening the files.

Settled against ground truth: a real calibrated TIRI product
(`data_calibrated/necp/.../tiri_cal_*.fits`) carries
`BUNIT = 'W m^-2 sr^-1'`. So the simulated frames must be band-integrated to
be comparable at all. In those units, at 345 K:

| filter | eff. wavelength | bandwidth `integral R dw` | L(345 K) |
|---|---|---|---|
| a | 8.43 um | 0.514 um | 9.24 W/m2/sr |
| b | 8.61 um | 0.669 um | 12.01 |
| c | 9.59 um | 0.598 um | 10.33 |
| d | 10.44 um | 0.382 um | 6.19 |
| e | 11.53 um | 0.431 um | 6.26 |
| f | 12.66 um | 0.361 um | 4.59 |
| **g (wide)** | 10.27 um | **2.707 um** | **43.66** |

`g` is 4.7x `a`, tracking the bandwidth ratio of 5.3x as it should.
`band_averaged` still exists for pipelines that quote radiance per unit
wavelength, and the ratio of the two is a useful diagnostic.

**Two lessons.** Do not characterise a ratio by its extremes when the
denominator passes through zero. And a physical cross-check -- "the wide
filter must read higher" -- caught in seconds what the derivation did not.

**No emission-angle cosine.** A grey Lambertian surface emits the same
*radiance* in every direction; the cosine enters only when integrating to a
flux. A pixel measures radiance and the projected area is already accounted
for by which pixels a facet covers. (`rad.py` carries a `cose` term because
it goes on to sum irradiance over the disk -- a different quantity.)

Reflected sunlight is omitted, having been checked rather than assumed: at
1.02 AU with A=0.07, reflected solar over 8-14 um is 0.032 W/m2/sr against
90.4 emitted at 345 K -- 0.04 %, rising only to 0.16 % at 250 K. The argument
closes itself, since reflected sunlight exists only on lit facets, which are
the warm ones.

## Why Dimorphos self-shadows so much more than Didymos

A surprise from the ablation: Dimorphos's self-shadowing costs it -116 K at
worst, against -11 K for Didymos, and at the study epoch it exceeds the
mutual eclipse. Two real effects and one caveat.

**More of its surface is shadowed.** Median day-side self-shadowed facets
over the segment: 739 for Dimorphos against 118 for Didymos, of 10,000 --
6.3x. It is the more irregular body relative to its size.

**Each shadowed facet stays shadowed far longer.** Dimorphos is tidally
locked at 11.37 h against Didymos's 2.26 h, so topography that blocks the Sun
blocks it for five times as long in absolute time. The worst Dimorphos facet
would peak at 324 K unshadowed and reaches only 271 K, and stays more than
5 K below the no-shadow run for 15.2 h. Its grid works *against* the effect
-- a 6.76 mm first layer against Didymos's 3.02 mm is more thermal mass in
the surface node, so a slower response -- and the drop is still an order of
magnitude larger.

**The caveat: the two are not compared at equal ground resolution.** Both
meshes carry 10,000 facets, but Didymos is 780 m across and Dimorphos 170 m,
so mean facet scales are **13.1 m and 2.73 m** -- Dimorphos is resolved 4.8x
finer. A finer mesh resolves more topography and therefore more
self-shadowing, so some unknown part of the 6.3x facet-count ratio is
resolution rather than shape. The *duration* argument is unaffected, being
purely rotational, and the ranking is unlikely to reverse -- but the
comparison should not be quoted as a shape difference without re-running
both at matched ground scale. Worth doing when the full-resolution meshes
are used.

## The product

`examples/hera_didymos/tiri_fits.py` writes seven FITS files, one per TIRI
filter, 1024x768 to match the real detector, `BITPIX=-32`, `BUNIT` in
W/m2/sr/um. Headers carry the model provenance — grid, stencil, solver,
spin-up length, which shadowing terms are on, that mutual and self heating
are **not**, thermal inertia, albedo, facet count, and the two methodological
choices above.

At the study epoch Hera is 25.8 km from Didymos, so the primary spans ~133
pixels and the pair fills 1.56 % of the frame. Dimorphos's shadow is an
obvious dark ellipse ~30 px across, and it is far more striking in radiance
than in temperature, which is the T^4 sensitivity made visible.

The temperature state is taken from a step landing 19 s from the epoch —
`tpm_phase2.py` saves that exactly, rather than letting the FITS pick from
the 280 s snapshot series, which would have been a third of the time the
shadow spot needs to cross a facet.

## Context frames for the delivered FITS

`examples/hera_didymos/tiri_movie.py` and `tiri_movie_compose.py`. The FITS
are a single instant, and a single instant of a binary in mutual event is
hard to read, so the same scene is rendered across +/-6.5 h of the study
epoch -- 13 hours, 1.14 Dimorphos orbits -- through the real TIRI pointing.
Three frames per epoch: diffuse for geometric context, surface temperature,
and the TIRI wide band that the FITS actually carry.

168 frames at a 280 s cadence, 10.5 s of wall time. The cadence is not
chosen: it is the phase 2 snapshot interval, so every frame is a state the
model computed rather than an interpolation between two.

The sequence contains all three mutual events of that orbit, and their
durations come out as §7.5b predicts from geometry alone:

| event | window | duration |
|---|---|---|
| Dimorphos in Didymos's umbra | −6.41 to −4.94 h | 93 min |
| **Dimorphos's shadow on Didymos** | **−0.74 to +0.74 h** | **93 min** |
| Dimorphos in Didymos's umbra | +4.93 to +6.41 h | 93 min |

The two umbra passages sit half a Dimorphos orbit either side of the shadow
transit, as they must: the secondary is between Sun and primary at the study
epoch and behind it half an orbit away.

Two presentation decisions worth keeping. The scales are **fixed** across the
sequence -- autoscaling makes a cooling body look constant, which is the one
thing the movie exists to show. And the crop is a single box computed from
the whole sequence rather than per frame, so the bodies move within a fixed
field instead of appearing still while the frame moves around them. The
export writes full 1024x768 frames and the crop happens in a separate
compose step, so the sequence can be recomposed without re-rendering.

TIRI needs no substitute pointing over this window: it is nadir-pointed at
Didymos throughout, measured at 0.000 deg off-axis every hour.

# 8. Next steps: thermal surface roughness

## 8.1 The science

Real regolith is rough far below facet resolution — the 10k mesh resolves
~10 m facets on Didymos, while the thermally relevant structure runs from
centimetres (the diurnal skin depth is 1 cm) to metres. That unresolved
roughness changes the emitted radiance in ways a smooth-facet model cannot
reproduce:

- **Thermal beaming.** Sunlit slopes tilted toward the observer are hotter
  than the facet mean, and at small phase angle you preferentially see exactly
  those slopes. A rough surface therefore appears *warmer* than a smooth one
  of the same thermal inertia, and the effect is strongly phase-dependent.
  This is what the NEATM beaming parameter eta absorbs empirically, and what a
  thermophysical model should instead produce from geometry.
- **Self-heating inside depressions.** Facets of a bowl irradiate one another
  in the thermal IR and multiply-scatter sunlight, raising the floor
  temperature above what an isolated flat element would reach.
- **Sub-facet shadowing.** At grazing incidence, parts of each depression are
  shadowed, which lowers the mean and — because shadowed and lit sub-elements
  have very different temperatures — makes the *fourth-power* average diverge
  from the average temperature. Radiance responds to `<T^4>`, not `<T>^4`, and
  roughness widens that gap.
- **Directional emissivity.** The angular distribution of emitted flux
  departs from Lambertian, mattering most at high emission angle near the limb.

The standard parameterisations treat each facet as carrying a sub-grid
depression — a spherical-cap or hemispherical crater (Lagerros; Spencer;
Rozitis & Green) — or a Gaussian random surface characterised by an RMS slope.
kalast already carries much of this: `rms_slope`, `rms_slope_hemisphere`,
`rms_slope_terrain`, `distribution_slope_angles`, `z_in_crater`,
`curvature_radius`, `largest_slope_angle_sphere`, plus the whole
`examples/crater_self_shadow/` study.

## 8.2 Step one: a geometric correction to the radiance

Keep the smooth-facet TPM exactly as it is; correct the emitted radiance
afterwards. The hook already exists — `examples/hera_mars_swingby/rad.py`
carries

```python
R = numpy.ones((nit, nface))   # roughness correction placeholder
```

and threads `R[it, iif]` into `spectral_radiance`. Today it is unity.

Plan:

1. Build a crater model once: a spherical-cap depression discretised into
   sub-facets, parameterised by an opening angle (equivalently an RMS slope).
   `z_in_crater` and `curvature_radius` already generate the geometry.
2. For a grid of (incidence, emission, azimuth, RMS slope), solve the
   sub-facet energy balance including shadowing, self-heating and multiple
   scattering — steady state first, which is defensible when the depression is
   small compared with the diurnal skin depth response time.
3. Integrate sub-facet Planck emission weighted by projected area toward the
   observer, and divide by what a smooth facet at the TPM temperature would
   emit. That ratio is `R`.
4. Store it as a lookup table, interpolate per facet per epoch in `rad.py`.

Cheap, reversible, and directly testable: `R = 1` recovers today's result, so
the change is isolated. Its limitation is conceptual and should be stated
plainly — it corrects what is *emitted* while leaving the subsurface
temperature field smooth, so it captures beaming but not the way roughness
alters the mean temperature and thermal lag.

## 8.3 Step two: roughness in the boundary condition

Move the depression into the solver. Each facet carries N sub-facets, each with
its own 1D column, and the surface boundary condition for sub-facet *j* becomes

```
absorbed_j = (1-A) [ F_sun cos i_j * lit_j  +  sum_k F_scat,k->j ]
           + sum_k epsilon sigma T_k^4 * VF_k->j
```

with `lit_j` the sub-facet shadowing and `VF` the intra-crater view factors —
the same bookkeeping as §7.3, one level down. Emitted radiance then integrates
true `<T^4>` over sub-facets rather than applying a correction to `<T>`.

This captures what step one cannot: self-heating raises the floor temperature
throughout the diurnal cycle, which changes the subsurface gradient and hence
the apparent thermal inertia retrieved from a lightcurve. Since retrieved
inertia is the headline product of this kind of modelling, the difference
between the two steps is not cosmetic.

Cost is `N` times the columns — N ~ 50-100 in the literature, so 10k facets
becomes 0.5-1M columns. **This is only tractable after the facet loop is
vectorised** (§6), which is one more reason that work comes first.

Validation path: with roughness switched off the two steps must agree with
today's smooth result; against each other, step two minus step one isolates
the thermal-history contribution that the geometric correction misses.

## 8.4 Future work: Hapke bidirectional reflectance

The reflected-solar term is currently a single Lambertian albedo. Hapke's
bidirectional reflectance model instead describes the surface with single
scattering albedo, an opposition-effect amplitude and width, a phase function,
porosity, and a **macroscopic roughness** parameter — that last one describing
the same unresolved topography as §8.1, from the optical side.

Two reasons it belongs on this list:

- TIRI's shorter filters and any AFC visible simulation both need a reflected
  component that a Lambertian albedo gets wrong at non-trivial phase angle,
  especially near opposition.
- **Consistency.** The thermal roughness of §8.3 and Hapke's photometric
  roughness are two views of one surface. Deriving each independently — thermal
  from TIRI, optical from AFC — and checking that they agree is a genuine test
  of the model rather than a fit. Disagreement would be informative about
  either the roughness parameterisation or the scale each observable is
  sensitive to.

## 8.5 Future work: FEM and lateral heat transfer

Everything above assumes heat flows only vertically: independent 1D columns,
no lateral conduction between facets. That holds while the lateral temperature
gradient varies slowly compared with the skin depth, which is fine for a large
body at coarse resolution.

It becomes questionable exactly where this project is heading:

- **Dimorphos is small compared with its own seasonal skin depth.** With the
  Didymos thermal properties above, `ls2pi_seasonal = 5.45 m` against
  Dimorphos semi-axes of 88.5 x 84 x 57 m — the seasonal wave penetrates
  roughly 6-10% of the body. A 1D column of that depth is no longer short
  compared with the local radius of curvature, so the geometry the column
  assumes is wrong, and the deep boundary is not an infinite half-space but
  the rest of a small body.
- **Sharp lateral gradients.** Across the terminator, and across the rim of an
  eclipse shadow during a mutual event, the surface temperature changes by
  ~100 K over a distance that can approach the skin depth. Lateral conduction
  smooths precisely the feature a thermal camera is resolving.
- **Tidal locking.** Dimorphos's spin equals its orbit (11.37 h), so it keeps a
  fixed face toward Didymos. The sub-Didymos and anti-Didymos hemispheres see
  systematically different mutual heating, sustaining a long-lived lateral
  gradient that no set of independent columns can relax.

A 3D conduction solver — finite elements on a tetrahedral mesh, or finite
volumes on a voxelised interior — would capture lateral transport, curvature
and whole-body seasonal storage together. The interesting comparison is not
"FEM is more correct" but *where and by how much* the 1D approximation
departs: quantifying that on Dimorphos, over a full orbit including mutual
events, would be a result in its own right, and would tell every other 1D
binary-asteroid model where its own validity ends.

---

# 9. Revisiting view factors

§7.2, §7.3 and §8.3 all reduce to view-factor bookkeeping, so the current
implementation is on the critical path three times over. It has problems worth
fixing before building on it.

## 9.1 What is there now

`mesh.rs::view_factor_facets` computes the differential form

```
VF = cos(theta_a) cos(theta_b) / (pi d^2)
```

per unit area, guarded by two tests: both facets must face each other
(`theta < 90 deg`), and

```rust
if distance_a2b < face_b.area.sqrt() {
    return 0.0;
}
```

## 9.2 Three problems

**(a) The proximity guard returns zero where the view factor is largest.**
The differential form assumes `d >> facet size`, and diverges as `d -> 0`, so
a guard is needed. But returning **0** is the worst available answer. Adjacent
facets have centroid separation of order `sqrt(area)` — exactly the threshold —
so the guard fires on precisely the neighbours that dominate self-heating
inside a concave region. Applied to §7.3 or §8.3 as written, most of the
self-heating would silently vanish, and the model would run and produce
plausible-looking, systematically cold results. The existing `TODO:
subdivide the facet and recompute` is the right fix; until then the failure is
biased, not merely approximate.

**(b) No occlusion test.** The function is pure geometry: it never asks whether
the two facets can actually see each other. The angle test catches facets that
face away, but not an intervening ridge or crater rim between two facets that
do face each other — which is the defining situation for concave terrain, and
therefore for every case where self-heating matters. Radiative exchange
between two facets separated by solid rock is currently counted in full.

**(c) `acos` then `cos`.** `angle_between` computes an angle whose cosine is
then taken. For unit vectors that round trip is exactly `dot(u, v)`. It costs
two transcendentals per pair and loses precision at small angles, over an
O(N^2) loop.

## 9.3 The GPU route: hemicube

(b) is the expensive problem — an exact visibility test between facet pairs is
`O(N^2)` ray casts, which is the same wall we hit with per-facet shadowing
before the shadow map replaced it (`2026-08-26_facet_shadow_query/`). The
resolution is the same, and it is the classic radiosity technique:

Place a **hemicube** at facet *i*, render the scene into it with each facet
writing its own **index** rather than a colour, and read the buffer back. Then

- every facet visible from *i* appears, and the depth test has already
  resolved occlusion exactly — no separate visibility pass;
- the number of pixels a facet covers, weighted by the per-pixel delta form
  factor (a fixed function of hemicube geometry), *is* its view factor;
- one render pass yields the entire row `VF[i, :]`, so the whole matrix costs
  `O(N)` passes rather than `O(N^2)` pair tests;
- the near-field problem (a) dissolves: a close facet simply covers many
  pixels, with no singular `1/d^2` and no threshold to choose.

kalast already has every piece: render-to-texture, a depth pass, per-facet
attributes, and GPU readback with the blocking/async trade already measured.
The facet-index render is the same shape as the facet-shadow compute pass,
and the **shadow proxy** finding applies directly — the occluder written into
the hemicube can be a decimated mesh, which was measured to shift results by
9 pixels in 1,040,400 (`2026-08-26_shadow_mesh_comparison/`).

## 9.4 Plan

1. Fix (c) immediately — replace `angle_between().cos()` with a dot product.
   Pure win, no behaviour change beyond precision.
2. Fix (a) honestly in the CPU path: subdivide when `d < k sqrt(area)` and
   sum the sub-pairs, rather than returning zero. Slower but correct, and it
   provides the reference the GPU path must reproduce.
3. Build the hemicube pass. Validate against (2) on a small mesh, then against
   analytically known configurations — two coaxial parallel discs and
   perpendicular unit squares both have closed-form view factors, which makes
   this testable the same way §3 tested conduction.
4. Exploit the invariance: for a tidally locked pair the relative geometry
   repeats every orbit, and self-view-factors are fixed in the body frame
   permanently. Compute once, reuse across 2.16M timesteps.

Worth noting the reciprocity check `A_i VF_ij = A_j VF_ji` and the closure
check `sum_j VF_ij <= 1` are cheap and catch most implementation errors — the
current code satisfies neither by construction, since a zeroed near-field pair
breaks reciprocity only if the guard fires asymmetrically, which it does
whenever the two facets differ in area.
