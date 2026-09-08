# Handoff — the cube examples

Picked up at the end of a long session and stopped mid-task. Two things were
asked; one is half done and unverified, the other is unanswered.

## 1. `debug_light_cube_fit` — written, never seen

**Asked for:** an option to include the debug light cube in the camera's
automatic frustum fit, so that turning it on in `examples/cube/light.py` shows
the cube orbiting the mesh instead of being clipped away.

**Done:** `config.debug_light_cube_fit`, committed. When it and
`debug_light_cube_show` are both on, `Window::update` unions the light cube's
box into the bounds the *camera* is fitted to. The light's own fit keeps the
plain body bounds on purpose — its frustum is what the shadow map covers, and
stretching it to the Sun would spend the whole map on empty space and leave
the bodies a few texels across. Documented in `CONFIG.md`; the panel is
regenerated; `cargo test --lib` and the guard tests pass.

**Not done, and the reason to be careful:**

- **Nobody has looked at it.** It compiles and it is plausible; that is all.
  Run `examples/cube/light.py` with `debug_light_cube_fit = True` and confirm
  the cube is actually on screen through a full orbit.
- **The example does not turn it on yet.** That was the point of the request
  — `light.py` should set it, or the option shows nothing to anyone who opens
  the example.
- **Check what the longer depth span costs.** The far plane moves from the
  cube's edge to the Sun at `ds = 5.0`, so near-to-far grows by about an order
  of magnitude. Z-fighting on the cube's own faces is the thing to look for,
  and if it appears, say so in the option's doc rather than quietly living
  with it.
- The half-extent used for the cube's box is `light_cube_scale`, taken as a
  half-size. If the cube is actually `light_cube_scale` across rather than
  either side of the Sun, the fit is generous by 2x — harmless for a far
  plane, wrong in the comment.

## 2. "What is the updated way of colouring each facet?" — unanswered

Asked about `examples/cube/colors.py`, which writes per-vertex colours
directly:

```python
mesh.colors[iif * 3 + k, :] = color        # k = 0, 1, 2
```

That still works, and for a *flattened* mesh it is still the way to give each
facet an arbitrary colour. What has changed around it, and what the answer
should cover:

- **`mesh.values` + the colormap** is now the way to colour facets from a
  *scalar* — one number per facet, `color_mode = 1`, and the colour bar and
  the surface read the same lookup table so they cannot disagree. See
  `API.md`, "mesh.values — colouring facets from data". `colors.py` predates
  it and paints a ramp by hand, which is exactly what `values` does properly.
- **`mark_colors_dirty()`** is needed when colours are written *after* the
  window opens. `colors.py` writes before `app.start()`, so the first upload
  carries them and it gets away without it.
- **Per-vertex `color_mode` is now honoured by the shader** (added this
  session for facet selection): a facet whose vertices carry mode 1 is drawn
  unlit in its own colour whatever the global mode is. That is a third way to
  mark individual facets, and the one that does not require the whole body to
  be unlit.
- `update_all_vertices_colors(mode, color)` sets every vertex at once.

The honest answer is probably "three ways, and which one depends on whether
you have a scalar, an arbitrary colour, or a handful of facets to mark" — and
then `colors.py` should demonstrate whichever is now the recommended one.

## State of the tree

`examples/crater_self_shadow/step.py` has one uncommitted change: a blank
line. Left alone deliberately — it is the user's edit, not mine.
