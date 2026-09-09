# Handoff — the Blender axes: an infinite grid and a navigation gizmo

Windows machine, 9 September, second session of the day. Everything below is
committed and pushed. The earlier `2026-09-09_HANDOFF.md` still stands for its
open decisions; nothing here touches them.

Three commits:

| | |
|---|---|
| `b34f5d1` | the ground plane of `axes = "blender"` is an infinite grid, shaded per pixel |
| `27aaa1c` | stop drawing the old segment grid underneath it |
| `4889b95` | the axis gizmo moves from the origin to a corner and becomes a control |

## The grid

`axes = "blender"` used to draw its ground plane as line segments: they
stopped at the scene bounds, sat at one spacing whatever the zoom, and were
one pixel wide because WebGPU has no line width. It is now a full-screen pass
that intersects the view ray with `z = 0` and asks, per pixel, how close that
point is to a grid line — Blender's viewport technique, and the one the WebGPU
samples call "pristine grid". `config.grid = False` restores the segments.

Three levels a factor `grid_major` apart are drawn at once, weighted so the
stack is continuous as it shifts: the finest fades out, the middle is always
solid, the coarsest fades in. **Two levels was the obvious version and is
wrong** — the coarse level arrives at the top of the stack at full strength
and pops. The middle level's colour walks from `grid_major_color` to
`grid_color` over the same blend, so when the stack shifts by one the line
changing hands does not change shade either.

### Four things had to be right, and only the first was obvious

Each was found by rendering at 4, 40 and 400 units and looking, not by
reasoning. Worth keeping because each failure looked like a different bug from
its cause.

1. **The line width is clamped to at least a pixel and at most half a cell.**
   Without the upper clamp a grazing view fills in solid: at 40 units the grid
   came out as a grey wash with black diamonds in it. The derivative along the
   direction running to the horizon is enormous, so the "line" grows to cover
   the cell. Past half a cell the coverage is mixed toward the average line
   density instead, so it greys out smoothly. This is Ben Golus, *The Best
   Darn Grid Shader (Yet)*; the naive `distance / fwidth` has neither clamp.

2. **`fwidth` is taken once on the world position and divided per level.**
   Taken on the already-divided cell coordinate it reads the derivative across
   a level boundary, where the divisor itself jumps, and speckles the whole
   plane with bright dots.

3. **Depth is clamped, not clipped.** The near and far planes are fitted to
   the *bodies*, so at 40 units out the frustum is a slab a couple of units
   deep and the grid was clipped to a narrow band across the middle of the
   screen with black above and below. Those hard edges were the near and far
   planes, not the horizon — which is the thing that took longest to see. Past
   the far plane it now pins to the farthest depth, so every body still
   occludes it; nearer than the near plane it pins to the nearest, so it
   occludes them, which is what ground between you and the scene should do.
   The pass writes no depth, so nothing downstream inherits a pinned value.

4. **The fade is on obliquity, not distance.** An infinite plane is bounded on
   screen by the horizon, so a distance fade never reaches its ramp. Two
   versions failed the same way — in world units, and as a fraction of the far
   plane — and both did nothing at all: the band stayed at full brightness
   right up to the cut. It fades on the angle between the view ray and the
   plane normal, which needs no idea of the scene's scale.

### And one that was hiding all of them

`gpu::RenderPipeline::new` hardcoded `BlendState::REPLACE`. The grid's alpha
*is* its antialiasing, so it was being thrown away: lines came out flat, and
wherever coverage saturated the plane filled with solid colour. There is a
`RenderPipeline::blended` now, and it is the only caller — everything else in
the renderer really is opaque.

## The gizmo

Six balls in a corner — `+X +Y +Z` filled and lettered, `-X -Y -Z` as rings —
laid out from the camera basis, drawn back to front and darkened with depth,
so the ordering itself says which way the scene is turned.

The three arrows it replaces stood at the world origin, and were only readable
when the origin was in shot and not behind the body, grew and shrank with the
zoom, and could not be clicked. **Nothing replaces them in the scene**: under
`"blender"` the grid already draws coloured X and Y lines through the origin
and the Z axis is picked out as a vertical line.

Gestures are in `CONTROLS.md`, options in `CONFIG.md`. The two decisions worth
recording:

- **Clicking an axis also switches to orthographic**, which is Blender's
  behaviour and the reason `Eye::view_along` was written: a plane view read in
  perspective is not measurable. Since no key is bound to the projection,
  **clicking the axis already being looked along toggles back to
  perspective** — that is the only way back, and it is why the toggle exists.
- **Left-drag orbits on the widget and nowhere else.** Everywhere else needs
  the middle button, or `Option` where `emulate_middle_button` is on. Asking
  for a modifier over a widget whose whole point is being clickable is not a
  gesture anyone would find.

The negative balls are not decoration: `-Z` looks *up* at the scene from
underneath, which `view_along` could not reach. It grew a `positive` argument,
in Rust (`view_along_from`) and in Python.

