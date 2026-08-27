# `app.config` reference

Every option on the app config, what it accepts, what it does, and where in
the code it takes effect.

Defined in `src/app/config.rs` (`Config` struct + its `Default` impl).
Exposed to Python in `src/py/app/config.rs` -- every field has a getter and a
setter, so all of them are readable and writable as `app.config.<name>`.

```python
app = kalast.app.App()
app.config.width = 1020
app.config.vsync = False
print(app.config)          # __repr__ dumps the whole struct
```

**Timing matters -- most options are read once.** The config is consumed when
the window is created, inside `app.start()`. Set everything before calling it.

- *(live)* -- re-read by the renderer every frame. `Globals` is rebuilt and
  re-uploaded each frame (it has to be, since the automatic shadow constants
  change as the scene moves), so all the shading and shadow settings are read
  fresh; `background` and the debug-draw flags are read straight from the
  config during the pass.

  **This does not mean you can animate them from Python.** `app.config` cannot
  be touched inside `before_render`/`after_render` -- the app is already
  mutably borrowed for the duration of the callback, so any access raises
  `RuntimeError: Already mutably borrowed`. In practice you still set
  everything before `app.start()`. What *(live)* buys you is that the renderer
  picks up values derived internally per frame, which is what makes automatic
  shadow fitting possible.
- *(startup only)* -- read once during setup and baked into GPU resources or
  window state. Assigning to these after `start()` still updates the
  Python-visible field but **has no effect on rendering**. These are the ones
  that size a GPU resource (`width`, `height`, `shadow_resolution`), pick a
  pipeline (`render_back_face`), configure the surface (`vsync`, `title`), set
  up the exporter (`export_dir`, `export_sync`, `export_max_queued`), or are
  copied to the controller once by `apply_config_at_start`
  (`src/app/mod.rs:66`) -- the four `sensitivity_*` values.

If you need to vary a startup-only parameter, do it across runs, not from a
callback.

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

---

## Window

### `title: String` — default `"kalast"` *(startup only)*
The OS window title. Applied at `src/app/mod.rs:117` via winit's
`.with_title()`.
Accepted: any string.

### `width: u32` — default `800` *(startup only)*
### `height: u32` — default `600` *(startup only)*
Initial window size in pixels, and therefore the render-target size and the
resolution of exported PNGs. The exporter reads the surface dimensions, not
these fields directly -- see `src/app/window.rs:450`, which passes
`self.surface_config.width/height` into `export_frame`.
Accepted: any positive integer within what the GPU allows.
Note exported frame size follows the *live* surface size, so resizing the
window mid-run changes the size of subsequent exports. Export buffers are
pooled by byte size, and stale-sized pooled buffers are discarded on resize
(`src/app/gpu.rs`, the `pool_rx.try_recv()` loop in `export_frame`).

### `render_back_face: bool` — default `false` *(startup only)*
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

### `vsync: bool` — default `true` *(startup only)*
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

### `export_dir: String` — default `"out/frames"` *(startup only)*
Destination directory, created if absent (`src/app/gpu.rs`,
`FrameExporter::new`). Numbering **resumes after any files already present**,
so an existing run's frames are never overwritten -- the directory is scanned
once at startup for the highest numeric filename.
Accepted: any path string. Forward slashes work on Windows.

Give dev/test runs their own directory. Two `FrameExporter`s pointed at the
same directory race on both the startup index scan and any cleanup, so an
`rm -rf` of one process's directory can delete files another just wrote.

### `export_sync: bool` — default `false` *(startup only)*
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

### `export_max_queued: u32` — default `64` *(startup only)*
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

### `emulate_middle_button: bool` — default `true` on macOS, `false` elsewhere *(startup only)*
Treat **alt + left-drag** as a middle-drag, so the arcball can be orbited on
hardware with no middle button -- a trackpad. Blender calls the same setting
"Emulate 3 Button Mouse", and the binding matches, so the muscle memory
carries over. The real middle button keeps working either way; turning this
off only makes alt + left inert.
Accepted: `True` / `False`.
Copied onto the controller once by `apply_config_at_start`, like the
`sensitivity_*` values, so set it before `app.start()`.

### `sensitivity_move: Float` — default `1.0` *(startup only)*
Translation speed. `src/app/frame.rs:197`.

### `sensitivity_look: Float` — default `1.0` *(startup only)*
Look/pan speed, scaling `SENSITIVITY_LOOK`. `src/app/frame.rs:205,209`.

### `sensitivity_rotate: Float` — default `1.0` *(startup only)*
Orbit speed, scaling `SENSITIVITY_ROTATE`. `src/app/frame.rs:178,182`.

### `sensitivity_zoom: Float` — default `1.0` *(startup only)*
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
The light's color, feeding both the ambient and diffuse terms. Passed at
`src/app/window.rs:231` as part of the `Light` uniform.
Accepted: `(r, g, b, a)`; alpha dropped.

### `light_cube_scale: Float` — default `0.25` *(live)*
Size of the debug light cube, in world units. Only visible when
`debug_light_cube_show` is on. Applied in the vertex shader at
`shaders/light_render.wgsl:47`:
`vertex.pos * light_cube_scale + light.pos`.
Accepted: any float.

---

## Shadows

The shadow map is a depth texture rendered from the light's point of view,
then compared against during the main pass. Sizing happens at
`src/app/window.rs:239-240`; the sampling happens in
`shaders/mesh_shadow.wgsl:153-172`.

### `shadow_resolution: u32` — default `8192` *(startup only)*
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

**All three default to `None`, meaning automatic.** They are derived every
frame from the fitted light frustum and `shadow_resolution`, expressed
relative to one shadow texel so they stay correct at any scene scale. Setting
one pins it and leaves the others automatic; assigning `None` again restores
automatic. Derivation and measurements in `2026-08-25_renderer_auto_fit_wireframe/`.

You should not normally need to touch these -- they exist for debugging a
suspected shadow problem, not as part of ordinary setup.

---

## Wireframe

Barycentric edge detection in the main fragment shader -- a single pass, so
the overlay cannot z-fight. Full write-up in `2026-08-25_renderer_auto_fit_wireframe/`.

**Requires flat meshes** (`load_mesh(..., flatten=True)`). The barycentrics
come from `vertex_index % 3`, which is only a triangle corner for
non-indexed geometry. Smooth meshes render shaded with a one-time warning
rather than noise; the check is per mesh, via `INSTANCE_FLAG_FLAT` in
`InstanceInput.flags`.

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
Accepted: `(r, g, b, a)` floats; alpha is dropped.

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

---

## Not currently wired up

- **`debug_simulation: bool`** — no reader anywhere in `src/`. A placeholder
  for future debug-print depth; settable, but nothing consumes it.

There are also commented-out light fields in `src/app/config.rs`
(`light_target`, `light_up`, `light_side`, `light_znear`, `light_zfar`) --
the light's framing is instead controlled through
`app.simulation.sun.projection.{side,near,far}` and `sim.sun.pos` /
`sim.sun.look_anchor()`, not through the config.
