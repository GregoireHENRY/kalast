# `app.simulation.config` reference

Every option on the **simulation's** config, what it accepts, what it does,
and where in the code it takes effect.

There are two configs, and they answer different questions.
`app.simulation.config` -- this document -- is about the thing being simulated
and the image made of it. `app.config` is about the application you are
looking at: its window now, its panel layout and colours as the editor grows.
It holds `editor`, `focus`, `width`, `height` and `toolbar`.

**Every option in this document has a widget in the editor's Config section**,
and that is guaranteed rather than kept up by hand: `src/app/gui/config_panel.rs`
is generated from `src/app/config.rs`, and `tests/test_config_panel.py` fails
if a field has no widget. Add an option and it appears in the panel. The two
exceptions are marked `:skip:` in the Rust and say why in their entry here --
`colormap`, which is an array a script passes, and `app.config.editor`, a
checkbox that would switch the UI off from inside the UI.

The reverse is not true of the rest of the API: see **What the panel reaches**
in `API.md` for what the editor does *not* get to.

### `app.config.focus: bool` — default `False` *(live)*
Give the whole window to the renderer: the panels get out of the way, each
coming back when the pointer reaches its edge — top for the toolbar, left for
the script, right for the config, bottom for the log — and staying while the
pointer is anywhere over it.

The edge strip is 24 points, and deliberately not enough on its own to hold a
panel open: the config panel is 240 wide, so reaching for anything in it would
leave the strip and the panel would vanish under the pointer.

Focus on the scene, in other words — not on the window, which is yours to size
by double clicking its title bar or dragging a corner, and which this does not
touch. Measured on a 1400×900 window: the viewport goes from 320×614 to
1400×900.

**The panels float over the scene here, they do not take space from it.** A
side panel shrinks the central area, so revealing one would reallocate the
render target's colour, MSAA and depth textures and shift the image under the
pointer. In focus mode the scene is drawn at the full window size behind
everything and the panels are laid on top, so the viewport is the same size
whether a panel is showing or not.

Independent of `simulation.config.fullscreen`, which is the OS window and
nothing else. Set both for an immersive fullscreen.

### `app.config.editor: bool` — default `False` *(startup only in practice)*
Draw in the editor layout. `start_editor()` is this plus `start()`, and
`python -m kalast` sets it.

Deliberately **not** in the config panel: a checkbox that switches the UI off
from inside the UI leaves nothing to switch it back on with.

### `app.config.toolbar: str` *(live)*
What the editor's toolbar says beside Play, Restart and Step. Default
`"iteration {drawn}    {its} it/s    {fps} fps"`; empty for a bare toolbar.

The same template as a HUD, so every placeholder below works here too --
including a precision, as `{fps:.1}`. It reads `{drawn}` rather than `{it}` by
default because once the frame for iteration 0 is drawn the counter is already
1, and "iteration 1" under a picture of iteration 0 is a lie of one frame.

**Both have `width`/`height`, and they are not the same number.**
`app.config.width` is the OS window. `app.simulation.config.width` is the
*image*: the camera's aspect ratio, where axis ticks project, where the colour
bar sits, and what an exported frame measures. It defaults to `0`, meaning
"follow the window" -- what a terminal run wants, and what every script got
when there was one pair of these. Set it to pin the render independently: a 4K
export from a small window, say.

While the two differ the window shows the **top-left of the image, not a
scaled version**, because the blit is a straight texel copy and cannot scale.
The export still gets the full pinned frame, which is the point of pinning it.
The editor has no such limit -- egui samples the render texture into a panel
of any size.

Defined in `src/app/config.rs` (`Config` struct + its `Default` impl).
Exposed to Python in `src/py/app/config.rs` -- every field has a getter and a
setter, so all of them are readable and writable as `app.simulation.config.<name>`.

```python
app = kalast.app.App()
app.simulation.config.width = 1020
app.simulation.config.vsync = False
print(app.simulation.config)   # __repr__ dumps the whole struct
```

**Timing matters -- most options are read once.** The config is consumed when
the window is created, inside `app.start()`. Set everything before calling it.

- *(live)* -- re-read by the renderer every frame. `Globals` is rebuilt and
  re-uploaded each frame (it has to be, since the automatic shadow constants
  change as the scene moves), so all the shading and shadow settings are read
  fresh; `background` and the debug-draw flags are read straight from the
  config during the pass.

  **Change them per frame from a callback**, which receives the app:

  ```python
  def before_render(sim: Simulation, dt: float) -> None:
      sim.config.colorbar = sim.state.iteration > 100
  ```

  This works because `py::App` holds the config beside the app rather than
  inside it. Fetching it through the app would hit the borrow `start()` takes
  for the whole run loop and raise `RuntimeError: Already mutably borrowed`,
  which is what it used to do.
- *(live, ...)* -- also live, but realising the change costs something, so
  the note says what. Each frame compares the config against what the window
  was actually built with and acts only on a difference; a frame where nothing
  changed does nothing. The costs are:
  - *reconfigures the surface* (`vsync`) -- no GPU resource recreated, only
    the swapchain's pacing.
  - *rebuilds pipelines* (`msaa`, `render_back_face`) -- sample count and cull
    mode are fixed when a pipeline is created, so changing either recompiles
    the shaders. Fine on a settings change; not something to drive per frame.
  - *reallocates the shadow map* (`shadow_resolution`) -- a new depth texture
    plus the pipeline rebuild, since the texture is bound through the pass
    bind groups.
  - *rebuilds the glyph atlas* (`hud_font`) -- a brush owns its atlas, so the
    font cannot be swapped inside one.
  - *flushes the queue first* (`export_dir`, `export_sync`,
    `export_max_queued`) -- frames already queued belong to the directory they
    were queued for, so the old exporter is finished before the new one
    replaces it, and a change made mid-export blocks until the backlog is on
    disk.
- *(startup only)* -- read once during setup, so a later change updates the
  Python-visible field but **has no effect**. Only the two `debug_window*`
  flags are left here, and only for what they print while the window is being
  built; their per-frame prints follow the config as it stands.

`width` and `height` are a request, not a command: a tiling window manager or
a fullscreen window may refuse the resize. The surface follows the `Resized`
event that a granted request produces, so the change lands a frame or two
later rather than instantly.

Everything a script touches *outside* the config — `app.simulation`,
`sim.state`, `sim.huds`, `sim.bodies`, the camera and Sun, the GPU-result
queries — is in **`API.md`**.

**Frame callbacks.** The app runs two per frame, either optional:

| | when | for |
|---|---|---|
| `app.before_render` | before the frame is drawn | positioning bodies, sun, camera |
| `app.after_render` | after it is drawn | consuming GPU results for *that* frame, e.g. `sim.facet_shadow(body)` |

`app.tick` is an alias for `before_render`. Both see the same
`state.iteration` for a given frame -- the counter advances only once both
have run -- so a loop deriving an epoch from it cannot see two different
times within one frame. Scene changes made in `after_render` take effect on
the next frame, and heavy CPU work there blocks the render loop.

Rust types map to Python as: `bool` -> `bool`, `u32` -> `int`,
`f32`/`Float` -> `float`, `String` -> `str`, `wgpu::Color` -> a 4-tuple of
floats `(r, g, b, a)`.

---

## Debug / diagnostics

### `debug_app: bool` — default `false` *(live)*
Prints app-level lifecycle events. Read at `src/app/mod.rs:214` and
`src/app/mod.rs:223`.
Accepted: `True` / `False`.

