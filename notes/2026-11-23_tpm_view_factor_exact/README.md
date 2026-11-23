# The exact view factor between two triangles, and the exchange between facets

23 November 2026. Item E8 of the papers' plan (`~/projects/paper-tpm/notes/2026-11-17_plan.md`, section 3.0):
"an exact triangle-to-triangle view factor (the contour integral of Lagerros 1998, appendix A.1) as the CPU's method
and as the reference the hemicube is checked against, shared edges included". It is the first half of the radiative
exchange (E2, E8). Continues `notes/2026-11-22_tpm_stepper/`.

## Progress

- [x] The formula read in the paper itself (Lagerros 1998, A&A 332, 1123, appendix A.1; Sparrow 1963 for the
      double contour integral).
- [x] `tpm::view_factor::exact(a, b)` and `exact_polygons(a, b)` in Rust, in double precision.
- [x] Eight Rust tests: the closed forms, a closed box, reciprocity, the far field, the cases that must give zero, a
      triangle cut by the other's plane, a box closed to a rounding only.
- [x] `kalast.mesh.view_factor_exact(tri_a, tri_b)` from Python, `tests/test_view_factor_exact.py`.
- [x] The exchange on the CPU (E2): `tpm::exchange`, sparse view factors and what every facet absorbs of the
      sunlight the others scatter and of their thermal radiation, reflections followed to the end, over every
      facet of every body. Five Rust tests, `kalast.tpm.exchange` from Python, `tests/test_tpm_exchange.py`.
- [ ] Visibility between two facets (a third one in the way) for view factors computed on the CPU: the hemicube
      has it on the GPU; the CPU needs rays or clipping.
- [x] The exchange on the GPU: `tpm::gpu_exchange::GpuExchange`, `kalast._rs.tpm.gpu.GpuExchange`, within 3e-7 of the
      CPU. Three Rust tests, `tests/test_tpm_gpu_exchange.py`.
- [ ] The exchange inside the stepping loop on the GPU, without the round trip through the host (the temperatures
      and the flux stay there): `GpuStepper` taking a `GpuExchange`.
- [ ] `kalast/tpm/heating.py` as a wrapper over it (E12).

## What it computes

The share of the radiation leaving a triangle `a`, diffusely, that reaches a triangle `b`:

```text
F_ab = 1/A_a  int_a int_b  cos(theta_a) cos(theta_b) / (pi d^2)  dA_b dA_a
```

Stokes' theorem applied twice turns it into two integrals along the outlines, each in the positive sense about its
triangle's normal:

```text
F_ab = 1/(2 pi A_a)  oint_a oint_b  ln(d)  dr_b . dr_a
     = 1/(2 pi A_a)  sum over the pairs of sides (k, l) of   v_k . v_l  int_0^1 int_0^1 ln|r_k - r_l| dt_k dt_l
```

with `r = p + v t` along a side. The sign is as written: for two surfaces that face each other the nearest sides run
opposite ways and the far ones, where `ln(d)` is larger, the same way, so the sum is positive. (The first version
had the opposite sign and returned zero everywhere; the closed forms settled it.)

- **The inner integral is closed.** With `s` the position along the side in units of its length, measured from the
  foot of the other point on the side's line, and `h` that point's distance to the line in the same units,
  `int ln sqrt(a (s^2 + h^2)) ds = ln(a)/2 + [s ln(s^2 + h^2) - 2 s + 2 h atan(s/h)]/2`. Written this way one
  expression covers Lagerros's general case and his equation A1 (the point on the line, `h = 0`), where `x ln x`
  goes to zero.
- **The outer integral is numerical**, as in the paper: Gauss and Kronrod's 7 and 15 points, halved where the two
  disagree by more than 1e-11, to 40 levels. A side that meets the other's line leaves a kink of logarithm type,
  which the halving follows.
- **Two sides on one line are closed entirely**: with `F'' = ln|x|`, `F(x) = x^2 ln|x| / 2 - 3 x^2 / 4`, the double
  integral over two segments of a line is a sum of four values of `F`. For a side shared in full it is Lagerros's
  `(ln a - 3)/2`. This is the case of two neighbouring facets, where the formula of two points fails.
- **Sides at a right angle add nothing** and are skipped.

Two things the formula needs and the paper does not spell out for general shapes:

- It holds where each surface is in front of the other's plane throughout. A triangle that crosses the other's
  plane is cut by it first (Sutherland and Hodgman), and only the part in front is integrated, so the routine works
  on convex polygons of three or four corners.
- A polygon lying in the other's plane is seen edge on and gives exactly zero. Without that test two triangles of
  one plane gave 1e-16.

The arithmetic is in double precision whatever `Float` is (the application is single precision): the logarithms of
nearly equal lengths cancel.

