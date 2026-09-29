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

## Open

- `heat_conduction` is the uniform stencil. The Hera scripts' geometric grids
  need `conduction_1d_nonuniform`'s form over every facet before they can
  leave `routine.step_*`.
- One thread. At 5120 facets it is 8 % of a drawn frame; a large mesh on the
  CPU would want the rows split across threads, or the GPU TPM.