### `debug_window: bool` — default `false` *(startup only)*
Prints window and GPU setup detail during surface configuration: chosen
surface format (`src/app/window.rs:114`), adapter features, device features,
and **the list of present modes the surface supports**
(`src/app/window.rs:134`). Also prints surface-reconfiguration events at
`src/app/mod.rs:147`.
Accepted: `True` / `False`.
Worth enabling once on any new machine -- it is how the vsync cap described in
`2026-08-25_BENCH_mesh_resolution_results.md` was identified.

### `debug_window_mesh: bool` — default `false` *(startup only)*
Prints per-mesh detail as meshes are uploaded. Read at
`src/app/window.rs:161`.
Accepted: `True` / `False`.

### `debug_simulation: bool` — default `false`
**Currently does nothing.** The field exists and is exposed to Python, but no
code reads it -- the only references are its declaration, its default, and its
Python accessors. Left in place as a placeholder.

### `debug_depth_show: bool` — default `false` *(live)*
Renders the shadow/depth map as an overlay instead of leaving it offscreen, by
running an extra depth-visualisation pass. Read at `src/app/pass/mod.rs:58`,
which calls `self.depth.render(view, encoder)`.
Accepted: `True` / `False`.
Useful for debugging shadow acne / peter-panning alongside the `shadow_bias_*`
options.

### `debug_light_cube_show: bool` — default `false` *(live)*
Draws a small cube at the light's position, so you can see where the sun
actually is. Read at `src/app/pass/render.rs:100`. Its size is controlled by
`light_cube_scale`.
Accepted: `True` / `False`.

`debug_light_cube_fit` below is on by default, which is what makes the cube
*visible* rather than merely drawn: the camera's far plane is fitted to the
bodies, and the Sun is usually well outside them.

### `debug_light_cube_fit: bool` — default `true` *(live)*
Fit the camera's frustum around the light cube as well as the bodies, which is
what makes `debug_light_cube_show` show something. On by default for that
reason -- asking to see the light and being shown nothing is not a useful
default -- and inert unless the cube is being drawn.

The **camera** only. The light's own frustum stays fitted to the bodies,
because that is what the shadow map covers: stretching it to the Sun would
spend the whole map on empty space and leave the bodies a few texels across.

Costs depth precision — a far plane at the Sun rather than at the body's edge
is a far longer near-to-far span. For looking at where the light is, not for a
figure.
Accepted: `True` / `False`.

---

## Window

### `title: String` — default `"kalast"` *(live)*
The OS window title. Applied at `src/app/mod.rs:117` via winit's
`.with_title()`.
Accepted: any string.

### `width: u32` — default `800` *(live)*
### `height: u32` — default `600` *(live)*
Initial window size in pixels, and therefore the render-target size and the
resolution of exported PNGs. The exporter reads the surface dimensions, not
these fields directly -- see `src/app/window.rs:450`, which passes
`self.surface_config.width/height` into `export_frame`.
Accepted: any positive integer within what the GPU allows.
Note exported frame size follows the *live* surface size, so resizing the
window mid-run changes the size of subsequent exports. Export buffers are
pooled by byte size, and stale-sized pooled buffers are discarded on resize
(`src/app/gpu.rs`, the `pool_rx.try_recv()` loop in `export_frame`).

### `fullscreen: bool` — default `false` *(live)*
Fill the screen. Accepted: `True` / `False`.

On macOS this is the **simple** fullscreen — the pre-Lion kind, the one
Electron gives VS Code. The window grows to cover the screen, menu bar
included, and stays on the Space it is on: no new desktop, no swipe to reach
it, and other windows can still sit on top of it. `WindowExtMacOS::
set_simple_fullscreen`. Elsewhere it is `Fullscreen::Borderless(None)`, which
is the nearest those platforms have.

This is the **window** and nothing else. For the renderer to take the whole
window with the panels out of the way, see `app.config.focus` — the two are
independent, and setting both is rid of everything at once.

**The green button does this too, on macOS.** It is AppKit's
`toggleFullScreen:` and winit offers no hook, so the window opts out of native
fullscreen instead — `NSWindowCollectionBehavior::FullScreenPrimary` cleared —
which turns that button into a plain zoom. A zoom is instant and stays on the
Space, and `App` reads one as "fullscreen was asked for": it puts the zoom
back and applies the simple fullscreen. The window therefore remembers the
size it had before the button was pressed.

`F` toggles it from the keyboard, which is the way out: simple fullscreen
hides the title bar, and with it the button.

**The transition used to be the expensive part, and is not any more.** Native
fullscreen — `Fullscreen::Borderless`, what the green button does — moves the
window to a Space of its own, and that animation was measured stalling the
render loop badly:

| | stall acquiring a drawable |
|---|---|
| launched native fullscreen, `shadow_pcf = 0` | 104 ms, once, at startup |
| launched native fullscreen, `shadow_pcf = 4` | 588 ms, once, at startup |
| **toggled** to native fullscreen while running | **1001 ms and 3725 ms**, repeatedly |
| toggled to **simple** fullscreen while running | nothing measurable |

1001 ms was not a coincidence: it is Metal's `nextDrawable` timeout, so during
a native toggle the compositor handed out no drawable at all for a full
second. The Space animation moved the window to a new surface mid-flight,
winit delivered a burst of `Resized` events, and each one reconfigured the
swapchain and invalidated the drawable pool while the render loop kept asking
at full rate.

Simple fullscreen has none of that machinery — it is a resize. Measured over
140 frames across a toggle at `shadow_pcf = 4`, four runs: 24, 25, 26 and
53 ms, and only twice did the worst frame fall anywhere near the toggle. One
long frame is what reallocating the swapchain and every render target at a
larger size costs, and it is the whole of it.

It also took the scale-factor change with it, which is what used to crash a
green-button fullscreen before `ScaleFactorChanged` was handled at all.

### `render_back_face: bool` — default `false` *(live, rebuilds pipelines)*
Whether triangles facing away from the camera are drawn.

- `false` (default): back faces are culled -- `Some(wgpu::Face::Back)` on the
  main render pipeline.
- `true`: no culling, both winding directions drawn.

Set at `src/app/pass/render.rs:47`, consumed by `RenderPipeline::new`
(`src/app/gpu.rs:50`), which puts it in the pipeline's `PrimitiveState`
alongside `front_face: Ccw` (`src/app/gpu.rs:114-115`).
Accepted: `True` / `False`.

Leave it `false` for closed shape models -- back faces are invisible there, so
culling them is free performance. Verified on the full-resolution
Didymos/Dimorphos meshes: culled vs unculled renders differ in **5 pixels out
of 1,040,400**, all on silhouette edges.

Set it `true` for geometry that is *not* closed -- open craters, clipped
sections, single-sided surfaces -- where the inside of the shell has to be
visible from outside. Otherwise those faces vanish.

The shadow pass deliberately stays unculled regardless, so non-closed geometry
still casts correctly from whichever side faces the light.

**This option previously did nothing.** Every pipeline passed `cull_mode:
None` (no culling at all, equivalent to `render_back_face = true`), with the
intended value sitting in a comment. Wiring it up means the default `false`
now culls -- a behaviour change, though as measured above an invisible one for
closed meshes.

---

## Presentation

