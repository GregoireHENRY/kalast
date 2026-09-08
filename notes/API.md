# Python API reference

Everything a script touches outside `app.simulation.config`, which has its own reference
in `CONFIG.md`. Undated in the filename because it is a living document — add
to it whenever something is exposed to Python.

Editor completion comes from the generated stubs in `kalast/**.pyi`; run
`python tools/gen_stubs.py` after changing any `#[pyclass]`, and
`python tests/test_stubs.py` to check they are current.

**Annotate callback parameters, or completion stops at the callback
boundary.** A stub can say what `App` has, but nothing tells an editor what
gets *passed* to a plain `def` — the assignment `app.before_render = f` is
checked against the declared type, it does not flow back into `f`'s
parameters. So `app.` inside an unannotated `def before_render(app, dt)`
offers nothing, no matter how complete the stubs are:

```python
from kalast.app import App

def before_render(sim: Simulation, dt: float) -> None:
    app.simulation.config.        # completes only because of the `: App`
```

All the examples are written this way.

Defined in `src/py/app/`. Rust types map to Python as `bool` → `bool`,
`u32`/`usize` → `int`, `f32`/`Float` → `float`, `String` → `str`, `Vec3`/`Mat4`
→ `numpy` arrays.

```python
app = kalast.app.App()
app.simulation.config...                 # see CONFIG.md
app.simulation.load_mesh(...)
app.before_render = before_render
app.start()                   # blocks until the window closes
```

## `App`

| | |
|---|---|
| `app.config` | the **application**: window size now, panels and colours later |
| `app.simulation` | the simulation: bodies, camera, sun, state, HUDs |
| `app.simulation.config` | the **simulation's** settings — `CONFIG.md` |
| `app.log(line)` | append a line to the editor's log panel |
| `app.start_editor()` | open the editor shell; **blocks** |
| `app.before_render` | callback run before the frame is drawn |
| `app.after_render` | callback run after it is drawn |
| `app.tick` | alias for `before_render` |
| `app.start()` | creates the window and runs the loop; **blocks** |
| `app.step()` | draws one frame; `False` once the window has closed |
| `app.close()` | asks the window to close; acts on the next `step()` |
| `app.running` | whether the window is still open |

Two configs, and `width`/`height` exist on both without meaning the same
thing: `app.config.width` is the OS window, `app.simulation.config.width` is
the image inside it and defaults to following the window. See the top of
`CONFIG.md`.

There are two ways to run: `start()`, which owns the loop and calls back into
the script, and `step()`, which hands the loop to the script. See **Driving
the loop yourself** below.

Both callbacks take **`(app, dt)`** and are optional. `dt` is the **wall-clock
time since the last frame**, in seconds — it is `(now - last).as_secs_f64()`
at `src/app/mod.rs:353`, not a simulation step, so integrating physics with it
ties the answer to the frame rate. Step the physics on `sim.state.iteration`
instead.

```python
def before_render(sim: Simulation, dt: float) -> None:
    sim = app.simulation          # the scene
    app.simulation.config.colorbar = True    # settings, changeable per frame
```

They receive the **simulation**, which carries its own config, so a frame can
place a body and change a setting through one handle.

This is the second answer to that question, and the first was not wrong at the
time. While `App` meant "the renderer and the scene", `config` and
`simulation` really were siblings and a callback needed both, so it got the
app. `App` now means the *application* — the window, the editor, the loop, the
script runner — and settings belong to the thing they configure, not to the
shell around it. Hence `app.simulation.config`, and a callback that takes the
simulation alone.

**`start()` blocks until the window closes**, so everything else is set before
it, and everything per-frame happens inside the callbacks.

### Which callback to use

| | when it runs | for |
|---|---|---|
| `before_render` | before the frame is drawn | placing bodies, sun and camera; *requesting* GPU results |
| `after_render` | after it is drawn | *reading* GPU results for that frame |

The split exists because a GPU result only exists once the frame has been
drawn. Request in `before_render`, read in `after_render` — reading in
`before_render` gives you the **previous** frame's answer, silently.

Both see the same `state.iteration`: the counter advances only once both have
run, so a loop deriving an epoch from it cannot see two different times within
one frame.

**Neither runs while paused** (`P`), so a script needs no `is_paused` check of
its own — it simply is not called. Heavy CPU work in either blocks the render
loop.

