# 2026-09-29 — a thermophysical model over every facet at once, for a short script

Asked for: `scripts/sphere/tpm.py`, the user's, mirroring the diffuse-lighting
`scripts/sphere/main.py` -- the sphere spinning, the Sun at 1 AU, thermal
properties, the skin depth and the column's depth, a time scale of a few
hundred spins, and in the loop "the solar BC and bottom adiabatic, then heat
conduction". `examples/hera_didymos/tpm.py` was the reference, and the user
found it "way too complex and unreadable and unmaintainable for a user".

Most of that script is the physics written again in Python, because the
Rust core had it for one column only: `newton_method` and `conduction_1d`
take a facet's column, and a Python loop over 10k facets was the 6.6 ms a
step that `kalast/tpm/routine.py`'s numpy versions (`step_surface_newton`,
`step_conduction`) were written to avoid -- the insolation with it, `einsum`
and an explicit clamp. Rule 14 forbids that, and it is what a user has to read.

## What the core has now

In `tpm::core`, beside the per-column forms they are built from, each step
over every facet at once, in place on one array:

| | |
|---|---|
| `columns(layers, facets, t)` | the temperatures, `(layers, facets)`: `t[0]` the surface, `t[-1]` the bottom, `t[:, i]` the ground under facet `i` |
| `solar_bc(t, dau, cosi, prop, dz)` | `radiation_sun` and `newton_method` on each column; an error names the first facet that did not converge |
| `bottom_adiabatic(t)` | the last layer at the temperature of the one above |
| `heat_conduction(t, prop, dt, dz)` | `conduction_1d` on each column; refuses `D dt / dz^2 > 1/2` (a few ulps allowed, for a `dt` taken at the limit) and a diffusivity never computed |

Layer-major, so the surface is one contiguous row -- `mesh.values = t[0]`
as it is -- and each step a sweep of whole rows (`Zip` over three rows, the
layer above kept as it was before its own step). They take `Properties`
rather than `se`, `k` and `2 dz`, which the script would otherwise compute
and pass in the right order. In Python, `t` must be kalast's float type to be
written in place; given another, the message says so and names `columns`,
where it used to be "'ndarray' object is not an instance of 'ndarray'".
`cosi` takes either width or a list.

And `Simulation::facet_incidence(body)` (`sim.facet_incidence`): `max(0,
cos i)` per facet from the body's pose and the Sun's position as set, no
frame and no shadow map -- the geometry `facet_illumination` already did,
which is now built on it. The script needs no normals, no frames, no
transposed rotation.

## Checked

- `every_facet_at_once_is_each_column_alone`: five columns, a cosine each,
  against `newton_method` and `conduction_1d` column by column.
- `a_column_in_constant_sunlight_settles_at_radiative_balance`: every layer
  within 0.05 K of `(S (1 - A) / e sigma)^(1/4)`.
- `a_spinning_column_gives_back_what_it_absorbs`: the equator of the script's
  sphere (TI 200, 6 h, 8 layers per skin depth, one `skin_depth_2pi` deep),
  40 spins: over the last, emitted and absorbed agree to 0.0002 %. The bottom
  swings 0.64 K, where the wave e^-2pi of the surface's 156 K, doubled by the
  wall, is 0.58 K -- the first bound, 0.5 K, was wrong, not the model.
- Resolution, the equator and 60 degrees against 32 layers per skin depth,
  `D dt / dz^2` at 1/2:

  | layers per skin depth | layers | steps a spin | worst error |
  |---|---|---|---|
  | 4 | 26 | 101 | 2.13 K |
  | 6 | 39 | 227 | 1.07 K |
  | 8 | 51 | 403 | 0.62 K |
  | 10 | 64 | 629 | 0.40 K |
  | 16 | 102 | 1609 | 0.13 K |

  8 at 1/6 instead of 1/2 is worse, 0.90 K. The script takes 8.
- The sphere (`res/ico4.obj`, 5120 facets, 51 layers, dt 53.6 s), 300 spins:
  120,900 steps in 102 s with the window drawing each one, 1,185 steps a
  second; the surface 128.6 to 371.0 K, the equator's peak the column's.