### `vsync: bool` — default `true` *(live, reconfigures the surface)*
`true` requests `wgpu::PresentMode::Fifo` (vsync, frame rate pinned to the
display refresh rate). `false` requests `PresentMode::Immediate` (uncapped).
Resolved by `pick_present_mode` at the bottom of `src/app/window.rs`, used at
`src/app/window.rs:127`. If the requested mode is not supported it falls back
to `caps.present_modes[0]`, which is always available.
Accepted: `True` / `False`.

**Set this to `False` for any performance measurement.** With vsync on, a GPU
faster than the display simply reports the refresh rate: on a 239 Hz panel the
render loop measured exactly 239.46 it/s regardless of scene complexity, which
made a 3.1M-facet scene look identical to a 100k-facet one. Details in
`2026-08-25_BENCH_mesh_resolution_results.md`.

Before this option existed, `present_modes[0]` (typically `Fifo`) was the
unconditional choice, so every run was vsync-capped.

### `msaa: u32` — default `4` *(live, rebuilds pipelines)*
Multisample anti-aliasing on the main render pass. `1` turns it off; `2`, `4`
and `8` are the useful values.
Accepted: any `int`, but a count the adapter does not support falls back to
`4`, then to `1` (`resolve_samples`, `src/app/pass/render.rs`). Asking wgpu
for an unsupported count would otherwise panic at pipeline creation.

On by default because **every silhouette this renderer draws is a
measurement** — a limb, a terminator, an apparent diameter. At one sample per
pixel each is quantised to whole pixels, which biases anything fitted from an
exported frame.

**Only the main pass is multisampled.** The shadow, facet-id and hemicube
passes stay single-sampled on purpose: they carry ids and depths, not colour,
and averaging an id across samples yields an id belonging to no facet — and
the facet-id buffer is what attaches a temperature to a pixel in the FITS
products. The light cube is drawn inside the main pass, so its pipeline takes
the same count; wgpu rejects a pipeline whose count disagrees with the
attachments in flight.

**Exports are unaffected in shape or size.** The pass draws into a
multisampled buffer and resolves into `render_texture`, the same
single-sample target that has always been blitted and exported, so nothing
downstream changed.

**Caveat:** `debug_depth_show` mirrors the main pass's depth only at
`msaa = 1`. Above that the pass writes its own multisampled depth buffer, and
the debug view is not it.

Unmeasured: there is no before/after on a fitted limb radius or centroid. MSAA
changes edge pixel values, so a measurement taken from a 1-sample export and
one from a 4-sample export are not the same measurement.

---

## Frame export

Frames are written as `{export_dir}/{N}.png`, `N` counting up from 0.
Exporting is triggered per-frame from the simulation, not from the config:
`sim.export_once()` for a single frame, `sim.toggle_export()` for continuous
export. See `src/app/simulation.rs:70` and `src/app/window.rs:359`.

### `access_shadow_map: bool` — default `false` *(live)*
Read the shadow map back per facet: computes solar occlusion for **every**
body each frame, read from
`after_render` with `sim.facet_shadow(body)` -- an array of occluded
fractions in `Mesh.facets` order, `0.0` fully lit to `1.0` fully shadowed in
quarter steps. Bodies not computed this frame return `None` rather than a
stale array.

Off by default because it is not free: ~1.6 ms per body at 100k facets and
~7.3 ms at 3.1M, dominated by the blocking readback. Turn it on for
thermophysical or radiance work, leave it off when you only want images.

For sparse use -- only particular epochs -- leave it off and call
`sim.request_facet_shadow(body)` from `before_render` for the frames you
want.

Full write-up, validation against ray tracing and accuracy budget in
`2026-08-26_facet_shadow_query/`.
Accepted: `True` / `False`.

### `export_dir: String` — default `"out/frames"` *(live, flushes the queue first)*
Destination directory, created if absent (`src/app/gpu.rs`,
`FrameExporter::new`). Numbering **resumes after any files already present**,
so an existing run's frames are never overwritten -- the directory is scanned
once at startup for the highest numeric filename.
Accepted: any path string. Forward slashes work on Windows.

Give dev/test runs their own directory. Two `FrameExporter`s pointed at the
same directory race on both the startup index scan and any cleanup, so an
`rm -rf` of one process's directory can delete files another just wrote.

### `export_sync: bool` — default `false` *(live, flushes the queue first)*
Chooses how exported frames reach disk.

- `False` (async, default): the GPU->CPU copy is spread across frames and
  drained non-blockingly, then PNG encoding and the disk write happen on a
  pool of background threads (2-8, `available_parallelism` clamped). The
  render loop only pays a buffer copy and a cheap handoff.
- `True` (sync): `export_frame` blocks until that frame is on disk, via
  `save_last_blocking` in `src/app/gpu.rs`. A full GPU stall plus encode plus
  write, inline on the render thread, every frame.

Both paths call the same `save_job` function, so output files are identical.

Accepted: `True` / `False`.

**Why you might want `True`.** The async queue is *unbounded*. If the render
loop outruns the encoders -- easy on a fast GPU -- the backlog grows without
limit, each queued frame pinning a mapped GPU buffer, and anything still
queued when the process dies is silently lost. Measured at 1020x1020, vsync
off, debug build:

| | async | sync |
|---|---|---|
| Render rate | 626 it/s | 4.84 it/s |
| RSS after ~16 s | 30 GB, growing ~2 GB/s | 340 MB, flat |
| Frames on disk (500-iteration run) | 32 / 500 | 500 / 500 |

The async rate is largely queueing debt: sustained on-disk throughput was
~5.6 frames/s, so 4.84 it/s is close to the honest end-to-end cost.

Async is the right default for interactive use and for runs that export
sparsely. Use sync when every frame must land and memory is a concern.

Regardless of mode, call the app's normal shutdown path -- `finish()` blocks
until the queue drains (`src/app/gpu.rs`). Killing the process mid-run
abandons whatever is outstanding and leaves as many truncated trailing files
as there are save workers.

### `huds: list[Hud]` — default `[]` *(live)*
### `hud_font: String` — default `""` *(live, rebuilds the glyph atlas)*
On-screen overlay text, drawn over the swapchain after the blit — so by
default it stays out of exported frames (`export_hud` adds it to those too).
Empty draws nothing.

`app.simulation.config.huds` and `app.simulation.huds` are **the same list**, not two.
Declare the HUDs once at setup and edit them per frame in `before_render`;
the objects handed back are the live ones, so setting `.text` takes effect
without reassigning anything.

```python
app.simulation.config.huds = [
    kalast.app.Hud("{it}/{nit} ({its} it/s)"),                 # top-left
    kalast.app.Hud("{fps} fps  {ms} ms", anchor="bottom-right"),
    kalast.app.Hud("", x=200, y=120, size=24.0),               # filled in below
]
app.simulation.config.hud_font = "Arial"                 # or a path; built-in otherwise


def before_render(sim: Simulation, dt: float) -> None:
    app.simulation.huds[2].text = f"epoch {spice.et2utc(et, 'C', 0)}"
```

A HUD that `before_render` does not touch keeps the text it had — these are
persistent objects, and only the placeholders re-expand each frame. A HUD
whose text is empty draws nothing, which is how to declare one now and fill
it in later.

Text set from a script is **still a template**, so `sim.huds[0].text =
f"{i}/{n} {{fps}} fps"` works — the placeholders below expand in whatever
is there.

#### Placeholders