`app.simulation.config` works inside a callback: it is a handle held beside the app, not
fetched through it, so it does not hit the borrow `start()` holds for the
whole run loop. Nearly every option is live now, including the ones baked into
GPU resources — each frame diffs the config against what the window was built
with and rebuilds only what changed. See the legend at the top of `CONFIG.md`
for which cost what.

## The editor

`app.start_editor()` opens the renderer as one panel of a layout, in the shape
of Blender or Unity: a toolbar, the viewport, a config panel and a log.
`python -m kalast` opens it on an empty scene, or with meshes named on the
command line.

**It does not change how scripts run.** `start()` and `step()` draw the scene
straight to the swapchain exactly as before; the editor is a second entry
point, not a mode the other two acquired. A script run from a terminal is
unaffected.

What differs inside is only where the scene lands. It has always been rendered
into `render_texture` and blitted to the swapchain at the end — the editor
skips the blit and lets egui sample that texture into the centre of the
layout instead. `render(None, …)` is the call for that, and it is the same
path an occluded window already takes: a full frame minus the blit and the
present.

### Rust examples

The editor opens `.rs` as well as `.py`, but it cannot host one: a Rust
example is a separate program that links kalast as a library, so it is
compiled and launched and opens a window of its own.

The Script panel grows a row for one — a `debug`/`release` choice and a
`compile` button. Until a binary exists for the selected profile, **Play**,
`Restart` and `Step` are all grey: there is nothing to run and nothing in this
window to step. Once it is compiled, Play launches it.

**Play hands over.** The example carries the editor UI, and the editor that
launched it closes, so there is one kalast window at a time: what opens is the
same thing that closed, with a scene in it. Its toolbar controls *its*
simulation — Play pauses and resumes, Step steps, `Restart` does not apply
because the program is the run. To edit and recompile, run `python -m kalast
<file>.rs` again.

It has to be a second process: an example has its own `main`, links kalast as
a library, and creates its own window and event loop, so it cannot be hosted
in the editor's window the way a Python script is. Handing over is as close to
one window as that allows. The launched example writes to the terminal rather
than the editor's log pipe -- it outlives the editor, and a child holding the
write end of a pipe nobody reads takes a `SIGPIPE` on its next line.

That is switched on by `KALAST_EDITOR=1`, which the launcher sets and nothing
else does, so `cargo run --example ...` from a terminal is exactly as it was.
It also raises the default window to 2000×1300, since four panels in an
800×600 window leave the scene in a corner; an example that sets its own size
still wins.

cargo and the example both inherit this process's redirected stdout, so their
output arrives in the Log beside everything else, and the command is echoed
there as it was run:

```
$ cargo build --color=never --example crater_step --release
built target/release/examples/crater_step
$ target/release/examples/crater_step
```

Examples are named explicitly in `Cargo.toml` rather than auto-discovered,
because each sits beside a Python script of the same name in a directory
cargo does not look into. That table is also what maps an opened file back to
the `--example` that builds it, so a `.rs` with no entry gets a message
saying to add one:

```toml
[[example]]
name = "crater_step"
path = "examples/crater_self_shadow/step.rs"
```

Release by default, because a debug build of this renderer is 2-15x slower
and an example run for its numbers wants the fast one.

### What the panel reaches

The editor is a front end for the API in this document, and it does not cover
all of it. Roughly six of every seven members that a panel could sensibly
carry have a widget; what follows is the rest, and why.

**The config is complete by construction.** Its panel is generated from the
Rust struct and guarded by a test, so an option cannot be added without a
widget appearing. See the top of `CONFIG.md`.

**The simulation is covered except for one cluster.** State, bodies, camera,
Sun, HUDs and export all have sections, and bodies can be added, removed,
reloaded, reshaded and transformed there. Not covered:

| | |
|---|---|
| `facet_shadow`, `facet_id_map`, `hemicube` and their `request_*` | a query is only half of it -- a result needs somewhere to be looked at, and a per-facet array is not a side panel |
| the camera's control mode and aiming helpers | bound to keys already (`T` cycles the mode), and `view_along` belongs to a figure being composed, not to a settings list |
| `flip_facets`, `inward_facing_facets`, `intersect`, `recompute_facets`, `mark_colors_dirty`, `update_all_vertices_colors` | surgery on a mesh, done once when a shape model turns out to be wrong, not while a run is going |

