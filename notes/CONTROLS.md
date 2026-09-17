# Keyboard and mouse reference

Every key and pointer gesture the render window responds to, what it does, and
where in the code it is handled. Undated in the filename because it is a living
document, like `CONFIG.md` — add to it whenever a binding is added.

Handled in `src/app/mod.rs` (`window_event` / `device_event`) and
`src/app/frame.rs` (`Controller`). Bindings are matched literally on
`winit::keyboard::KeyCode`; none are remappable or exposed to Python.

## Keys

| Key | Mode | Does |
|---|---|---|
| `H` | any | Print camera pos / up / dir / anchor / projection |
| `P` | any | Toggle simulation pause — the editor's Play/Pause button |
| `K` | any | Advance one iteration and hold — the editor's Step button |
| `F` | any | Toggle fullscreen — the same thing the green button does |
| `Shift`+`F` | any | Toggle focus mode — give the window to the renderer |
| `T` | any | Toggle camera control, Arcball ⇄ WASD |
| `W` `A` `S` `D` | WASD | Move forward / left / back / right |
| `Space` | WASD | Move up |
| `Left Shift` | WASD | Move down |
| `Option` / `Alt` | Arcball | Held with a left-drag, stands in for the middle button — see below |
| `Left Shift` | Arcball | Held during a drag or a two-finger swipe, pans instead of orbiting — see below |
| `Ctrl` | Arcball | Held during a two-finger swipe, zooms instead of orbiting — see below |

**The editor's Restart has no key, deliberately.** It clears the scene and
runs the script again -- a script driving its own `while app.running:` loop
is asked to end it first, `running` going false as it does at close -- so a keystroke would throw away a long run; it is worth
having to aim for the button.

### `F` — fullscreen

The same simple fullscreen `app.config.fullscreen` gives, and the same
thing the window's green button does; see `CONFIG.md`.

Worth having as a key because simple fullscreen hides the title bar, and with
it the button that got you there.

### `Shift`+`F` — focus mode

`AppConfig::focus`: the panels give the window to the renderer, each returning
when the pointer reaches its edge. The editor's own checkbox sets the same
field, so the key and the box cannot drift apart.

Beside plain `F` because the two are the same wish at different scopes — one
hides the panels, the other hides the desktop — and they compose: both on is
the renderer alone on the screen.

Independent of `app.config.fullscreen`, which is the OS window and
nothing else.

### `K` — one iteration

The two lines the editor's Step button runs: set `state.pause_at` one ahead
and clear `is_paused`, so the next frame advances and `Simulation::update`
holds it again on the mark. Same code, so the key and the button cannot drift
apart.

Nothing happens while a script has not run — there is no iteration to take.

### Quitting

**`Escape` is deliberately not bound.** It used to quit, which is a long run
thrown away by the key most often pressed to mean "stop what you are doing".
Closing the window does it instead, or ⌘Q.

`App::exit` blocks on `FrameExporter::finish()` before quitting, so every
queued frame reaches disk. Killing the process instead loses whatever is still
in the export pipeline — a killed background thread cannot be resumed. For any
run with export on, quitting by `Escape` and quitting by `kill` are not
equivalent.

Also releases the cursor if the camera is in WASD mode.

### `H` — print the camera state

```
camera: pos=[..] up=[..] dir=[..] anchor=[..] projection=Projection { .. }
```

`src/app/mod.rs:379`. Prints unconditionally — unlike `P` and `T`, it does not
require `config.debug.app`.

This is the supported way to recover a viewpoint reached by navigating: orbit
to the view, press `H`, and copy `pos` and `dir` into the script. A view found
interactively is otherwise lost when the window closes, since the camera state
is not persisted.

### `P` — pause

Calls `state.toggle_pause()`. Readable and writable from Python as
`sim.state.is_paused`.

Pauses the simulation, not the render: the window keeps drawing the paused
scene and stays responsive, so orbiting, zooming and `H` all still work. What
stops is `state.iteration` advancing and **both callbacks** — `before_render`
and `after_render` are skipped entirely while paused (`src/app/mod.rs:240,313`).
A script therefore does not need its own `is_paused` check to stop stepping; it
is simply not called.

The callbacks are gated deliberately. `Simulation::update` does nothing but
increment the iteration counter, so gating that alone left `P` with no
observable effect.

### `T` — camera control

Cycles `Control`: Arcball → WASD → Arcball. From `Control::None` it goes to
WASD, so `set_control_none()` prevents input from moving the camera but does
not prevent `T` from leaving that state.

Switching to WASD hides and grabs the cursor; switching back releases and
re-centres it. Either way the camera is **levelled** first -- roll taken out,
the side kept -- so the mode being entered starts with the world's pole at the
top of the screen, or the bottom if you arrived upside down.

### `W` `A` `S` `D`, `Space`, `Left Shift` — movement

Effective in WASD mode only. `Left Shift` specifically — `ShiftRight` is not
bound.

### `Left Shift`, `Option` and `Ctrl` — drag modifiers