| | |
|---|---|
| `{it}` | iteration count, `sim.state.iteration` |
| `{drawn}` | the iteration the frame **on screen** was drawn for |
| `{nit}` | `sim.state.pause_at` if set, else `?` |
| `{its}` | iterations per second; `0` while paused |
| `{fps}` | frames per second |
| `{ms}` | **frame time in milliseconds**, i.e. `1000 / fps` |
| `{paused}` | `PAUSED` when paused, empty otherwise |

`{ms}` is the same information as `{fps}` inverted, but it is the one you
compare against a frame budget: 8.3 ms is the whole of a 120 Hz frame, and
"11.8" says immediately that you are missing it where "85 fps" does not.

#### `pin`

A HUD's `pin` is used in place of its `text` while it is set, whatever `text`
says. It is how the editor's HUDs section takes a HUD off a script: a callback
that assigns `text` every iteration owns it completely, and a *driven* script
(`while app.step():`) goes on assigning even while paused -- pausing stops the
iteration counter, not a `while` loop the script owns -- so there is nowhere
else an edit could stand.

Typing in that section sets it; its release button, or `hud.pin = None`, gives
the HUD back. The script goes on writing `text` throughout, harmlessly.

`{drawn}` trails `{it}` by one for the whole of every frame: the counter moves
on only after the frame it belongs to has been drawn, so `{it}` is how many
iterations have been *begun* and `{drawn}` is the one you are looking at. While
paused they differ too -- the counter has already stepped past what is on
screen, and `{drawn}` stays on the last frame that advanced.

`{nit}` reads `?` rather than a number when nothing has set `pause_at`, because
the engine genuinely does not know how long your run is. `{its}` and `{fps}`
differ **only** while paused: `P` stops the iteration counter but not the
render loop, so reporting the frame rate as an iteration rate would be false.

An unrecognised `{name}` renders verbatim — `"{nope} x {it}"` gives
`"{nope} x 1"` — so a typo, or braces in your own text, cannot break the frame.

#### Number format

**Rates are whole numbers** unless you ask otherwise: `{fps}` → `60`,
`{fps:.1}` → `59.6`, `{fps:.2f}` → `59.62`. A frame rate quoted to a tenth is
noise and the digit churns without informing anyone. `{ms}` is the exception
and keeps one decimal by default, since whole milliseconds cannot separate 8
from 8.4; `{ms:.0}` overrides that.

**Rates average over a one-second window** and then update, rather than being
smoothed per frame. An exponential moving average still moves every frame, so
the digits change faster than they can be read. The window is
`HUD_RATE_WINDOW` in `src/app/mod.rs`.

#### `Hud`

`kalast.app.Hud(text, anchor="top-left", x=None, y=None, size=18.0,
color=None)`.

`size` is the font size in pixels and is **per HUD**, so a large counter and a
small frame-rate readout cost nothing extra. `color` is `(r, g, b, a)`.

`anchor` is one of nine: `top-left`, `top-center`, `top-right`,
`middle-left`, `middle-center`, `middle-right`, `bottom-left`,
`bottom-center`, `bottom-right`. Hyphens, underscores and spaces are
interchangeable, and case is ignored, since all of those get typed. Anything
else raises `ValueError` listing the valid values rather than silently
defaulting.

`align_h` (`left`/`center`/`right`) and `align_v` (`top`/`center`/`bottom`)
set how the text block aligns *within itself*, separately from where it sits.
`None`, the default, follows the anchor — which is what you want almost
always. They matter when a multi-line block should read left-aligned while
sitting against the right edge, where following the anchor would ragged-right
it.

`x`/`y` are an **inset from the anchor**, defaulting to 8 and 6 — so a
bottom-right HUD sits as far from its own edges as a top-left one does, and
neither moves when the window is resized.

**Absolute positioning needs no special anchor.** The top-left anchor is the
origin, so `Hud(text, x=200, y=120)` places the HUD at exactly (200, 120).
There was briefly a `custom` anchor for this; it computed the identical
position to `top-left` and was removed as a second name for the default.

#### Font

`hud_font` takes either a **font name** or a **path**, and applies to *every*
HUD; empty uses the built-in DejaVu Sans:

```python
app.simulation.config.hud_font = "Arial"                      # name
app.simulation.config.hud_font = "Times New Roman"            # spaces and case ignored
app.simulation.config.hud_font = "/Library/Fonts/Arial.ttf"   # path
```

Anything that exists on disk is treated as a path; anything else is looked up
by name in the platform's font directories, most specific first so a
user-installed font beats a system one:

| | |
|---|---|
| macOS | `~/Library/Fonts`, `/Library/Fonts`, `/System/Library/Fonts`, `/System/Library/Fonts/Supplemental` |
| Windows | `%LOCALAPPDATA%/Microsoft/Windows/Fonts`, `C:/Windows/Fonts` |
| other | `~/.local/share/fonts`, `~/.fonts`, `/usr/local/share/fonts`, `/usr/share/fonts` |

`.ttf`, `.otf`, `.ttc` and `.otc` are all read; for a collection the first
face is used.

**Matching is on the filename, not the family name recorded inside the font.**
Reading real family names needs a font-database dependency (`fontdb`,
`font-kit`), which an overlay does not justify — filenames cover the names
anyone actually types. Punctuation and case are ignored, so `Times New Roman`
finds `Times New Roman.ttf` and `HelveticaNeue.ttc` answers to
`"Helvetica Neue"`. A family whose file is named nothing like it will not
resolve; give a path in that case.

It is one font for all HUDs because each additional font needs its own glyph
cache and draw, which is not worth it for an overlay — per-HUD `size` is free
by comparison.

A font that will not resolve **warns once and falls back** to the built-in:

```
hud_font: no font named "NotAFont" in ["/Users/x/Library/Fonts", ...], using built-in
```

rather than leaving the run with no HUD. Losing the counters a long run is
being watched by, because a font path was mistyped, is a worse outcome than
the wrong typeface.

Right- and bottom-anchored HUDs are placed with glyph_brush's own alignment
rather than by measuring the text. The text changes every frame, and measuring
it to subtract a width would make the block jitter sideways as digits change
width.

---

### `export_hud: bool` — default `false` *(live)*
Whether the HUD text (`sim.huds`) is burned into exported frames as well as
drawn on screen.

- `false` (default): exports carry the render alone. The on-screen HUD is
  drawn onto the swapchain *after* the exporter has copied `render_texture`
  (`src/app/window.rs`), so it cannot reach a frame.
- `true`: an extra text pass draws the HUD into `render_texture` before that
  copy, so exported PNGs show it too. The window is unchanged either way --
  the text is simply drawn twice.

Accepted: `True` / `False`.

Leave it `false` for anything that is a data product: a GIS3D/TIRI frame set
should be the render and nothing else. Turn it on for a screen-capture-style
movie where the run state should be legible in the frames themselves. Costs
one text pass, and only on frames that are actually exported.

### `export_max_queued: u32` — default `64` *(live, flushes the queue first)*
Upper bound on frames that have been exported but not yet written, before
`export_frame` blocks the render loop to let the encoders catch up. Enforced
by `apply_backpressure` in `src/app/gpu.rs`; ignored when `export_sync` is on.
Accepted: any `int >= 0`. **`0` disables the bound**, restoring the original
unbounded behaviour.

Memory is capped at roughly `export_max_queued * width * height * 4` bytes --
at the default 64 and 1020x1020, about 266 MB.