- The step alone, 5120 x 51: `solar_bc` 16.6 us, `bottom_adiabatic` 0.6 us,
  `heat_conduction` 47 us -- 64 us, where `routine.step_surface_newton` and
  `step_conduction` take 51 and 782 us on the same machine, 13 times as long
  (in float64, the numpy path's). In the drawn run above, 64 us of a
  frame's 844.

## Opened, a script that gives its run's length played straight through

The script says how long it runs as the engine is told it,
`state.pause_after_iteration = n_spins * steps_per_spin - 1`, which is also
what `{nit}` reads. The UI app holds an opened script at iteration 0 through
the same field -- set before the script runs -- so the script's length
replaced the hold and it played through. Step and `K` wrote it too, so one
Step lost the length and the run no longer stopped.

`State::hold_after_iteration` is the app's: Step, `K`, the iteration an
opened script or a paused Restart shows, spent once it fires. The run's length
stays the script's. One exception keeps its old meaning: `None`
(`State::set_pause_after_iteration`, both Python spellings) drops the hold as
well, "never pause on your own" -- `examples/landmark_tracking/main.py` says
it to run straight, because a resume with `P` after the hold skipped an
iteration (open since 24 September), and `test_editor_startup`'s driven script
does too. `a_run_length_and_the_apps_holds_keep_out_of_each_other`; and live,
a background UI app opening a 2-spin copy of the script: held at 1, the
length 805; Play; stopped at 806, the length 805.

## On the way

- `kalast/tpm/core.pyi`: `kalast.tpm.core` had no stub, so the editor knew
  none of its functions. `tools/gen_stubs.py` now reads a signature inside
  `#[cfg_attr(feature = "python", pyo3(...))]`: `stability_maxdt`'s `s=0.5`
  read as required.
- `tests/test_config_panel.py` looked for a field's `:skip:` by name in the
  whole file: the window's `width`, skipped since the remembered window
  (`2026-09-29_window_place_and_panel_fixes.md`), was checked against the
  wireframe's. Now in the field's own struct.

## The editor's checker, on the script

Reported next, in the bundle's editor: ty marked
`app.simulation.config.data.colormap = "inferno"` as an error, the stub
saying `colormap: numpy.ndarray`. The generator makes a getter and setter
one attribute of the getter's type unless the setter's is known and
different, and `set_colormap` takes `PyAny` -- `object` -- to parse names
and arrays itself. A `:pytype:` on the setter says what it takes, and the
stub has a property whose setter does: `str | numpy.ndarray |
Sequence[Sequence[float]]`. `mesh.values`, the other `PyAny` setter, had the
same narrowing: `numpy.ndarray | Sequence[float]` now.

ty then showed two more on the script: `body.mesh` was `Mesh | None`, so
`bodies[0].mesh.facets` and `.values = ...` were errors -- as in
`examples/cube/color_map.py`, and in the pattern API.md teaches. A body
without a mesh is only `Body::default()` in Rust tests; `load_mesh` and
`add_mesh` always attach one, and no Python checks for `None`. The getter
returns `Mesh` and raises `AttributeError` for a meshless body, so
`getattr(body, "mesh", None)` still asks. The script and `color_map.py`
check clean; over every current example ty finds 127 diagnostics, none
about meshes, values or colormaps.

And the day after, on `solar_bc(temperature, dau, cosi, prop, dz)`: `cosi`
was `ndarray | None`, `sim.facet_incidence` giving `None` for a body that
does not exist -- only ever a wrong index -- and `solar_bc` now saying it
takes an array. It raises `IndexError` instead, and is typed an array.

## The scheme, named where it is hovered

Asked: "what is the TPM method used here? is it finite difference with fixed
step size? this has to be documented in core.heat_conduction() docstring".
It is: explicit finite differences, forward in time and centred in depth
(FTCS), on equal layers and at a fixed step -- first-order in time, second in
depth, stable while `D dt / dz^2 <= 1/2`. The surface is implicit in `T0`
alone, Newton's method to 0.1 K from the step before, the gradient a
second-order one-sided difference; the bottom's zero flux is the first-order
`T[-1] = T[-2]`. Now in the docs of `columns`, `solar_bc`,
`bottom_adiabatic` and `heat_conduction`, Rust and Python both, and in
API.md. The docstring was in the stub all along -- ty's hover returned it
-- but the editor asked for no hover that far down the file: `index_at`
took the line from `y / 15.125` and the character from the galley's 15.0
rows, and on line 81 the two disagreed over two thirds of the row. The
fix is the gutter's (TIMELINE, "the editor's rows by the galley").

## Per facet and per layer: `Ground`

Asked: "thermal properties are global for body unless they are provided per
facet and per depth layer, does the current functions in sphere/tpm.py allow
this?" -- they did not: one `Properties` for every facet and layer, only
`cosi` per facet. Then "ok implement it", on the design offered.

`tpm::core::Ground`: `albedo` and `emissivity` per facet; `conductivity`,
`density` and `heat_capacity` per layer and facet, each of any shape that
broadcasts to `(layers, facets)`. `GroundView` borrows it -- from Rust arrays
or from numpy's -- and `Thermal` is what `solar_bc` and `heat_conduction`
take, `&Properties` or a `Ground`, so both keep their names and the uniform
path is untouched.

The physics. At the surface, each facet's albedo, emissivity and top-layer
conductivity in the same Newton balance. Below, the conservative form,
`rho c_i dT_i/dt = [k_{i+1/2} dT_{i+1/2} - k_{i-1/2} dT_{i-1/2}] / dz^2`, `k`
at a boundary the harmonic mean -- two half-layers in series. A diffusivity
per layer in the uniform stencil, what `conduction_1d`'s `d` array allows,
drops the `dk/dz` term and does not conserve the flow across a boundary.
Stable while `dt (k_{i-1/2} + k_{i+1/2}) / (rho c_i dz^2) <= 1`
(`Ground::stability_maxdt`). Tests: one material steps as its `Properties`;
each facet as its own `Properties` would; over a step the interior gains what
crosses its ends, to 1e-4 (conservation, materials mixed pseudo-randomly);
two materials in series, ends held, settle with one flow at every boundary,
`dT / sum(dz / k)`, and a fluffy layer drops ten times what a conductive one
does; broadcast shapes step to the bit as full arrays; a layered spinning
column gives back what it absorbs; and what does not fit is refused.

In Python the arrays are `Py<PyArray>` held by the class -- a getter returns
the array itself, so `ground.conductivity[:6] = ...` is the array the step
reads, and no view can outlive its memory as one borrowed from a Rust field
would when a setter replaced it. Setters take a number, a list or either
float width, kept at the shape given; a flat `(layers,)` meant per layer is
refused with the `[:, None]` it wants. `tests/test_tpm_ground.py`.

The speed took four tries. Through `Zip` -- the facet index for the error, a
pass per coefficient, broadcast rows read strided -- the step was 616 us at
5120 by 51, twelve times the uniform 52. Slices indexed seven ways kept
bounds checks and a branchy flag: 1.8 ms. Rows copied through ndarray's
element iterator: a nanosecond a value. What stood: one fused pass a layer,
contiguous rows read in place, slices cut to `facets` so the checks go,
the stability as a max and a min, a broadcast value filled once a step --
140 us full, 148 compact, 158 with the surface, against 56 for
`Properties`. `stability_maxdt` 215 us, once.

## Where a column starts: its latitude's effective temperature (2026-09-30)

Asked on the obliquity script: "a better effective temperature that also
consider latitude and obliquity, to have winter pole initialize close to 0
and summer pole to other temperature". The scripts started every column at
`effective_temperature(dau, 0.25, A, e)`, the whole sphere's; the user had
tried 0 K instead.

`effective_temperature`'s `r`, the ratio of the areas receiving and
emitting, is the mean cosine of incidence -- 1/4 over a sphere -- so a
latitude needs only its own: `mean_incidence(lat, dec)`, the daily mean of
`max(cos i, 0)` for ground facing `lat`, the Sun at `dec`,
`(h0 sin lat sin dec + sin h0 cos lat cos dec) / pi` with `cos h0 = -tan lat
tan dec` -- written without the tangents, from `s = sin lat sin dec` and
`c = cos lat cos dec`: polar day where `s >= c`, night where `-s >= c`, which
a pole in f32 (`cos(pi/2)` = -4e-8) takes cleanly. Rust scalar; the Python
binding broadcasts numbers and arrays as numpy does, and returns a number
for numbers (`effective_temperature` too, which moved to a binding of its
own for it). `columns` takes one start a facet, any shape that broadcasts,
as a `Ground`'s arrays -- `node_shape` shared with their setters.

Checked: the closed form against the hour angle summed 20 000 times, polar
day and night included, to 1e-5; over a sphere weighed by area, 1/4 at any
`dec`; a column at latitude 60, the Sun at 25, spun 40 times, radiates as
309.00 K against its start's 308.99. On the 45-degree sphere of
`scripts/sphere/tpm_obliquity.py` (5120 facets, 6 h, Γ 200): the
latitude start puts every column's bottom within 1 K of its settled value
after 18 spins at most (median 8-16 by band); 1/4's leaves the polar night
96 K off after 50 spins and 36 columns near it unsettled at 200; 0 K
leaves 70 cold ones unsettled at 200. A lit column starts 0 to 14 K warm (median 10):
the mean of `T^4` above the mean of `T` a column settles to at depth. The
polar night starts at 0 K and stays there: 1-D columns exchange nothing
sideways and nothing comes from below. `tests/test_tpm_ground.py` has the
bindings.

Then, in one line, as the whole sphere's 1/4 was:
`core.effective_temperature(dau, sim.facet_mean_incidence(0), a, e)`.
`Simulation::facet_mean_incidence` reads each facet's latitude and the Sun's
from the pose as it stands -- the normals by the pose's inverse transpose,
as `facet_incidence` has turned them since the logo's flattened body asked
it to, the axis as the body's points -- and gives `mean_incidence` of them;
`z` the axis unless given. Checked against `facet_incidence` averaged over a
spin, tilted and flattened: 1e-3.

## A graded column: the day and the year in one (2026-09-30)

Asked, of the logo's seasons with each day's sunlight averaged: "that's
good approximation but not correct". Spinning the body needs the day's
wave resolved at the surface -- 8 mm layers at thermal inertia 800 -- and
the year's reached below, 18 m: 2,300 equal layers, 79 s steps, half an
hour a Didymos year. `Ground::graded(prop, facets, dz, depth, ratio)`: three
layers `dz` thick, then each `ratio` times the one above -- 36 to 20 m at
1.2 -- as a `Ground` of layers `dz` apart whose properties carry each
layer's width `w`, the conductivity times `dz / w`, the density times
`w / dz`. The conservative stencil on those is the finite-volume scheme on
the graded cells exactly: the heat capacity `rho c w_i`, and between two
layers the harmonic mean of `k dz / w`, which is `k dz / d`, `d` the mean of
their widths, the distance between their middles. `solar_bc`'s one-sided
gradient reads the top three, kept even. The step is the thinnest layers',
81 s. Nothing new in the stepping: the per-layer `Ground` already was a
non-uniform column, needing only the widths put into it. Checked: the
surface's last day of twenty within 0.5 K of equal layers half as thick
(0.22 K at 1.1, 0.25 K at 1.2); at ratio 1 the uniform `Ground`; what cannot
be graded refused. The logo's run spins 3.31 years, 2.76 million steps, in
about three minutes.

## Open

- `heat_conduction` is the uniform stencil; a graded column is a `Ground`
  (`Ground::graded`). The Hera scripts' geometric grids could move to it
  and leave `routine.step_*`.
- One thread. At 5120 facets it is 8 % of a drawn frame; a large mesh on the
  CPU would want the rows split across threads, or the GPU TPM.
