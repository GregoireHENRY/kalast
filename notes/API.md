# Python API reference

Everything a script touches outside `app.config`, which has its own reference
in `CONFIG.md`. Undated in the filename because it is a living document — add
to it whenever something is exposed to Python.

Defined in `src/py/app/`. Rust types map to Python as `bool` → `bool`,
`u32`/`usize` → `int`, `f32`/`Float` → `float`, `String` → `str`, `Vec3`/`Mat4`
→ `numpy` arrays.

```python
app = kalast.app.App()
app.config...                 # see CONFIG.md
app.simulation.load_mesh(...)
app.before_render = before_render
app.start()                   # blocks until the window closes
```

## `App`

| | |
|---|---|
| `app.config` | the config object — `CONFIG.md` |
| `app.simulation` | the scene: bodies, camera, sun, state, HUDs |
| `app.before_render` | callback run before the frame is drawn |
| `app.after_render` | callback run after it is drawn |
| `app.tick` | alias for `before_render` |
| `app.start()` | creates the window and runs the loop; **blocks** |

Both callbacks take `(sim, dt)` and are optional. `dt` is the frame time in
seconds.

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
loop, and `app.config` cannot be touched from inside them (the app is already
mutably borrowed; you get `RuntimeError: Already mutably borrowed`).

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

The live HUD list — **the same objects as `app.config.huds`**, not copies.
Declare them once at setup, edit them per frame:

```python
app.config.huds = [kalast.app.Hud("{it}/{nit}"), kalast.app.Hud("")]

def before_render(sim, dt):
    sim.huds[1].text = f"epoch {utc}"
```

Text written here is still a template. A HUD left untouched keeps its text.
See `CONFIG.md` for placeholders, anchors, size, colour and font.

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

**The Sun ignores `dir` and `anchor`.** Since per-body shadow layers landed,
each layer aims itself from `sun.pos` at the body it covers, so `sun.pos`
alone determines the lighting. Older scripts calling `sun.look_anchor()` still
run; the call simply has no effect.

Use `set_control_none()` for a scripted render whose camera is placed from
SPICE, so a stray drag cannot move an instrument pointing. Note `T` still
switches out of it — see `CONTROLS.md`.

### `Eye.projection`

| | |
|---|---|
| `fovy`, `near`, `far`, `side` | `None` means *fitted automatically each frame* |
| `resolved_near`, `resolved_far`, `resolved_side` | what the fit actually chose |
| `set_perspective()`, `set_orthographic()`, `is_*()` | projection mode |

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