Measured at 100k facets, vsync off, debug build:

| | unbounded (`0`) | bounded (`64`, default) | `export_sync = True` |
|---|---|---|---|
| Reported rate | 626 it/s | 45 it/s | 4.84 it/s |
| Frames actually reaching disk | ~5.6/s | 45/s | 4.84/s |
| RSS after ~16 s | 30 GB, growing ~2 GB/s | 627 MB, flat | 340 MB, flat |

Note the bound makes the pipeline **faster**, not slower: unbounded, the
render thread starves the encoder pool and thrashes memory, so only ~5.6
frames/s reach disk despite the loop reporting 626 it/s. Bounded, it reaches
45/s. The unbounded rate was never real throughput -- it was queue growth.

Lower it if memory is tight, raise it to absorb burstier export patterns. A
hard kill still abandons whatever is outstanding, so the bound also caps how
many frames a crash can lose; a normal exit loses nothing.

---

## Camera controls

Bindings in the default Arcball mode:

| Input | Action |
|---|---|
| Middle-drag | orbit |
| Alt + left-drag | orbit (when `emulate_middle_button`) |
| Shift + middle-drag | pan |
| Shift + alt + left-drag | pan (when `emulate_middle_button`) |
| Wheel / two-finger scroll | zoom |
| Pinch gesture | zoom (trackpad) |
| `T` | toggle Arcball / WASD |

WASD mode grabs the cursor and uses `W A S D` + `Space` / `LeftShift` to fly.

All four sensitivities scale a corresponding built-in constant, so `1.0` means
"the default feel" and `2.0` means "twice as fast". Copied onto the controller
**once**, by `apply_config_at_start` (`src/app/mod.rs:66-70`); the controller
is then applied each frame via `sim.camera.update_with_controller`
(`src/app/mod.rs:184`) using the arithmetic in `src/app/frame.rs`. Changing
them after `start()` does nothing.

Pointer-driven terms are deliberately **not** scaled by frame time -- a mouse
delta is a displacement, not a rate. `sensitivity_move` doubles as the pan
scale; `sensitivity_look` applies to WASD only.

### `emulate_middle_button: bool` — default `true` on macOS, `false` elsewhere *(live)*
Treat **alt + left-drag** as a middle-drag, so the arcball can be orbited on
hardware with no middle button -- a trackpad. Blender calls the same setting
"Emulate 3 Button Mouse", and the binding matches, so the muscle memory
carries over. The real middle button keeps working either way; turning this
off only makes alt + left inert.
Accepted: `True` / `False`.
Copied onto the controller once by `apply_config_at_start`, like the
`sensitivity_*` values, so set it before `app.start()`.

### `sensitivity_move: Float` — default `1.0` *(live)*
Translation speed. `src/app/frame.rs:197`.

### `sensitivity_look: Float` — default `1.0` *(live)*
Look/pan speed, scaling `SENSITIVITY_LOOK`. `src/app/frame.rs:205,209`.

### `sensitivity_rotate: Float` — default `1.0` *(live)*
Orbit speed, scaling `SENSITIVITY_ROTATE`. `src/app/frame.rs:178,182`.

### `sensitivity_zoom: Float` — default `1.0` *(live)*
Zoom speed. `src/app/frame.rs:170`.

Accepted: any float. `0.0` disables that input; negatives invert it.

Note these only affect interactive control. Scripts that set
`sim.camera.pos` / `.dir` / `.up` from `before_render` -- as the Hera
examples do --
overwrite the controller's result every frame and are unaffected.

**A script that assigns `camera.pos`/`.dir`/`.up` every frame cannot be
orbited** -- the assignment overwrites whatever the drag just did, so the
camera appears frozen. This is not a controller bug; the Hera examples do
exactly this to follow the spacecraft. Assign the camera only on the frames
where you want it driven (e.g. `if sim.state.iteration == 0:`) if you also
want to orbit it by hand.

The camera basis is re-orthonormalised before each arcball update, so an
assigned `up` that is not perpendicular to `dir` -- or is parallel to it, which
used to produce NaN and permanently freeze the camera -- is handled.

The arcball now only touches the camera when there is actual pointer input.
It previously ran `look_anchor()` every frame, silently discarding any `dir` a
script had just assigned and re-aiming the camera at the anchor. If you want
that auto-centring back, call `sim.camera.look_anchor()` at the end of
`before_render` or `sim.camera.set_control_none()`.

---

## Color and shading

Most of these are packed into the `Globals` uniform
(`src/app/uniform.rs`) by `build_globals` in `src/app/window.rs` and consumed
by `shaders/mesh_shadow.wgsl` (and `shaders/mesh.wgsl`, `light_render.wgsl`).

That buffer is rebuilt and re-uploaded **every frame** -- it has to be, since
the automatic shadow constants change as the scene moves -- so everything in
this section is read fresh each frame. It used to be written once at startup.
Note this still does not let a script change them mid-run: see the *(live)*
caveat at the top. `background` never went through `Globals` at all; it is
read from the config directly in the render pass.

### `background: wgpu::Color` — default `BLACK` *(live)*
Clear color for the render pass. Used as `LoadOp::Clear(config.background)` at
`src/app/pass/render.rs:85`.
Accepted: `(r, g, b, a)` floats, normally 0.0-1.0.

### `color: wgpu::Color` — default `WHITE` *(live)*
A single global color. **Only used when `color_mode == 2`.** Passed as
`color_vec3(&config.color)` at `src/app/window.rs:193`; read in the shader's
`color_mode == 2` branch.
Accepted: `(r, g, b, a)`; alpha is dropped (converted to `Vec3`).

### `color_mode: u32` — default `0` *(live)*
Selects the fragment color path. Documented on the `Globals` struct in
`src/app/uniform.rs` and branched in `shaders/mesh_shadow.wgsl:117-128` (and `:175` for mode 3).

| Value | Meaning |
|---|---|
| `0` | vertex/instance color + lighting + shadow (the normal path) |
| `1` | vertex/instance color, raw -- no lighting, no shadow |
| `2` | the global `color` field, raw -- no lighting, no shadow |
| `3` | same as `0` but with shadowing forced off (`shadow = 1.0`) |
| other | falls through to `0` |

Accepted: any `int`; anything outside 0-3 behaves as `0`.

Mode `3` is the cheap way to answer "how much is the shadow pass costing me?"
without touching code -- though note it only disables the shadow *lookup* in
the fragment shader, it does not skip rendering the shadow map.

### `extra: u32` — default `0` *(live)*
A spare uniform slot. Plumbed all the way through -- config ->
`src/app/window.rs:208` -> `Globals` -> declared in `shaders/mesh.wgsl:7` and
`shaders/mesh_shadow.wgsl:13` -- but **no shader currently reads it**. It
exists so a scratch value can be pushed to the GPU without changing the
uniform layout.
Accepted: any `int`.

### `srgb_mode: u32` — default `0` *(live)*
Controls where the sRGB/linear conversion happens. `src/app/window.rs:196`.

| Value | Meaning |
|---|---|
| `0` | convert sRGB -> linear on the *input* color, to show raw color faithfully (applied inside the `color_mode == 1` and `== 2` branches) |
| `1` | treat input as already linear and convert the *final lit* color instead (applied at the end of `fs_main`) |

Accepted: `0` or `1`. Both branches call `srgb_to_linear(color, gamma)`, so
this picks which color gets converted, not whether conversion happens.