All three are modifiers rather than actions: they change what a pointer
gesture does in Arcball mode. `Left Shift` turns an orbit into a pan, drag or
swipe alike; `Option` makes a left drag act as a middle drag; `Ctrl` makes a
two-finger swipe zoom. See "Mouse and trackpad" below.

## The navigation gizmo

Shown by `config.axes.style = "gizmo"` and `"blender"`, in the corner
`config.axes.style.gizmo_anchor` names. Six balls: `+X +Y +Z` filled and lettered,
`-X -Y -Z` as rings.

| Gesture | Does |
|---|---|
| Click a ball | Look straight down that axis, orthographically |
| Turn away from that view | Puts the projection back to what it was |
| Zoom in an axis view (wheel, pinch, `Ctrl` + swipe) | Zooms — by the *extent*, since distance means nothing to a parallel projection |
| Left-drag anywhere on the widget | Orbit — no middle button, no `Option` |
| Hover a ball | Lightens it; a ring fills in, to show it can be clicked |

**Left-drag orbits here and nowhere else.** Everywhere else on the image a
plain left-drag is not a camera gesture: orbiting needs the middle button, or
`Option` where `controls.emulate_middle_button` is on. Requiring a modifier over a
widget whose whole point is being clickable is not a gesture anyone would
find, and Blender's gizmo does not either.

**The gizmo takes the click before the scene does.** A press anywhere on the
widget starts a gizmo gesture rather than a facet pick, so the axis views stay
reachable over a body. Clicks elsewhere are unaffected — the widget is a
`axes.gizmo_size`-radius disc in one corner and nothing outside it changes.

**Clicking an axis also switches to orthographic**, which is Blender's
behaviour and for the reason `Eye::view_along` was written: a plane view read
in perspective is not measurable, near rim and far rim being at different
scales.

**The orthographic is borrowed, not kept.** The projection in force beforehand
is remembered, and the first rotation away from the axis puts it back — so a
glance down an axis costs nothing and there is no mode left behind to notice
and undo. Clicking straight from one plane view to another does not overwrite
that memory, so the way back is still the projection you started from. Kept in
`App::snap` and restored beside the camera update in `src/app/mod.rs`.

**The wheel works there too**, which it did not before. Zoom moved the eye
along its own view direction, and a parallel projection does not care where
along that line the eye sits — so scrolling in a plane view was simply dead.
It now scales the projection *extent*, pinning it on the first notch (it is
fitted to the scene until then) and releasing it again on the way out.

**And the ground grid turns to face you.** The shaded grid is on whichever of
the three planes the view is down: XY looking along Z, YZ along X, XZ along Y.
Fixed to XY, a side view showed the ground edge-on with nothing behind the
body at all.

The negative balls are not decoration: `-Z` looks *up* at the scene from
underneath, which `sim.camera.view_along("z")` cannot reach on its own. In
Rust that is `Eye::view_along_from(axis, positive, bounds, orthographic)`.

Handled in `src/app/mod.rs` (`window_event`, `view_along_ball`) with the
layout and hit-testing in `src/app/gizmo.rs`, which is pure arithmetic on the
camera basis and is unit-tested — the same `Gizmo` is used to draw the widget
and to test a click, so the picture and the click target cannot drift apart.

## Selecting facets

| Gesture | Does |
|---|---|
| Click on the scene | Select the facet under the pointer; click it again to drop it |

A *click*, not a drag: press and release within four pixels. Anything longer
is a camera gesture, so `Option` + drag still orbits and a plain drag is still
free for whatever the control mode does with it.

Not on the navigation gizmo, which takes a press over itself first — see
above.

The facet turns `config.selection.color` (yellow by default) and is drawn
unlit, while the rest of the body keeps its shading — the shader honours a
per-facet colour mode, so one facet can be marked without flattening the whole
render. Deselecting restores whatever the facet's vertices had.

Each click prints the facet index, the exact intersection in world *and* body
coordinates, its latitude and longitude, and the whole selection so far:

```
selected body 0 facet 107
  hit world -0.154289 -0.389472 -0.106714
  hit body  -0.154289 -0.389472 -0.106714   lat -14.2914 lon -111.6110
  selected (1): 0:107
```

The editor's **Selection** panel lists them, drops one with ×, clears all, and
adds one by index — which is the only way to reach a facet that is facing away
from the camera.

From a script: `sim.selected_facets`, `sim.toggle_facet(body, facet)`,
`sim.clear_selection()`, and `sim.pick_facet(origin, direction)` for the ray
test on its own. See `API.md`.

**On an indexed mesh the colour bleeds into the neighbouring facets**, because
they share the three vertices being painted. Load with `flatten=True`, which
per-facet work wants anyway.

## Mouse and trackpad — Arcball (the default)

**With a three-button mouse:**

| Gesture | Does |
|---|---|
| Middle-drag | Orbit around `camera.anchor` |
| `Shift` + middle-drag | Pan; moves eye and anchor together |
| Scroll wheel | Zoom |
| Left-drag on the navigation gizmo | Orbit — see above |