**Some members are not panel-shaped at all** and are excluded from that count:
program entry points (`start`, `start_editor`, `tick`), the editor's own
plumbing (`set_script`, `take_script_request`, `script_runner`, `pointer`,
`ui_size`, `panels_shown`, `flush_output`), matrices derived from state the
panel already shows (`mat`, `view_proj`, `right`), and the bulk per-vertex
arrays -- 3.1M rows is not something to put in a side panel.

### Who wins, the panel or the script

They write the same fields, and the script writes last: a callback that
assigns something every iteration owns it, and a panel edit lasts until the
next assignment. That is not a race to be fixed -- it is what a script
assigning every frame means -- but it does mean an edit can look like it was
refused.

Where it bites, and what to do:

| | |
|---|---|
| `hud.text` | `hud.pin` overrides it outright; the HUDs section sets that when you type. The only field with an override |
| `body.mat` | an edit lasts one frame against a script that places bodies per iteration. Fine for a scene placed once |
| `camera.pos`/`dir`/`up` | same, and the same for a camera driven from SPICE |
| `config.*` | no contention in practice: scripts set these once at the top |

Pausing stops callbacks, so an edit holds -- **except against a driven
script**, whose `while app.step():` keeps running while paused. Pausing stops
the iteration counter, not a loop the script owns.

### Driving the editor from a script

The editor's own loop lives in `kalast/__main__.py` rather than inside
`start_editor()`, because a script has to run *between* frames — one with its
own `while app.step():` cannot run inside the frame that is drawing it. These
are what that loop is built from, and what a custom launcher would use:

| | |
|---|---|
| `set_script(path, source)` | load a script into the panel without running it |
| `run_script()` | Play: build the scene and start |
| `restart_script()` | Restart: rebuild and hold at the start |
| `open_script()` | read the file named in the panel's path field |
| `take_script_request()` | `(path, source, paused)` when a button asked for a run, else `None`; clears it |
| `script_requested` | the same, as a peek that does not clear |
| `script_runner` | the callable the launcher installs to execute a script |
| `script_ran` | whether what is on screen came from the current source |
| `drawn_iteration` | the iteration the frame on screen was drawn for — `{drawn}` in a template |
| `log` | the captured stdout/stderr the Log panel shows |
| `flush_output` | push buffered output into it; `atexit` calls this |
| `pointer` | where the UI last saw the pointer, in egui points, or `None` |
| `ui_size` | the size that is measured against |
| `panels_shown` | `(top, bottom, left, right)` — all four normally, only the summoned ones in `focus` |

`pointer` is `None` for an unfocused window on macOS, which delivers
mouse-moved events only to the front application.

### Two sizes, not one

`Window::render_size` is the *scene* target; `surface_config` stays the
window. They are equal for every terminal run and differ in the editor, where
the viewport is a panel:

| | follows |
|---|---|
| aspect ratio, frustum fits, axis tick projection | `render_size` |
| where the colour bar sits, what an exported frame measures | `render_size` |
| the swapchain, the UI drawn on it, cursor centring | `surface_config` |

So an exported frame in the editor measures the viewport, not the window —
which is the check that the layout is real rather than a full-window render
with panels painted over it. At a 1280×800 window on a 2× display the viewport
came out 680×514: 640 − 300 points of config panel, 400 − 120 of log − 23 of
toolbar.

The viewport is sized from the *previous* frame's layout, because the scene
must be rendered before egui runs. On a resize the image is one frame stale,
which is invisible; the alternative is a blank frame at every new size.

## Driving the loop yourself

`app.step()` draws one frame and returns `False` once the window has closed,
so the loop can stay in the script. **`step()` goes in the middle of the body,
not in the `while` line:**

```python
app = App()
app.simulation.load_mesh(path=..., mat=numpy.eye(4), flatten=True)
sim = app.simulation

while app.running:
    it = sim.state.iteration

    sim.bodies[0].mat = pose(et0 + it * dt)     # the before_render half
    sim.request_facet_shadow(0)

    if not app.step():
        break

    lit = sim.facet_shadow(0)                   # the after_render half
    if it >= 2000:
        app.close()
```

`examples/crater_self_shadow/step.py` is a complete one — the same scene as
that example's `main.py`, run the other way round — on `res/` data only.