### `gamma: Float` — default `2.2` *(live)*
Exponent used by `srgb_to_linear` in the shader. `src/app/window.rs:197`.
Accepted: any positive float. `2.2` is the standard sRGB approximation; `1.0`
makes the conversion a no-op.

---

## Lighting

### `ambient_strength: f32` — default `0.002` *(live)*
Scales the light color into an ambient term added to every fragment
regardless of shadowing: `ambient_color = light.color * ambient_strength`.
`src/app/window.rs:199`.
Accepted: any float `>= 0.0`. The default is deliberately tiny -- airless
bodies have essentially no ambient fill, and raising it washes out the
terminator.

### `light_color: wgpu::Color` — default `WHITE` *(live)*
The light's colour, feeding both the ambient and diffuse terms, and the colour
the debug light cube is drawn in. Part of the `Light` uniform, refreshed each
frame beside the Sun's position.

Live only since 9 September: it was written when the window was made and never
again, so it was documented as live and was not. A value set before `start()`
reached the shader, and one set from a callback or from the config panel did
nothing at all.
Accepted: `(r, g, b, a)`; alpha dropped.

### `light_cube_scale: Float` — default `0.25` *(live)*
Size of the debug light cube, in world units. Only visible when
`debug_light_cube_show` is on. Applied in the vertex shader at
`shaders/light_render.wgsl:47`:
`vertex.pos * light_cube_scale + light.pos`.
Accepted: any float.

---

## Shadows

The shadow map is a depth texture **array** rendered from the light's point of
view, then compared against during the main pass. Sizing happens at
`src/app/window.rs:330-331`; the array is always allocated at
`MAX_SHADOW_LAYERS` layers, and `shadow_per_body` decides how many are used.
Each body carries the layer it samples as an instance attribute
(`shadow_layer`), read in `shaders/mesh_shadow.wgsl:229-230`.

**The Sun no longer needs aiming.** It is a light source, not a camera: each
layer derives its own direction from `sun.pos` and the body it targets, so
`sun.dir` and `sun.anchor` are not consulted for shadowing and `sun.pos` alone
determines the lighting. Setting them still works and is simply not read.
`sun.look_anchor()` was previously required, and forgetting it left the Sun
pointing at whatever `anchor` held — usually the origin, which is the
spacecraft in most of these scripts.

### `shadow_per_body: bool` — default `true` *(live)*
One shadow map per body — aimed at it and sized to it — instead of a single
map fitted to the whole scene. Layer count is chosen in
`src/app/window.rs:741`, the per-layer matrices at `:752-769`, and the layer a
body samples at `:839`.
Accepted: `True` / `False`.

On by default because a shared map is fitted to the scene's extent, so a small
body beside a large one gets almost no texels. **6 km Deimos beside 3,396 km
Mars is the case that forced it:** the shared map broke the terminator into
ragged stripes and found **49 shadowed pixels where the per-body map finds
249** — it was missing most of the shadow, not adding to it.

**Mutual shadowing is unaffected.** Each layer is aimed at one body but its
depth range still spans the whole scene, so anything between the Sun and that
body still casts into its map, and does so at the layer's own resolution
rather than the scene's. Verified on the Didymos/Dimorphos transit: 714 px of
1,040,400 differ, all at shadow edges.

The bias is per layer too, since one texel is a different world distance in
each — a single shared bias would be right for at most one body, giving acne
on the coarse layers and detached shadows on the fine ones. User-pinned values
still win.

`facet_shadow` reads the queried body's own layer. It feeds the TPM, so the
wrong layer would have been silently wrong physics rather than a visible
fault.

Costs one shadow pass per body. Layers cap at `MAX_SHADOW_LAYERS = 8`
(`src/app/uniform.rs:65`); beyond that bodies share the last one — degraded,
not wrong. Setting it to `False` restores the old single scene-fitted map,
which is worth doing only to reproduce older output, or when every body is a
similar size.

### `shadow_resolution: u32` — default `8192` *(live, reallocates the shadow map)*
Side length of the square shadow map, in texels. Used **twice** at
`src/app/window.rs:239,240` (width and height) and also passed into `Globals`
at `src/app/window.rs:202`, where the shader uses it to compute
`texel_size = 1.0 / shadow_resolution` for PCF offsets.
Accepted: any positive integer the GPU can allocate as a depth texture;
powers of two are the sane choice. `8192` is a 256 MB-class depth target --
lowering it to `4096` or `2048` is the first thing to try if you are tight on
VRAM.

### `shadow_pcf: u32` — default `0` *(live)*
Percentage-closer-filtering kernel *radius*.

**The normal offset scales with this**, `lb.x * (1 + shadow_pcf)`. One texel
diagonal is the right surface separation for a single tap, but an `N`-radius
kernel reaches `N` texels away and each of those taps compares against a
stored depth that far along the surface. With a one-texel offset they flip,
and averaging turns full shadow into grey: on the crater example 7,952 px of
the floor lifted out of black at `N = 4`, against 388 once scaled. `N = 0` is
bit-identical to the previous behaviour, so nothing rendered with the default
changes.

| Value | Behaviour |
|---|---|
| `0` | a single `textureSampleCompare` -- hardware 2x2 PCF only, hard edges |
| `N > 0` | a `(2N+1) x (2N+1)` grid of comparison samples, averaged |

So `1` = 9 taps, `2` = 25 taps, `3` = 49 taps. Cost grows quadratically.
Accepted: any `int >= 0`.

The blur you actually see scales with kernel radius *in shadow-map texels*,
which at the Hera geometry is only ~0.1 image pixels per unit of `shadow_pcf`
-- so small values look like no change at all. Softening becomes visible
around `8` and obvious by `24`. Worked example, measurements and side-by-side
renders in `2026-08-25_pcf_shadow_comparison/`.

**Previously buggy.** Before the current fix, the `N > 0` branch accumulated
taps onto `var shadow = 1.0` instead of a zeroed sum, adding `1/(2N+1)^2` of
unshadowed light to every filtered fragment -- the umbra measured 93/255
instead of 7/255 at `shadow_pcf = 1`, a 13x over-brightening. If you have
older rendered output with `shadow_pcf > 0`, its shadows are too light.

### `shadow_normal_offset_scale: Optional[float]` — default `None` (automatic) *(live)*
Pushes the sample position along the surface normal before projecting into
light space, scaled by `k = 1 - N·L` so the offset grows at grazing angles:
`offset_pos = world_pos + world_normal * shadow_normal_offset_scale * k`.
`src/app/window.rs:205`, shader `mesh_shadow.wgsl:144-145`.
Accepted: any float. Too small leaves shadow acne; too large detaches shadows
from their casters (peter-panning).

### `shadow_bias_scale: Optional[float]` — default `None` (automatic) *(live)*
### `shadow_bias_minimum: Optional[float]` — default `None` (automatic) *(live)*
Depth-comparison bias, combined in the shader as
`bias = max(shadow_bias_scale * k2, shadow_bias_minimum)` where `k2 = (1 - N·L)^2`.
So `shadow_bias_scale` sets the angle-dependent term and
`shadow_bias_minimum` the floor applied to head-on surfaces.
`src/app/window.rs:203,204`.
Accepted: any float `>= 0.0`.