**A stall, found the same night and removed.** The first version (commit `faad807`) took the point's distance to the
side's line as `sqrt(|d|^2/a - s0^2)`. For a point almost on the line that is the difference of two nearly equal
numbers, so the distance was noise; multiplied by the arc tangent it made the integrand rough at the 1e-10 level, which
the integrator, asked for 1e-11, halved after without end. On exact coordinates the tests passed, slowly (207 s for the
suite); on a box whose corners had been summed in single precision in two different orders, sides 2e-7 apart on lines
almost the same, one pair never returned. Now the distance is the length of what is left of `d` beside the line, and
the integrator has a budget of 20,000 values a pair of sides, so that nothing can make it run on. The same suite takes
0.1 s, the closed box sums to one within 7e-16, and the box closed to a rounding within 5e-7 (the test
`corners_that_miss_each_other_by_a_rounding_do_not_stall_it`). A real mesh shares its vertices exactly and was never
at risk; a script that builds triangles by hand was.

## Measured

`KALAST_MEASURE=1 cargo test --lib tpm::view_factor -- --nocapture --test-threads=1`

| Case | Exact routine | Closed form | Difference |
|---|---|---|---|
| Unit squares directly opposed, 1 apart | 0.199824895698 | 0.199824895698 | 8e-17 |
| The same, 0.1 apart | 0.826994522397 | 0.826994522397 | 2e-16 |
| 2 by 0.5 rectangles, 3 apart | 0.030925447879 | 0.030925447879 | 7e-16 |
| Unit squares, 20 apart | 0.000794451236 | 0.000794451236 | 2e-14 |
| Unit squares at a right angle sharing a side | 0.200043776075 | 0.200043776075 | 6e-17 |
| 0.3 wide to 2 high, sharing a side of 1 | 0.364862719916 | 0.364862719916 | 1e-15 |
| Unit squares sharing a side of 5 | 0.271036935697 | 0.271036935697 | 7e-16 |

- **A closed box** of twelve triangles looking inward (1.3 by 0.8 by 2.1): each triangle's factors to the eleven
  others add up to one within 2e-15, and `A_a F_ab = A_b F_ba` within 1e-9 for every pair. This is the closure the
  isothermal-cavity test of the exchange will stand on.
- **Far apart** (a pair 86 times its size away): 0.99997 of the formula of two points.
- For comparison, `mesh::view_factor_triangles` (subdivision, ratio 6, 4 levels) is 3.7 % high on the pair that
  shares a side, and `view_factor_facets` (two points, zero below `sqrt(area)`) is 37.8 % low there
  (`tests/test_view_factors.py`).

## The exchange (E2, on the CPU)

`src/tpm/exchange.rs`. With `F_ij` the view factor from facet `i` to facet `j`, grey surfaces without a preferred
direction, the radiosity `J` (what leaves a facet) obeys

```text
visible:  J_i = A_i (S_i + sum_j F_ij J_j)                       S the sunlight falling on it (W/m2)
thermal:  J_i = e_i sigma T_i^4 + (1 - e_i) sum_j F_ij J_j
```

and the facet absorbs `(1 - A_i) sum_j F_ij J_j` of the first and `e_i sum_j F_ij J_j` of the second (Lagerros 1998,
equations 3 to 6; Rozitis & Green 2011, equations 18 to 20). The thermal reflectivity is `1 - e`, by Kirchhoff's law.

- **One matrix for every facet of every body.** The sums run over all of them, so what one body scatters to the other
  and back is in the solution without a term of its own. The Python module solved the reflections on one body's
  square block and added the other body's light once.
- **`ViewFactors`**: compressed sparse rows, built from a hemicube's dense rows (`from_dense`, with the threshold of
  1e-7 measured in August), from sparse rows (`from_csr`), or exactly for surfaces that all see each other (`exact`).
- **`absorbed(factors, albedo, emissivity, incident, temperature, bounces)`**: one value a facet for albedo and
  emissivity. The radiosity is found by substitution, each pass one reflection more; `Bounces { most: 1 }` is the
  direct term, `most: n` follows `n - 1` reflections, `most: 0` goes on until a pass changes no radiosity by more
  than `tolerance` (1e-6) times the largest source, and refuses after 10,000 passes (a cavity that absorbs nothing).
  The sums are in double precision; the fluxes come back in `Float`.

Measured (`KALAST_MEASURE=1 cargo test --lib tpm::exchange -- --nocapture --test-threads=1`):

| Test | Result |
|---|---|
| Two facets that see only each other, against the 2 by 2 system solved by hand | scattered and thermal within 1e-4; one bounce is the direct term; each bounce more is nearer |
| An isothermal closed box (twelve triangles, exact factors) at emissivity 1, 0.9, 0.5, 0.2 | every facet absorbs `e sigma T^4`, what it emits, within 7e-8; with one bounce `e^2 sigma T^4` |
| Sunlight put on two facets of the closed box, albedo 0.1, 0.6, 0.95 | 877.5000 W absorbed of 877.4999 W entered, in 10, 42 and 403 passes |
| A box that absorbs nothing (albedo 1) | refused: "did not settle" |

From Python (`tests/test_tpm_exchange.py`): the same fluxes as `kalast.tpm.heating` to 2e-5 on a random matrix of
400 facets at 1, 3 and 8 bounces, and the isothermal box at three emissivities.