**With a trackpad**, or any pointer without a middle button:

| Gesture | Does |
|---|---|
| Two-finger swipe | Orbit |
| `Shift` + two-finger swipe | Pan |
| `Ctrl` + two-finger swipe | Zoom |
| Pinch | Zoom — 1:1, fingers opening to twice the spread bring the scene twice as close |
| `Option` + click-drag | Orbit (`Option` is `Alt`; on a Mac keyboard the key is labelled ⌥) |
| `Shift` + `Option` + click-drag | Pan |

This is Blender's default trackpad map, and the trackpad is told from a wheel
the way Blender tells it: by what the event reports. A wheel reports notches,
a trackpad reports pixels (`hasPreciseScrollingDeltas` on macOS, which winit
keeps as `LineDelta` against `PixelDelta`), and `Controller::scroll` routes on
that alone. So a Magic Mouse counts as a trackpad, as it does in Blender;
`config.controls.trackpad_orbit = False` makes any pixel-reporting surface
zoom like a wheel again, the behaviour before 17 September.

A swipe orbits as far as a drag of the same length: the pixels arrive
physical and a drag arrives in points, so the swipe is divided by the window's
scale factor. macOS keeps sending scroll events after the fingers lift
(momentum), and winit does not mark them, so a flick keeps orbiting a little
-- Blender does the same. With "natural" scrolling off in System Settings the
swipe runs the other way; there is no switch for that yet.

The `Option` substitution is gated on `config.controls.emulate_middle_button`, which
defaults to `true` on macOS and `false` elsewhere, and matches Blender's
"Emulate 3 Button Mouse". Set it to `True` to get the same substitution on
Linux or Windows. It exists because a trackpad has no middle button, which had
made the arcball unusable on macOS; covered by the regression test
`alt_left_drag_substitutes_for_the_middle_button`.

The arcball reacts to a drag, a swipe and a zoom, and to nothing else, leaving
the cursor free. Wheel notches and `Ctrl` + swipe pixels are normalised against
each other in `Controller::scroll` (`PIXELS_PER_NOTCH`, fifty) so one
`sensitivity_zoom` suits both; before that a notch was multiplied by 100 and
fed to rotation, which limited a mouse to large single-axis jumps. The pinch
did nothing in the editor until 17 September: egui consumed it before the
camera saw it, and where it did get through (a bare window) it was fed in as
notches, so a whole pinch moved the eye 11%.

**Either way up, the scene follows the pointer.** The turntable spins about
world up, so with the view upside down the same drag moved the camera to the
same *world* side -- the opposite side of the screen -- and a drag to the right
turned the scene to the left. Blender reverses its orbital term when world-Z
points down the screen (`vod->reverse`), and `Eye::arcball_update` does the
same from the sign of `up · up_world`. The vertical term needs nothing, its
axis being the screen's own right. Since 17 September; before that the flip
happened at the pole.

**And the orbit is level.** Every orbit ends with `Eye::level()`: `up` is put
back to world-up as seen from the view direction, on the side it already was.
Roll used to get in two ways and then stay -- the WASD look yawed about the
camera's own up, and a new `anchor_body` re-aimed the camera while `fix_up`
kept whatever `up` the old elevation had left -- so the turntable's pole sat
off the top of the screen and the orbit ran tilted. A script's `up` is still
its own: nothing levels until you take hold of the camera.

## Mouse — WASD

The cursor is hidden and grabbed (`Confined`, falling back to `Locked`), so
every pointer motion is a look, with no button held. Looking around yaws about
world up and pitches about the screen's right, so it cannot roll the horizon;
until 17 September it yawed about the camera's own up, and a pitch followed by
a look around tilted the horizon a little every time -- the tilt that met you
on switching back to the arcball. `T` returns to Arcball and
releases the cursor; `Escape` quits and also releases it.

## Setting the mode from Python

```python
app.simulation.camera.set_control_arcball()   # default
app.simulation.camera.set_control_wasd()
app.simulation.camera.set_control_none()      # ignore all camera input
app.simulation.camera.control_toggle()        # same as pressing T
```

Use `set_control_none()` for a scripted render whose camera is placed from
SPICE, so a stray drag cannot move a camera that represents an instrument
pointing. See the `T` entry for its one limitation.

Sensitivities are config rather than bindings — `config.controls.sensitivity_move`,
`controls.sensitivity_look`, `controls.sensitivity_rotate`, `controls.sensitivity_zoom`, all default `1.0`
and all startup-only. See `CONFIG.md`.

## Details

- **Keys are matched on physical position** (`PhysicalKey::Code`). On AZERTY
  the movement keys are where QWERTY's W/A/S/D physically sit, not where the
  letters are printed.
- **Every key reaches the movement controller first.** `handle_key` runs before
  the `match`, in all modes. It only has an effect in WASD mode, but it means
  W/A/S/D are not available for other bindings.
- **Changing a binding means editing the `match`** in `src/app/mod.rs`; there
  is no configuration path.