`None` fits both from the light frustum and the shadow map size, in units of
one texel's depth: **one** texel-depth for the slope term and one for the
floor. The slope factor was ten until it was measured against ray-traced
truth, at which point it turned out to be lighting the night side -- a crater
with the Sun below its plane reported up to 0.6 % of facets sunlit. See
`notes/2026-09-08_shadow_bias.md`.

**A receiver-plane term is applied per PCF tap**, on top of these. The
gradient `d(depth)/d(uv)` is derived from the facet normal and the layer
matrix, so a tap at `offset` compares against `depth + dot(offset, grad)` --
the depth the receiver actually has there. Without it, a receiver tilted in
light space self-shadows under the filter: the crater's lit wall darkened
39,219 px at `shadow_pcf = 4` that `shadow_pcf = 0` renders clean.

It is derived analytically, **not** from `dpdx`/`dpdy`. Screen-space
derivatives are meaningless across a facet boundary and on a flat-shaded mesh
every pixel is near one -- a `dpdx` version made the facet-edge leak 6x worse
(1,279 px against 215). It is also clamped, at `GRAD_MAX = 1e-4` in normalised
depth, because the gradient is only valid where the occluder *is* the
receiver; where a separate surface casts (the rim onto the floor) the
receiver's slope says nothing, and unclamped it pushed those taps out of
shadow, 215 -> 3,892 px.

**All three default to `None`, meaning automatic.** They are derived every
frame from the fitted light frustum and `shadow_resolution`, expressed
relative to one shadow texel so they stay correct at any scene scale. Setting
one pins it and leaves the others automatic; assigning `None` again restores
automatic. Derivation and measurements in `2026-08-25_renderer_auto_fit_wireframe/`.

**With `shadow_per_body` on, there is one set of these per layer**, derived
from that layer's own half-extent and the scene depth range
(`light.layer_bias[8]` in the uniform, read at `mesh_shadow.wgsl:231`). The
spread is large — on the Mars swing-by quick-look:

| layer | body | `side` | normal offset | bias scale | bias min |
|---|---|---|---|---|---|
| 0 | Mars | 5,758 km | 1.988 km | 5.7e-4 | 5.7e-5 |
| 1 | Phobos (10x) | 226.8 km | 0.0783 km | 2.3e-5 | 2.3e-6 |
| 2 | Deimos | 14.28 km | 0.0049 km | 1.4e-6 | 1.4e-7 |

403x between the largest and smallest. Note the depth range is the *same* for
every layer, since each slab spans the whole scene so occluders still cast:
self and mutual shadowing share a layer rather than getting separate
parameters.

**So pinning any of the three is now worse than leaving it automatic.** The
per-layer loop reads `config.shadow_normal_offset_scale.unwrap_or(fit.…)`, so
a pinned value replaces the fitted one on *every* layer — one number across
that 403x spread. Hand-tuning these to match the single Sun frustum was
correct before per-body layers existed; it is not any more.

You should not normally need to touch these -- they exist for debugging a
suspected shadow problem, not as part of ordinary setup.

**They were broken until 4 September**, and silently: the caller recovered the
layer's half-extent as `1.0 / view_proj.x_axis.x`, but `view_proj` is
`projection * view`, so that yields `side / |R[0][0]|` rather than `side`. With
the Sun where the swing-by puts it, Mars's `R[0][0]` fell below `f32::EPSILON`
and a `1.0` fallback took over: a 3,788 km body was biased as though it were
1 km across, giving a 0.35 m normal offset. The whole Mars disc rendered with
self-shadow acne that no automatic setting could clear, and `shadow_per_body =
False` did not visibly help because the single scene layer was mis-sized too.
`fit_light_view_proj` now returns its extents instead of having them
reverse-engineered from the matrix.

---

## Reference axes

### `axes: str` — default `"off"` *(live)*
Draw a measured frame around the scene, in one of four styles.

| | |
|---|---|
| `off` | nothing |
| `box` | closed box, ticked on the near edges — MATLAB's `box on`; every edge is a ruler |
| `panes` | the three far panes, gridded, ticks on their outer edges — matplotlib's `Axes3D`; reads as a room the body sits in, so the grid gives depth cues a bare box does not |
| `gizmo` | three labelled arrows at the origin and nothing else — for fly-throughs, where a box would occlude the subject every time the camera swings |
| `blender` | ground grid on XY with the Z axis picked out, as Blender's viewport |

Accepted: those names; anything else raises `ValueError` listing them.

### `axes_ticks: int` *(live)*
Roughly how many ticks per axis. The step is rounded to 1, 2 or 5 times a
power of ten first, so the count lands *near* this rather than on it — ticks
at 0.0347 are unreadable.

### `axes_unit: str` *(live)*
Appended to every tick label, e.g. `" km"`.

The renderer knows a mesh is 0.437 across but not whether that is metres or
kilometres, so the unit has to come from the script. **Nothing checks it**, so
a wrong unit here mislabels a figure silently.

### `axes_color: list[float]` *(live)*
### `axes_label_size: float` *(live)*
### `axes_label_color: list[float]` *(live)*
Line and grid colour `(r, g, b)`, tick label size in pixels, and label colour
`(r, g, b, a)`.

---

## Facet colouring from data

### `colormap` *(live)*
The colour table. Accepts a built-in name, any N×3 or N×4 array (alpha
ignored) in float32 or float64, or a sequence of `[r, g, b]` triples:

```python
app.simulation.config.colormap = "inferno"
app.simulation.config.colormap = matplotlib.colormaps["magma"](numpy.linspace(0, 1, 256))[:, :3]
app.simulation.config.colormap = kalast.app.colormap("inferno")[::-1]      # reversed
```

Any length works — it is resampled to 256 entries on upload, interpolated
rather than nearest, since nearest turned the 8-anchor built-ins into 8
visible bands. Obvious on a colour scale, which is a flat ramp with nothing
to hide behind.

Reading it back gives the stored table as an array. Defaults to greyscale, so
a mesh with values but no colormap set still reads as data rather than one
flat colour.

**`kalast.app.colormap(name)`** returns a built-in as a 256×3 array, and
**`kalast.app.colormap_names()`** lists them. That makes the built-ins data
rather than a string only the setter understands, so one can be reversed,
sliced or concatenated before use. An unknown name raises `ValueError`
listing the built-ins.

Until 7 September the setter took **only float32**, so the matplotlib call
in its own documentation failed — numpy's default is float64 — with
`'ndarray' object is not an instance of 'ndarray'`, which does not suggest a
dtype problem.

**There is no switch to turn data colouring on.** `color_mode = 1` — the
unlit mode — *is* the data map, for any mesh that carries `mesh.values`; a
mesh without values falls back to its vertex colours, which is what mode 1
always meant.

That is deliberate. Unlit is what a quantitative figure wants anyway, since
shading a data map makes one value read as two colours, so a separate
`value_mode` only created combinations that were either redundant or wrong.
One setting decides what you are looking at:

| `color_mode` | the surface shows | the colour bar shows |
|---|---|---|
| 0, 3 | diffuse lighting | lighting, 0..1 |
| 1 | the data | the data, `value_min`..`value_max` |
| 2 | one flat colour | nothing — the bar is not drawn |

### `value_min: float | None` — default `None` *(live)*
### `value_max: float | None` — default `None` *(live)*
Ends of the colour scale, or `None` to fit the data each frame.

**Pin both for anything comparative.** An automatic range silently rescales
between frames, so two images of the same scene are not on the same colour
scale and the difference between them reads as physics rather than as
bookkeeping.

