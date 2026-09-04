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
| `Escape` | any | Quit, flushing the frame-export queue first |
| `H` | any | Print camera pos / up / dir / anchor / projection |
| `P` | any | Toggle simulation pause |
| `T` | any | Toggle camera control, Arcball ⇄ WASD |
| `W` `A` `S` `D` | WASD | Move forward / left / back / right |
| `Space` | WASD | Move up |
| `Left Shift` | WASD | Move down |
| `Option` / `Alt` | Arcball | Held with a left-drag, stands in for the middle button — see below |
| `Left Shift` | Arcball | Held during a drag, pans instead of orbiting — see below |

### `Escape` — quit

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
require `config.debug_app`.

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
re-centres it.

### `W` `A` `S` `D`, `Space`, `Left Shift` — movement

Effective in WASD mode only. `Left Shift` specifically — `ShiftRight` is not
bound.

### `Left Shift` and `Option` — drag modifiers

Both are modifiers rather than actions: they change what a pointer drag does in
Arcball mode. `Left Shift` turns an orbit into a pan; `Option` makes a left
drag act as a middle drag. See the next section.

## Mouse and trackpad — Arcball (the default)

**With a three-button mouse:**

| Gesture | Does |
|---|---|
| Middle-drag | Orbit around `camera.anchor` |
| `Shift` + middle-drag | Pan; moves eye and anchor together |
| Scroll wheel | Zoom |

**With a trackpad**, or any pointer without a middle button:

| Gesture | Does |
|---|---|
| `Option` + click-drag | Orbit (`Option` is `Alt`; on a Mac keyboard the key is labelled ⌥) |
| `Shift` + `Option` + click-drag | Pan |
| Two-finger scroll | Zoom |
| Pinch | Zoom |

The `Option` substitution is gated on `config.emulate_middle_button`, which
defaults to `true` on macOS and `false` elsewhere, and matches Blender's
"Emulate 3 Button Mouse". Set it to `True` to get the same substitution on
Linux or Windows. It exists because a trackpad has no middle button, which had
made the arcball unusable on macOS; covered by the regression test
`alt_left_drag_substitutes_for_the_middle_button`.

The arcball reacts only during a drag, leaving the cursor free otherwise. Both
zoom gestures go through the same path: a wheel reports discrete notches and a
trackpad reports pixels, and they are normalised against each other
(`src/app/mod.rs:448-454`) so one sensitivity constant suits both. Before that
a notch was multiplied by 100 and fed to rotation, which limited a mouse to
large single-axis jumps.

## Mouse — WASD

The cursor is hidden and grabbed (`Confined`, falling back to `Locked`), so
every pointer motion is a look, with no button held. `T` returns to Arcball and
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

Sensitivities are config rather than bindings — `sensitivity_move`,
`sensitivity_look`, `sensitivity_rotate`, `sensitivity_zoom`, all default `1.0`
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