## The exchange on the GPU

`src/tpm/gpu_exchange.rs`, `shaders/tpm_exchange.wgsl`. The same sums, one thread a facet over its own row of the
sparse view factors; the two radiosities (visible, thermal) are advanced together, in two arrays that change places
between dispatches; a call is one submission: the sources, a dispatch a reflection, what is absorbed. Eight storage
buffers, the number a device has to allow.

- **The passes are counted before anything is sent.** A thread cannot say that the series has settled, and reading
  the radiosity back at each pass would cost more than the passes. Each pass shrinks what is left by at most the
  largest `reflectivity x row sum` over the facets, known from the view factors and the surfaces, so
  `GpuExchange::passes` gives as many as bring `ratio^k / (1 - ratio)` under the tolerance, and refuses a ratio of
  one (a cavity that absorbs nothing) before sending. It is a bound: 16 passes where the CPU stopped at 10 and 8.
- A matrix, its surfaces and the device make one `GpuExchange`; `absorbed(incident, temperature, bounces)` writes
  the two arrays, runs and reads the result back.

Measured (`KALAST_MEASURE=1 cargo test --lib tpm::gpu_exchange -- --nocapture --test-threads=1`):

| Test | Result |
|---|---|
| 3,000 facets each seeing about 40 others, albedo and emissivity of their own, at 1, 2 and 5 bounces and to the end | the GPU within 3e-7 of the CPU's largest flux, scattered and thermal |
| An isothermal closed box at emissivity 1, 0.9, 0.5 and 0.2 | `e sigma T^4` within 1e-7, in 9, 9, 26 and 81 passes |
| What the CPU refuses | refused, the mirrors facing each other before anything is sent |

## Where it is

| | |
|---|---|
| `src/tpm/view_factor.rs` | `exact`, `exact_polygons`, the clipping, the two integrals, the tests |
| `src/tpm/exchange.rs` | `ViewFactors`, `Bounces`, `absorbed`, the binding, the tests |
| `kalast/tpm/exchange.py`, `.pyi` | `kalast.tpm.exchange` |
| `src/tpm/gpu_exchange.rs`, `shaders/tpm_exchange.wgsl`, `src/py/tpm/gpu.rs` | `GpuExchange` and its binding |
| `tests/test_tpm_gpu_exchange.py` | the GPU against the CPU from Python |
| `tests/test_tpm_exchange.py` | the binding against `kalast.tpm.heating` and the isothermal box |
| `src/py/mesh.rs`, `src/py/mod.rs` | `kalast._rs.mesh.view_factor_exact` |
| `kalast/mesh.py`, `kalast/mesh.pyi` | the export and its stub |
| `tests/test_view_factor_exact.py` | the binding against the closed forms, in the float32 a script passes |
| `docs/API.md`, `CHANGELOG.md` | the function for users |

## Not part of kalast, but made with it the same night

For the PhD progress meeting of 23 November, figures of Didymos and Dimorphos on DART's 10k shape models were made
with the new stepper (`tpm::stepper`, Crank-Nicolson), the exact lit fractions (`shadowing.lit_fractions`) and the GPU
hemicube: the diurnal curve against thermal inertia, the view factor between the two bodies, mutual heating at three
places of the orbit, one orbit with both eclipses, the temperature at two sites through the eclipses, light curves in
the visible and at 10 micrometres. Scripts and pictures are in
`~/projects/phd-manuscript/notes/2026-11-23_progress_meeting/figures/`. They are prototypes of the paper's figures
F3, F7, F10, F11, F12 and F13: the paper's own will come from the exchange in the core, with the heating fed back
into the temperatures, and from the real geometry.

What the stepper made possible there: 150 orbits of Dimorphos (68,000 steps of 90 s) for two bodies of 10,000 facets
with 51 and 101 layers took 134 s on the CPU from Python. The exact lit fractions of the two bodies together cost
1.1 s a call, which is what limits a run with eclipses (two orbits: 11 minutes).

Two mistakes of the scripts worth remembering for the paper's own: switching the shadows on in the middle of an
eclipse leaves the eclipsed body too warm for the rest of that eclipse (start away from any event, and keep the second
orbit); and a point of Didymos can stay in Dimorphos's shadow for tens of minutes, because the surface and the shadow
move the same way, so the comparison "with and without the eclipse" is the same facet one rotation earlier, not a
column without shadows.

## State of the plan's engine items

| Item | State |
|---|---|
| E1, E4 (solar constant), E9, E10, E11, E13 (layered ground on the GPU) | done (18 to 22 November) |
| E8 exact view factors | the geometry done on the CPU (this note); visibility with the exchange |
| E2 radiative exchange | done on the CPU and on the GPU (this note): every facet of every body in one system, reflections to the end; the stepping loop on the GPU without the host is next |
| E6 roughness in the boundary condition | open |
| E3 lit fractions and the exchange reaching the GPU model | open |
| E12 the Python modules as wrappers | open |
| E7 thermal emission in the renderer | open (Paper II) |