---

## Colour bar

### `colorbar: bool` — default `False` *(live)*
Draw the colour scale over the render.

**What the bar describes follows `color_mode`**, and is not a setting of its
own: the legend cannot be made to describe something the surface is not. In
the lit modes it is the diffuse shading, `ambient + cos(i) * visibility` —
normalised direct insolation including shadowing, **not** radiance and **not**
temperature, and it carries the `ambient_strength` floor, so label it for what
it is. In the unlit mode it is the data map, over `value_min`..`value_max`,
read from the same lookup table the surface uses so the two cannot disagree.

With `color_mode = 2` the bar is not drawn at all: every body is one flat
colour, so there is no scale to label.

### `colorbar_label: str` *(live)*
Caption, e.g. `"Surface temperature (K)"`. Same warning as `axes_unit`:
nothing checks it against what is actually mapped.

### `colorbar_anchor: str` *(live)*
### `colorbar_x: float` *(live)*
### `colorbar_y: float` *(live)*
Placement, using the same nine anchors and inset convention as `Hud`.

### `colorbar_vertical: bool | None` — default `None` *(live)*
Orientation. `None` infers it from the anchor, which is right for the corners.

### `colorbar_length: float` *(live)*
### `colorbar_thickness: float` *(live)*
Long and short axis of the bar, in pixels.

### `colorbar_ticks: int` *(live)*
### `colorbar_text_size: float` *(live)*
### `colorbar_text_color: list[float]` *(live)*
Roughly how many numbered ticks — rounded to a readable step as the axes are —
plus label size and colour.

### `colorbar_border: bool` — default `True` *(live)*
Outline around the strip, so it reads as a scale rather than as part of the
scene when it sits over a dark body.

---

## Wireframe

Barycentric edge detection in the main fragment shader -- a single pass, so
the overlay cannot z-fight. Full write-up in `2026-08-25_renderer_auto_fit_wireframe/`.

**Requires flat meshes** (`load_mesh(..., flatten=True)`). The barycentrics
come from `vertex_index % 3`, which is only a triangle corner for
non-indexed geometry. Smooth meshes render shaded with a one-time warning
rather than noise; the check is per mesh, via `INSTANCE_FLAG_FLAT` in
`InstanceInput.flags`.

### `selection_color: list[float]` — default yellow `(1.0, 1.0, 0.0, 1.0)` *(live)*
The colour a facet takes when selected — by clicking it in the viewport, or
through `sim.toggle_facet`. Written onto the facet's own vertices together
with colour-mode 1, which the shader honours for that facet alone, so the rest
of the body keeps its shading. Deselecting restores what was there.

Live in the sense that it applies to the *next* selection: facets already
selected keep the colour they were given.

### `facet_labels: bool` — default `False` *(live)*
### `facet_labels_max: int` — default `2000` *(live)*
### `facet_label_size: float` — default `12.0` *(live)*
### `facet_label_color: list[float]` — default `(1, 1, 1, 0.9)` *(live)*
Draw each facet's index at its centre, for reading off which facet a number
in a data product refers to:

```python
app.simulation.config.facet_labels = True
```

Two limits, both deliberate. Only facets **turned towards the camera** are
labelled — the text is a screen-space overlay with no depth test, so labelling
the far side would print numbers over the surface hiding them. And no more
than `facet_labels_max` per body, because a label is a text draw and a shape
model has millions of facets; the count is per body and the rest are dropped
silently rather than the frame rate being.

Indices are the mesh's own facet indices, the same ones `sim.toggle_facet`
takes and `mesh.values`/`mesh.colors` are indexed by. Useful on `res/cube.obj`
(12 facets) to see which triangle is which before writing per-facet data.

### `wireframe_mode: u32` — default `0` *(live)*

| Value | Meaning |
|---|---|
| `0` | shaded mesh only |
| `1` | wireframe only -- edges kept, interior discarded |
| `2` | wireframe composited over the shaded mesh |

Accepted: `0`, `1` or `2`; anything else behaves as `0`.

### `wireframe_width: f32` — default `1.0` *(live)*
Line half-width in **pixels**. Screen-space, so thickness is constant
regardless of distance or zoom.
Accepted: any float `> 0`.

Note the visible blur depends on triangle size on screen: on a mesh finer than
the framebuffer (100k+ facets seen from far away) every fragment is within
`wireframe_width` of an edge and the body renders solid. Zoom in or use a
decimated mesh.

### `wireframe_color: wgpu::Color` — default `BLACK` *(live)*
Accepted: any 4-element sequence of floats — tuple, list or `numpy.array` —
as `(r, g, b, a)`; alpha is dropped. Same as `background`, `color` and
`light_color`.

It took a Python tuple *only* until 4 September, because it was typed as a
Rust tuple while the other three used `[Float; 4]`; an array extracts from any
sequence, a tuple does not. The getter now returns a list rather than a tuple,
for the same consistency.

Mode `2` blends the colour by edge coverage, so it is antialiased. Mode `1`
thresholds instead, because the pipeline blend state is `REPLACE` and a
fractional alpha would be ignored -- so mode `1` edges are aliased.

---

## Automatic frustum fitting

Not config fields -- these live on `app.simulation.camera.projection` and
`app.simulation.sun.projection` -- but they follow the same `None` = automatic
rule, so they are documented here.

`near`, `far` and `side` default to `None` and are fitted every frame to the
bounding box of all loaded bodies (`Eye::fit_projection`, `src/app/frame.rs`).
Set one to pin it; assign `None` to restore automatic.

```python
app.simulation.sun.projection.near          # -> None (automatic)
app.simulation.sun.projection.resolved_near # -> 7.70341 (what the fit chose)
app.simulation.sun.projection.near = 0.1    # pin it; far and side stay automatic
```

`fovy` and the projection mode are **not** automatic -- they are real
instrument properties, not scene-derived.

The orthographic (light) fit sizes itself from the bounding *sphere*, which is
rotation-invariant, and quantises to whole shadow texels so the shadow edge
does not crawl as the sun moves. `shadow_resolution` therefore feeds both the
fit and the derived bias.

**With `shadow_per_body` on (the default), the light fit runs per body**, not
once for the scene: each layer is sized to its own body while its depth range
still spans the scene. A pinned `sun.projection.side` therefore applies to
every layer, which is rarely what you want — pinning it is a way to defeat
exactly the sizing per-body layers exist to provide.

### `anchor_body`

Also not a config field. `camera.anchor_body = 0` keeps the anchor on body 0
as it moves; `None` (the default) leaves the anchor fixed. Assigning `anchor`
from a body matrix instead only captures where that body was at that instant,
which every animating script previously had to remember to redo by hand.

---

## Not currently wired up

- **`debug_simulation: bool`** — no reader anywhere in `src/`. A placeholder
  for future debug-print depth; settable, but nothing consumes it.

There are also commented-out light fields in `src/app/config.rs`
(`light_target`, `light_up`, `light_side`, `light_znear`, `light_zfar`) --
the light's framing is instead controlled through
`app.simulation.sun.projection.{side,near,far}` and `sim.sun.pos`, not
through the config.

**`sim.sun.look_anchor()` no longer belongs in that list.** Since
`shadow_per_body`, each layer aims itself from `sun.pos` at the body it
covers, so `sun.dir` and `sun.anchor` are ignored for shadowing. Older scripts
that call it still run; the call simply has no effect on the lighting.