**Where the code goes.** Work written *before* `step()` is what
`before_render` did — it lands in the frame about to be drawn. Work written
*after* it is what `after_render` did: that frame has rendered, so
`facet_shadow()`, `facet_id_map()` and `hemicube()` answer for the scene just
drawn.

**`while app.step():` is wrong**, and wrong silently. It puts every line
*after* the draw, so the pose you set applies to the next frame while the
result you read describes the previous one — the two are a frame apart and
nothing says so. Measured on the crater with a sun alternating between two
elevations, against the callback path as ground truth:

| | sun set this pass | `facet_shadow` read this pass |
|---|---|---|
| `before_render`/`after_render` | `y = 20` | `lit = 0.741` |
| `step()` in the middle | `y = 20` | `lit = 0.741` — matches |
| `step()` in the `while` line | `y = 20` | `lit = 1.000` — a frame late |

`while app.running:` with `if not app.step(): break` is the shape that
reproduces the callbacks exactly. `while True:` does not work either: once the
window closes `step()` returns immediately without drawing, `state.iteration`
stops advancing, and a loop keyed on it spins forever.

**Both modes work together.** Callbacks still run inside the frame if set, so
nothing existing changes. Pick one per script; there is no reason to mix.

### What it does not do

- **A paused frame still draws.** `step()` returns `True` as usual — the
  window has to stay responsive to the key that unpauses it — but
  `state.iteration` does not advance and the callbacks do not run. A driven
  loop that should also idle when paused must check `sim.state.is_paused`
  itself.
- **Not usable after `start()`.** A platform event loop cannot be created
  twice in one process and `start()` consumes it. `step()` afterwards reports
  the app as stopped rather than panicking.
- **Not on the web or iOS.** It is `winit`'s `pump_app_events`, unsupported
  there. macOS, Windows, X11 and Wayland are fine.

### What it costs

A little more than `start()`, consistently, and not by much.

400 frames a run, release, `vsync = False`, on the 2048-facet crater, the two
modes run back to back so each pair meets the same machine conditions. Medians
of per-frame time, the quiet pairs:

| `start()` | `step()` | |
|---|---|---|
| 0.556 ms | 0.584 ms | +5 % |
| 0.489 ms | 0.537 ms | +10 % |
| 0.471 ms | 0.505 ms | +7 % |
| 0.519 ms | 0.681 ms | +31 % |

`step()` was the slower of every pair. Under load both inflate and the gap
widens (1.39 → 2.11 ms in one), so read the *pairs*, not the absolute numbers:
unpaired, `start()` alone ranged 0.33–8.30 ms on this machine, which says more
about the machine than about either mode. The window was not frontmost, which
per `CLAUDE.md` makes any figure here a lower bound.

The overhead is what winit documents for `pump_app_events` on macOS, where
pumping stops and restarts the `NSApplication` rather than polling.

Irrelevant for interactive work. Worth knowing for a long export run, where
`start()` stays the cheaper way to spend a few hours.

Rendering still happens inside winit's handler either way — the caller's code
runs *between* frames, never inside one. That is what `pump_app_events`
requires: macOS drives drawing from `drawRect` and expects it finished before
the callback returns.

## `sim.state`

| | |
|---|---|
| `iteration` | frames advanced so far; readable and writable |
| `is_paused` | `P` toggles it; readable and writable |
| `pause_at` | `int` or `None` — stop at this iteration |
| `toggle_pause()` | flips `is_paused`, returns the new value |

`pause_at` is also what `{nit}` reads in a HUD template, since it is the only
thing that tells the engine how long a run is meant to be.

## `sim.huds`

The live HUD list — **the same objects as `app.simulation.config.huds`**, not copies.
Declare them once at setup, edit them per frame:

```python
app.simulation.config.huds = [kalast.app.Hud("{it}/{nit}"), kalast.app.Hud("")]

def before_render(sim: Simulation, dt: float) -> None:
    app.simulation.huds[1].text = f"epoch {utc}"
```

Text written here is still a template. A HUD left untouched keeps its text.
See `CONFIG.md` for the placeholders and the font.

Each `Hud` carries:

| | |
|---|---|
| `text` | the template |
| `pin` | used instead of `text` while set — see below |
| `anchor` | which corner `x`/`y` are measured from |
| `x`, `y` | inset from the anchor, in pixels |
| `size` | font size in pixels |
| `color` | `(r, g, b, a)`, each 0–1 |