`src/app/gizmo.rs` is pure arithmetic on the camera basis and the image size —
no mesh, no camera matrix, no depth buffer. The same `Gizmo` draws the widget
and hit-tests a click, so the picture and the click target cannot drift apart,
and it is unit-tested on its own.

Drawn as screen-space quads with the disc and the ring cut out in the fragment
shader, which antialiases at any size where a triangle fan shows its facets
and needs a segment count chosen against the radius. **The balls are opaque
and shaded by depth rather than faded by alpha**: dimming with alpha let the
stems show through the balls covering them, and would have let a bright body
show through a control that has to stay legible over whatever it is drawn on.

**Its letters go into exported frames whether or not `export_hud` does**,
because the balls they sit on already did — the widget is drawn in the render
pass, into the texture the exporter copies, and lettered balls with no letters
on them read as a bug. Everything else in `export_hud`'s remit is unaffected.

## Traps for whoever touches this next

- **WGSL errors only surface at run time.** `target` is a reserved keyword and
  cost a build-and-run cycle to find. Nothing in the test suite compiles a
  shader; `cargo check` will not tell you.
- **A `.wgsl` edit needs the crate rebuilt.** `include_wgsl!` embeds the file
  at compile time. Cargo does track it, but `maturin develop` has to be re-run
  before a Python script sees the change — otherwise the old shader runs and
  the error messages point at line numbers that no longer exist.
- **`RenderPipeline::new` is opaque.** Use `blended` for anything whose alpha
  means something. See above for what the default costs.
- **`config.axes.has_gizmo()` gates drawing *and* hit-testing.** A style added
  to one and not the other gives a widget that cannot be clicked, or clicks
  that hit a widget nobody can see.
- **The `.pyd` is held by every running kalast window**, not just by VS Code's
  language server, so `maturin develop` fails while one is open. Find the
  holder rather than guessing:

      Get-Process | ? { $_.Modules.FileName -contains 'C:\projects\kalast\kalast\_rs.pyd' }

## How the interaction was verified, and how to do it again

Rendering a frame proves the widget is drawn; it says nothing about whether a
click reaches it. The whole chain — winit event, cursor-to-image mapping,
hit test, camera — was driven from PowerShell against a running window.

**Posted window messages, for a click.** `PostMessage` of `WM_MOUSEMOVE`,
`WM_LBUTTONDOWN`, `WM_LBUTTONUP` with the client position in `lParam` goes
through winit's window proc exactly as a real click does, and touches neither
the cursor nor the focus. This is the tool to reach for: it is safe on a
machine someone is using.

**Raw motion, for a drag, needs the foreground.** The arcball orbits from
`DeviceEvent::MouseMotion`, which is `WM_INPUT`, and raw input only reaches a
focused window — so a drag cannot be posted and needs `SendInput`. Windows
refuses the foreground to a background process: `SetForegroundWindow`,
`SwitchToThisWindow` and `AttachThreadInput` were all refused in turn. What
works is the **target** granting it, with
`ctypes.windll.user32.AllowSetForegroundWindow(-1)` in the test script. Save
and restore the cursor position and the previous foreground window around the
injection, and check `GetForegroundWindow` before injecting anything — abort
if it is not yours, or the clicks land in whatever the user is doing.

The driving script prints the ball layout after every camera change, which is
needed: each snap re-orients the widget, so click coordinates computed before
a snap are stale afterwards. Two of the first results looked like bugs and
were only stale aim.

What that showed: `+X` gives `dir=(-1,0,0) up=(0,0,1)` orthographic; clicking
it again drops orthographic without moving the camera; `+Z` gives
`dir=(0,0,-1)`; `-Z` gives `dir=(0,0,1)`; a left-drag from the widget produced
327 incremental camera changes and selected nothing; a click on the body still
picked a facet.

## Open

- **Not driven in the editor.** The widget is laid out in image pixels and the
  editor letterboxes the scene inside a viewport panel, so the cursor mapping
  matters there. It is the same `cursor_in_image` that facet picking already
  uses in the editor, which is why it is expected to work — but the gizmo was
  not clicked inside a panel.
- **The gizmo is burned into exported frames**, which is right for a figure
  and arguable for a data product. There is no "on screen only" option; the
  way to keep it out of an export today is `axes = "off"`, which also removes
  the grid. If the movie exports want the grid without the widget, that is a
  flag to add.
- **No key is bound to the axis views or to the projection.** The gizmo is the
  only way to reach either without a script. `CONTROLS.md` has a free-key
  problem — W/A/S/D are taken in all modes — but the numpad is untouched and
  is what Blender uses for exactly this.
- **`view_along(..., positive=False)` is committed but not in the built
  wheel.** Two kalast windows were open at the end of the session and had the
  `.pyd` locked, so the last `maturin develop` failed. Everything else here is
  in the wheel that was running. **Run `uv run maturin develop --uv` first
  thing.**
- The thin/thick grid hierarchy is 111 against 157 in sRGB, which is a subtle
  difference at a glance. Both are config, so this is a default to argue about
  rather than a limitation.
