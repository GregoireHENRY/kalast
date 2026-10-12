# One shadow method, colour modes by name, the console's own names

12 October 2026, M1 Pro.

The user, on the settings after the ray tracing came over: "i dont understand
what reference button does"; "instead of a toggle ray and cascades toggle, you
should present a choice between shadow map, CSM, and rays, and no shadow";
"no shadow would remove the option color mode 3"; "the shadow option should
appears only when color mode is 0"; "color_mode is now either 0 or 1 and could
be renamed". And a console that lost its variables: `a = mesh`, then
`a.facets` was `'float' object has no attribute 'facets'`.

## `shadows.method`

`shadows.rays` (a switch) and `shadows.cascades` (0 for none, else a count)
were two ways of saying which method draws the shadows, and `color_mode = 3`
a third. One field now, `ShadowMethod` in `config.rs`:

| `shadows.method` | |
|---|---|
| `"maps"` | a layer per body, the default |
| `"cascades"` | cascaded maps, `shadows.cascades` of them (1-4, 3 by default) |
| `"rays"` | traced, on a GPU with ray queries; the maps answer without |
| `"none"` | no shadows |

Neither `rays` nor `cascades` had been released (the beta was cut on 6
October, before both), so they changed without a shim. `Shadows::cascades_used`
is the count with `"cascades"` and 0 otherwise, so the code that read the old
count reads the method through it; `per_body_layers` follows. The presets set
`shadows.method` (`Preset::Method`): `"maps"` for the maps' four, `"rays"` for
the rays'.

**"none" draws nothing.** `color_mode = 3` drew every shadow layer and then
set `shadow = 1.0` in the fragment shader. Now no layer is planned
(`n_body_layers = 0` in `Window::update`), the array is put away to 512 as
with the rays (`shadow_side`), the penumbra pass is not wanted, and the
shaders take "no layer and no rays" to mean no shadows (`shadowless()` in
`mesh_shadow.wgsl`): the main pass looks nothing up, walks nothing, and
leaves out the horizon map and the dust's transmission -- everything that
stands between the Sun and a point. The per-facet query answers by facing
alone, 0 for a point facing the Sun and 1 for one facing away, as the maps'
query does for those (`facet_sun` returns 0 there): the TPM takes the
shadows the image shows, none. A reference image is traced with its shadows
whatever the method (`Config::shadows_on`), as it was whatever `rays` said.

## `shading.color_mode` by name

`ColorMode`: `"shaded"` and `"data"`, its `code()` the 0 and 1 the shaders
always took. The user counted two modes once 3 went; there were three -- 2 was
the flat `shading.color`, a silhouette, which nothing in the examples or tests
used. Kept as `"flat"` at first and offered for removal: "flat color mode is
pointless, there is only shaded or data". It went, and `shading.color` with it
-- nothing else read it; the uniform keeps its slot, unwritten, so every
shader's copy of `Globals` still binds, and `config_renames.py` no longer
forwards the old flat name `config.color`.

The numbers were released, and every data example set `color_mode = 1`, so
Python still takes them for a release, with a `DeprecationWarning` naming the
name (`set_color_mode` in `src/py/app/config.rs`, the field `:py_custom:`).
3 sets `shadows.method = "none"` and `"shaded"`; 2 is refused, saying the flat
mode is gone and `"data"` shows each mesh's own colours. A 0 after 3, which in the
old world meant shadows back on, sets `"maps"` again when the method is
`"none"` -- `tests/test_pcf_filters.py` and `test_pcf_relief.py` toggled 3
and 0 to get an image without shadows to compare with, and a script that does
would otherwise keep them off. The examples write the names now.

## The panel

`gen_config_panel.py` learned three things for this:

- a `:when:` of clauses joined by `&&`, each about its own group's field or,
  named first, another group's (`method.uses_maps() && light.sun_as_point ==
  false`) -- a dotted first word is another group only when it is one, so a
  method of the field's own (`method.uses_maps()`) is not taken for one;
- an enum's variants by their own names in a `:when:`
  (`method == ShadowMethod::Rays`), qualified in the generated code;
- an enum's list gives each variant its doc's first paragraph as its hover.

So under the method only its settings show: the maps' resolution, PCF, biases
and levels of detail with the maps or the cascades; `per_body`, `near_layer`
and `cache` with the maps; the count with the cascades; `ray_samples` and
`ray_probe` with the rays; `access_shadow_map` always.

The Shadows topic (`shadows_ui` in `simulation_panel.rs`) shows in the shaded
mode only, as asked -- with one exception the request did not foresee: the
per-facet shadows the thermophysical model reads follow the method whatever
the image shows, and a data map of temperatures is exactly when they are
read. So while `access_shadow_map` is on, the settings stay in the data mode
too, under a line saying they are the per-facet shadows'. Otherwise a line
says shadows show in "Shaded".

## The cascades removed, "quick" made quick

The user, trying the methods: "CSM seems to be slower and less accurate than
shadow mapping. It is supposed to be ok to be less accurate only if it is
much faster to render". Measured -- once the main pass's ray code was out of
the way, which had halved everything on the M1 Pro
(`2026-10-12_ray_tracing_on_metal/`) -- it was slower in both views, frames a
second:

| | maps | none | cascades, 3 x 4096, PCF 2 | cascades, 3 x 1024, PCF 1 | one layer, 2048, PCF 1 | maps, 2048, PCF 0 |
|---|---|---|---|---|---|---|
| the Didymos pair from 1 km, 1600 x 1000 | 685 | 1421 | 479 | 595 | 901 | 868 |
| Dimorphos from 68 m, 3234 x 1774 | 201 | 686 | 127 | 168 | 179 | 331 |

"isnt it possible to improve the CSM method?" Not into something much quicker
here. Up close the maps' 3.5 ms is mostly their lookups -- 3.1 ms of PCF over
5.7 million pixels, the shadow pass 1.15 -- which cascades pay as much; and each
cascade draws every body (3.2 ms of shadow passes at 1024, 6.3 at 4096). What
cascades do in a game's world, putting texels where the camera looks, a layer
per body fitted to what the camera sees of it, with the near layer up close,
does already. What is much quicker is hard edges at 2048. The user chose:
the method gone, and `"quick"` that.

`ShadowMethod::Cascades`, `shadows.cascades`, `cascade_region`, the cascade
choice in `shadow_layer_at` and in `cs_facets`, and `tests/test_cascades.py`
went; `Shadows::per_body_layers` was `per_body` again; the uniform keeps the
count's slot (`_spare`). `"quick"` is a layer per body at 2048, PCF 0, a point
Sun. The presets, measured after it:

| | Didymos pair from 1 km | Dimorphos from 68 m |
|---|---|---|
| `"quick"` | 879 | 339 |
| `"fast"`, nothing moving | 1078 | 239 |
| `"point"`, the defaults | 692 | 204 |
| `"accurate"` | 161 | 66 |

## The reference image's box

It was a checkbox named `enabled` under "reference", its hover starting
"Integrate the image to a reference". It is **render** under "reference
image" (`:label: render`), its hover saying what it is for first: a still
drawn as exactly as kalast can, for a figure or to check a faster setting
against, sharpening every frame until its error is under `error`, then held.
The line under it gives the samples a pixel and the error it stops at, and
says when the GPU has no ray queries, its shadows then the maps'.

## Slow frames and the UI app

"when the HUD has low FPS like 10 or less the UI is unusable cus mouse is
laggy and teleports" -- the user had rays or a reference running. The panels,
the mouse and the scene share one loop, so every frame the panels wait for the
scene's: rays across the disc on the M1 Pro, 0.14-0.4 s.

A preview was tried first (`0c40844`): while the user worked the mouse or the
keyboard, a traced image slower than 1/30 s drawn with the maps by untraced
pipelines, exact again 0.3 s after. It worked -- 2.2 ms a frame against 384 --
and the user did not want it: "i dont like that when moving my mouse the
method changes, it should instead be a separate thread for the UI". It is gone.

What stays is what changes nothing on screen: **paused, the scene drawn only
on change**, in the UI app -- no input within 0.3 s (`INPUT_HOLD`), no camera
move by the controller (a key held sends nothing between its repeats), the
last frame drawn into the target there is now (`Window::drawn_generation`,
made anew by a resize or a rebuild), nothing asked of the frame (an export, a
per-facet query, a pick) and no reference summing -- then neither
`Window::update` nor `render` runs, nor the queries, whose answers are the
last frame's still; the panels draw onto the scene as it was, and the loop
sleeps to the next present. Once a second (`IDLE_REDRAW`) it is drawn anyway,
for what changes without input: a level of detail finished on its thread. Only
input counts: a window opening sends resizes and uncovers of its own, which
counted at first and made a script stepping fast lose its first frames. The
frame rate counts the frames that draw.

A thread for the UI alone would not have kept the panels going under the
rays: their frame is one draw on the GPU's one queue, 0.14-0.4 s, and the UI's
draws queue behind it whichever thread sends them. What does is cutting the
scene's GPU work into slices of a few milliseconds, the UI drawn between them
with the last finished image; a UI thread besides frees the panels from what
holds the CPU -- a script's step, the TPM. The user chose both, now.

**Slices.** In the UI app a traced frame -- the rays, or a reference image's
sample -- whose bodies took the GPU longer than 12 ms the last time
(`BANDED_ABOVE_MS`, measured by the queue's done-callback) is drawn in bands
(`Window::render`, `render_continue`, `Banded`): horizontal strips of the
image by scissor, each about 6 ms (`BAND_MS`), the next sent once the GPU has
finished the last, one a turn of the loop, the panels drawn every turn. The
first band clears, the others draw beside it; no resolve until a last pass
draws the overlays over them and resolves (`render::Pass::render_band`,
`render_band_finish`; `render_reference_band` and the sum after the last for
a reference). The image as it was is copied aside (`hold_image`) and shown
until the new one is whole (`Window::display`). The frame's per-facet shadows
-- asked for at its start now, `Window::facet_wanted`, answered at its end --
are traced with rays a range of facets a turn (`raytrace::FacetPass`, a
`first` facet in the uniform), read back without waiting; the frame, its
`after_render` and its step end when they are in. Only the UI app: a plain
window shows each frame whole. `tests/test_banded_frames.py`, a UI app and a
plain window apiece: rays in 24-26 bands of 150-190 ms, the image the same to
a level (0 of 240,000 pixels); a reference with two bounces summed to 32
samples, the same; 80,012 facets' shadows by ranges, the same to the bit.

**The scripts' thread.** Stage two. In the UI app -- `python -m kalast` and
the `kalast` binary, its interpreter linked in -- scripts, their
`before_render` and `after_render`, and the python tab run on a thread of
their own (`app::script`, `py::app::script`); the main thread keeps the
window, the panels and the scene's drawing, and never takes Python's lock:
`App.run_editor` lets go of it for the session. A script working between two
steps, or a slow callback, holds that thread, and the panels go on.

- **The scene shared.** Everything both reach went from `Rc<RefCell<..>>` to
  `Locked` (`src/locked.rs`, `e9746e5`): an `Arc` around the same `RefCell`,
  every borrow of any of them under one lock for the whole process,
  reentrant, so a thread borrows as it did -- nested, the same value twice --
  and only the other waits. One lock, no order to take two in; the main
  thread never takes Python's, so no deadlock with that either. Callbacks
  are `Send`.
- **Messages.** The main thread asks for a script to run, the python tab to
  be served, a callback to be called; the scripts' thread asks for a frame --
  a script's `app.step()` -- and says when what it was asked is done
  (`ToScript`, `ToMain`). `step()` there waits for its frame with the
  interpreter let go, serving its callbacks and the console meanwhile.
- **The frame in phases** (`Phase`): begun, its `before_render`, drawn (or in
  bands), its `after_render`. A Python callback goes to the thread and the
  frame waits for it a turn of the loop at a time, as for its bands, the
  panels drawn every turn; most return within the 2 ms the frame waits before
  its turn ends. A script running there has its frames when it steps, one a
  `step()`; between two, the panels only, over the last image. A run, a
  reset, a mesh or an example wait for the thread to be free; lines typed go
  to it, served when it is free or at the running script's next `step()`.
  `dt` is the time between two frames' beginnings now, not between two turns.
- **A script named on the command line** still runs before the window
  exists: no turn of the loop, which would make one, until it steps or ends.
- **Rust examples** stay on the main thread: a hosted library carries its own
  copy of the lock. The thread is free while one runs, and the python tab is
  served on it between the example's frames, waited for, the example not
  running meanwhile -- so the two never touch the scene at once. What the
  console calls `app.simulation` is the example's, on screen
  (`Shared::shown`).

Found on the way:

- **A callback called again.** The end of a turn reads what the thread says,
  and took a callback's answer before the frame did, which then asked again:
  `after_render` ran six times over one frame. An answer is kept until the
  frame takes it (`Main::called`).
- **The loop spun.** Waiting for the thread, the loop waits for what it says
  until the next present. A covered window refuses every present, so the
  next one was always due: 20,000 turns a second, a paused scene's as well,
  before this. The swapchain is asked once an interval since the last time
  it was asked, given or refused (`present_tried`): about 115 turns a second
  covered.
- **A mesh read under the lock.** `load_mesh` held the simulation while it
  read the file -- seconds for a large mesh, the window's turn waiting on the
  lock. The body is read first and added after (`Simulation::read_body`).
- **Freed on the wrong thread.** Python objects kalast makes belong to the
  thread that made them. Restart left the first run's module as a cycle,
  freed by the interpreter's last collection on the main thread at exit, and
  pyo3 said so ("State is unsendable, but is being dropped on another
  thread"). As the window closes the thread lets go of the callbacks, its app
  and the editor's namespaces, and collects, the front door waiting up to 2 s
  (`script::Ended`, `kalast.editor.forget_scripts`).
- **`step()` gave up after 5 s** without a frame, meant for a surface that
  never configures. A frame in bands, or waiting for a callback, takes
  longer: 5 s without a turn of the loop now.
- **matplotlib** draws to files on the thread (`MPLBACKEND` defaults to
  `agg` there): macOS takes the process down over a window opened off the
  main thread.

Limits: Restart waits for a script that never steps -- a long computation at
its top level -- to end; the panels go on meanwhile. `app.step()` from a
thread of the script's own raises, as it did. The plain window (`App.step()`
or `start()` from a terminal) is as it was: one thread, its callbacks called
in the frame. Measured after, plain windows on the M1 Pro: the Didymos pair
1425-1433 frames a second without shadows (1421 before), 690-693 with the
maps (685), Dimorphos from 68 m 204 (201). `tests/test_script_thread.py`,
through `python -m kalast`: the script, its callbacks and the console off the
main thread, the loop turning 98-113 times a second while a script sleeps
3 s, each callback once a frame with `dt` the frame's (0.21 s for a 0.2 s
callback), Restart ending a driven script at its next step and running it
again, nothing freed off its thread. By hand: a slow `after_render` and a
driven script through the binary; the crater example loaded from a script,
the console served between its frames and seeing its scene, Play and close
from the console.

## The reference image a method

"i maintain that render reference image as a separate option that is not part
of method is confusing, i can have method None no shadow and reference image
render, i can also have reference image as shadow map. It makes no sense". It
is `shadows.method = "reference"` now (`ShadowMethod::Reference`, `traces()`
with the rays); `reference.enabled` went, unreleased, and `reference.error`,
`max_samples` and `bounces` show under the method in the list. Its per-facet
shadows are the rays' (`ray_samples` and `ray_probe` show under both), a sum
over frames being no answer for a step. A GPU without ray queries draws it
with the maps, the kalast tab saying so.

## The console

The python tab ran its lines in the running script's own globals -- by
design, to read and set the script's variables. `examples/cube/light.py`
sets `a` every frame (`a = 0.001 * app.simulation.state.iteration`), so the
console's `a = mesh` lasted until the next frame. Typed directly,
`app.simulation.bodies[0].mesh.facets[4]` worked, which is what made it look
like completion had broken.

The console has its own namespace now (`_console_namespace` in
`kalast/editor.py`): the script's variables copied in before each line and
each Tab, except the names the console bound itself, which `symtable` finds
in each statement it runs -- assigned, defined, imported, a loop's or a
`with`'s. A name deleted, or not bound because the line failed, shows the
script's again. A new run starts it afresh, as the script is. What it gives
up: assigning a script variable from the console no longer reaches the
script; mutating the objects it holds still does, `app` included.
`tests/test_console_namespace.py`.

## Tests

`tests/test_shadow_method.py`: a plate and a wall, the Sun 25 deg up. Maps:
the 48 facets behind the wall hidden. None: no facet of the plate hidden, the
wall's facets facing away hidden either way, and the image the maps' outside
the shadow to the level, 14,694 pixels lit inside it, none dim. The colour
mode's names and numbers with their warnings. On the M1 Pro the suite's 45
scripts pass, and the Rust tests' 335.