All of them are editable in the editor's HUDs section, which can also add and
remove HUDs.

**A callback that assigns `text` every iteration owns it**, and an edit made
anywhere else is gone by the next frame. Pausing does not help a *driven*
script: pausing stops the iteration counter, not a `while` loop the script
owns, so `sim.huds[0].text = ...` keeps firing.

`hud.pin` is where an edit can stand. While it is set it is used in place of
`text`, whatever `text` says, and the script goes on writing `text`
harmlessly. Typing in the editor's HUDs section sets it; its release button,
or `hud.pin = None`, hands the HUD back.

```python
app.simulation.huds[0].pin = "lit {paused}"   # mine now
app.simulation.huds[0].pin = None             # the script's again
```

## `sim.bodies`

A list, in load order. Each `Body` has:

| | |
|---|---|
| `mat` | 4×4 model matrix as a numpy array — position and orientation |
| `mesh` | the `Mesh`, or `None` |

```python
sim.bodies[0].mat[:3, :3] = spice.pxform("IAU_MARS", "HERA_TIRI", et)
sim.bodies[0].mat[:3, 3] = position_km
```

`bodies` hands back fresh wrappers each time, so removing from the list it
returns removes from a copy. `sim.remove_body(i)` is the real thing:

```python
sim.remove_body(1)          # IndexError if there is no body 1
```

The bodies after it shift down, so an index held across the call means a
different body — `camera.anchor_body` included, which follows by index and
will quietly follow the neighbour.

`sim.reset()` empties the scene entirely: bodies, HUDs, the iteration counter
and any pending GPU request. The config survives, deliberately — it is what a
script sets on its way through *and* what the editor's panel edits by hand.
The editor calls it before running a script, because `load_mesh` appends and a
script run twice would otherwise load its meshes twice.

## `sim.camera` and `sim.sun`

Both are an `Eye`.

| | |
|---|---|
| `pos`, `dir`, `up`, `up_world` | placement; `dir` and `up` must be unit vectors |
| `anchor` | the point the arcball orbits |
| `anchor_body` | body index to track, or `None` — an anchor that follows a body instead of snapshotting where it was |
| `projection` | see below |
| `look_anchor()` | point `dir` at `anchor` |
| `set_target(p)` | set `anchor` to `p` *and* look at it |
| `target()`, `right()`, `distance_anchor()` | derived quantities |
| `set_control_arcball()` / `set_control_wasd()` / `set_control_none()` | input mode |
| `control_toggle()`, `is_control_*()` | same, from a script |
| `view_along(axis, orthographic=True)` | look down `+x`/`-x`/`+y`/`-y`/`+z`/`-z` |

`view_along` is for the plane views a figure wants. It switches to an
**orthographic** projection by default, because a perspective view of a plane
is not measurable — the near and far sides of a crater are at different
scales, which is why published figures of this kind are orthographic. Pass
`orthographic=False` to keep perspective and just take the viewpoint.

Framing is left to the automatic frustum fit, so the eye's distance is not
something to tune. It clears `anchor_body` if set: a plane view is about the
scene, not one body. It does nothing if no geometry is loaded, so call it
after the meshes.

**The Sun ignores `dir` and `anchor`.** Since per-body shadow layers landed,
each layer aims itself from `sun.pos` at the body it covers, so `sun.pos`
alone determines the lighting. Older scripts calling `sun.look_anchor()` still
run; the call simply has no effect.

Use `set_control_none()` for a scripted render whose camera is placed from
SPICE, so a stray drag cannot move an instrument pointing. Note `T` still
switches out of it — see `CONTROLS.md`.

### Control mode

Which mouse and key bindings drive an `Eye`. `CONTROLS.md` has the bindings
themselves.

| | |
|---|---|
| `set_control_arcball()` | orbit the anchor — the default |
| `set_control_wasd()` | fly |
| `set_control_none()` | frozen, for a camera a script places every frame |
| `control_toggle()` | cycle |
| `is_control_arcball()`, `is_control_wasd()`, `is_control_none()` | which it is |

And the aiming helpers, which move `pos`/`dir`/`up` together rather than one
at a time:

| | |
|---|---|
| `look_anchor()` | point `dir` at `anchor`, leaving `pos` alone. No effect on the Sun, whose layers aim themselves from `pos` |
| `set_target(p)` | set `anchor` to a point *and* look at it |
| `view_along(axis, orthographic=True)` | look straight down an axis at the whole scene, the way a plot does. `"z"` and `"xy"` are the same call. Orthographic by default, because a profile read off a perspective view is not measurable. Needs geometry loaded |
| `fix_up()` | re-orthogonalise `up` against `dir`. An `up` parallel to `dir` normalises a zero vector, and the NaN freezes the camera for good |
| `up_world` | the reference up an arcball keeps the camera aligned to |

Read-only, derived from the above:

| | |
|---|---|
| `target()` | `pos + dir` |
| `right()` | unit vector pointing right in the image plane |
| `distance_anchor()` | how far the eye is from `anchor` |
| `mat()`, `lookto()`, `view_proj()` | the rotation, the view matrix, and view × projection |

These are **methods, not properties** — `cam.target()`, not `cam.target`.
`pos`, `dir`, `up`, `anchor`, `anchor_body` and `up_world` are properties.

### `Eye.projection`

| | |
|---|---|
| `fovy`, `near`, `far`, `side` | `None` means *fitted automatically each frame* |
| `resolved_near`, `resolved_far`, `resolved_side` | what the fit actually chose |
| `set_perspective()`, `set_orthographic()` | projection mode; `is_perspective()`, `is_orthographic()` read it |

Assigning any of `near`/`far`/`side` **pins** it and defeats the automatic fit
for that plane; assigning `None` restores automatic. `fovy` is never automatic
— it is a real instrument property, not a scene-derived one.

## Meshes

```python
sim.load_mesh(path=..., mat=numpy.eye(4), flatten=True, shadow_path=None)
sim.add_mesh(mesh, mat=None)
```

`flatten=True` gives each facet its own vertices, which is what makes
per-facet data and the wireframe overlay work.

`shadow_path` names a coarser mesh to render into the shadow map in place of
`path`. The shadow map only decides which fragments are lit, so a coarser
occluder buys performance without touching per-facet science data — unlike
loading a coarser `path`, which would invalidate anything facet-indexed.

### `sim.rebuild_meshes()` — after changing a mesh's *shape*

The GPU buffers are built from the meshes once and thereafter only the
transforms are re-uploaded. So a change of **placement** needs nothing —
`body.mat` goes up every frame — but a change of **shape** is invisible until
the buffers are rebuilt, with no error to say so:

```python
mesh.smoothen()             # or flatten(), or replacing vertices
sim.rebuild_meshes()        # ...or the render keeps the old geometry
```

`sim.remove_body` does it for you, since removing from the middle changes what
every later index means. Colours have their own, cheaper route in
`mesh.mark_colors_dirty()`.

### What a `Mesh` carries

Per-vertex arrays, one row per vertex, in the order the buffers hold them —
which after `flatten=True` is three unshared rows per facet:

| | |
|---|---|
| `positions`, `normals` | `(n, 3)` |
| `textures` | `(n, 2)` texture coordinates |
| `tangents`, `bitangents` | `(n, 3)`, for normal mapping |
| `colors` | `(n, 3)` |
| `color_modes` | `(n,)`, per-vertex selector for what the shader outputs |
| `vertices`, `facets` | views over the rows themselves, as `Vertex` / `Facet` objects |
| `indices` | `(3f,)` triangle indices — after `flatten` these are `0..3f`, one per row |
| `values` | `(f,)` per-facet scalar to colour by; see below |
| `material_id` | index into the model's materials, or `None` |

And the operations on one:

| | |
|---|---|
| `is_flat()` | whether each facet owns its vertices — a method, not a property |
| `flatten()`, `smoothen()` | switch between that and shared corners. Follow with `sim.rebuild_meshes()` |
| `recompute_facets()` | recompute centres, normals and areas after moving vertices |
| `mark_colors_dirty()` | re-upload colours next frame, after writing `colors` in place |
| `update_all_vertices_colors(mode, color)` | set every vertex to one colour and mode |
| `get_facet_positions(i)` | the three corners of one facet |
| `get_facet_normals(i)`, `get_facet_colors(i)` | the same three, other attributes |
| `get_facet_indices(i)`, `get_facet_vertices(i)` | its indices, and its `Vertex` rows |
| `inward_facing_facets()` | indices whose normal points into the body — a shape-model sanity check |
| `flip_facets(indices)` | reverse their winding; returns how many. Before `start()`, since the buffers are built once |
| `intersect(p, u, exit_first)` | ray cast: `(facet, point)` or `None` |

A facet whose winding is reversed is permanently dark in the thermophysical
model, and a hemicube placed on it reports a self view factor near 1 — which
is what `inward_facing_facets` is for. It assumes a roughly star-shaped body,
so read the result rather than trusting it.

### `mesh.values` — colouring facets from data

One float per facet, in `Mesh.facets` order:

```python
sim.bodies[0].mesh.values = temperatures      # numpy array, one per facet
app.simulation.config.color_mode = 1                     # unlit: this *is* the data map
app.simulation.config.colormap = "inferno"
app.simulation.config.value_min, app.simulation.config.value_max = 90.0, 290.0
```

Assigning marks the mesh dirty, so the change reaches the GPU on the next
frame with no separate call.

**Pin `value_min`/`value_max` for anything comparative.** Left automatic the
range refits every frame, so two images of the same scene sit on different
colour scales and the difference between them reads as physics rather than as
bookkeeping.

`color_mode = 1` is what shows them: the unlit mode *is* the data map for any
mesh carrying values, and a mesh without values falls back to its vertex
colours. There is no separate switch, because unlit is what a quantitative
figure wants anyway — shading a data map makes one value read as two colours.
The colour bar follows the same setting; see `CONFIG.md`.

## Frame export

| | |
|---|---|
| `sim.export` | `bool`, export every frame |
| `sim.export_once()` | export the next frame only |
| `sim.toggle_export()` | flip continuous export |

Destination and behaviour are config: `export_dir`, `export_sync`,
`export_max_queued`, `export_hud`. **Redirect `export_dir` for any test run** —
the default `out/frames` is shared, and two exporters pointed at one directory
race. See CLAUDE.md.

## GPU results

All three follow the same rule: **request in `before_render`, read in
`after_render`.**

### Per-facet occlusion — feeds the TPM

```python
sim.request_facet_shadow(body)      # before_render
frac = sim.facet_shadow(body)       # after_render -> array or None
```

One entry per facet in `Mesh.facets` order: `0.0` fully lit, `1.0` fully
shadowed, quarter steps between (4 samples per facet). `1.0 - frac` is the lit
fraction.

Set `config.access_shadow_map = True` to have every body computed every frame
instead of requesting per body.

**This is the TPM's occlusion term, not a rendering detail.** It is read back
from the shadow map, so it inherits the shadow bias — see the calibration note
in `2026-09-04_shadow_fixes.md` before trusting absolute values.

### Facet index map — feeds the FITS products

```python
sim.request_facet_id()              # before_render
ids, offsets = sim.facet_id_map()   # after_render -> tuple or None
```

`ids` is `(height, width)` `uint32`: 0 where nothing was drawn, otherwise
`1 + offsets[body] + facet`. For body `b`:

```python
mask  = (ids > offsets[b]) & (ids <= offsets[b] + n_facets_b)
facet = ids[mask] - offsets[b] - 1
```

Depth is resolved by the rasteriser, so a facet missing from the map is one
the camera genuinely cannot see. **Only flattened meshes are drawn**, since
the facet index comes from the vertex index.

There is deliberately no config flag to leave this on: it renders the scene a
second time and blocks on a readback, so it is for the frames a data product
comes from, not for every frame of a long run.

### View factors — a precompute, not a per-frame query

```python
sim.request_hemicube(body=0, facets=None, resolution=128, batch=64)
vf, offsets = sim.hemicube()
```

`vf` has shape `(len(facets), n_total)` over a facet index space shared by
every loaded body, so one array carries self *and* mutual view factors;
`vf[:, offsets[b]:offsets[b] + n_b]` is body `b`'s block. Rows sum to at most
1, the shortfall being what is radiated to space. Occlusion is included, by
the other body as well as by the body's own terrain.

A full 10,000-facet matrix is minutes of GPU work and 400 MB dense. For a
rigid body the self view factors are fixed in the body frame, so compute once
per shape model and reuse. Delta form factors close to unity as
`1/resolution²`, reaching 3e-5 at 128.

## `sim.update()`

Advances `state.iteration`. The app calls it once per frame; a script does not
normally need it.
