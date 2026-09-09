# Handoff, 9 September 2026 — macOS laptop, evening

Second handoff of the day. The morning one (`2026-09-09_HANDOFF.md`, written
on the Windows machine) is separate and still stands; this covers what the Mac
session did afterwards.

Everything below is committed. Nine commits, `ad6d300`..`f3c1baf`.

## What is done

**Facet index labels** (`ad6d300`). `config.facet_labels` draws each facet's
index at its centre, with `facet_labels_max` (2000), `facet_label_size` and
`facet_label_color`. Only facets facing the camera are labelled -- the text is
a screen-space overlay with no depth test, so labelling the far side prints
numbers over the surface hiding them. **Not wired into
`examples/cube/light.py`**, deliberately: two lines, and that file was being
edited at the time.

**A guard for the Python bindings** (`17f4c11`). The editor's panel and the
`.pyi` stubs are generated from `src/app/config.rs`; the `#[getter]`/`#[setter]`
pair is not. `tests/test_config_bindings.py` closes that gap and found two
options that were complete everywhere except from a script: `selection_color`
(documented in CONFIG.md as usable, never bound) and `colorbar_border` (a
checkbox in the editor, nothing in Python). Both bound now.

**GPU timestamp queries** (`4a362b2`), from the WebGPU samples' `timestampQuery`.
`config.gpu_timing = True`, then `sim.gpu_timings()` or the `{gpu}` HUD
placeholders. Read `notes/2026-09-09_gpu_pass_timings.md` before using the
numbers: the per-pass figures **overlap and must not be summed**, and they lag
the current iteration by a few frames (the dict carries `"frame"`).

**A near-plane regression, fixed** (`b0a1989`). Found from a screenshot of
z-fighting on the crater. `debug_light_cube_fit` merged the cube's box into the
scene bounds with a union; a box spanning the scene *and* the Sun has corners
in empty space, and the near plane was fitted to one of them -- 1.17 became
0.000028 against a far of 2.8. Each box is fitted from its own corners now, and
the near floor is `far * 1e-3` rather than `far * 1e-5`.

**`App::set_tick` is `App::set_before_render`** (`d797ec2`), to pair with
`set_after_render`. No alias: Rust had two callers, both in this repo. Python
keeps `tick` as an alias, as before.

## What was tried and did not work, so it is not tried again

**Collapsing the shadow submits** (`bb5e1cf`) is *implemented and kept*, and
bought nothing. The layer index reaches the shadow pass as a dynamic offset
now, so one encoder and one submit cover every layer instead of one submit
each. Measured both ends -- 12-facet cubes and Didymos + Dimorphos at 3,145,728
facets apiece -- and the frame time does not move either way.

So the ~1 ms an extra body costs is **per render pass, not per submit**.

**Multiview and the shadow atlas are therefore not worth building**, which
reverses the recommendation this session started with. They remove one pass out
of two, worth roughly half a frame on a cube scene and single-digit percent of
a 34.5 ms two-body frame. What *is* worth having already exists:
`shadow_path` proxies take the shadow passes from 28.5 ms to 6.5 and the frame
from 34.5 to 17.4, a 2x. Full table in the timing note.

The shadow pass is also quadratic in bodies -- every layer draws every mesh,
which is what keeps mutual shadowing. With two bodies both draws are needed.
With more, per-layer occluder culling is the thing to look at, not how the
passes are packaged.

## Work to do

Three items left from the WebGPU samples comparison, in the order I would take
them.

### 1. Reversed-Z — the one to do first

Every pipeline is `CompareFunction::Less` on a 0..1 depth range. Depth
precision is concentrated near the near plane, which is the wrong end: it makes
`near / far` the thing that decides whether distant coplanar surfaces fight,
and this session lost an afternoon to exactly that.

`b0a1989` treats the symptom -- a `far * 1e-3` floor is a patch that assumes no
scene needs a nearer near plane than a thousandth of its far one. A Hera close
approach with a 100 km far plane cannot see anything within 100 m of the
camera. Reversed-Z removes the assumption instead of tuning it.

What it touches:

- Depth format must be float (`Depth32Float`), not unorm -- reversed-Z is
  worthless on a fixed-point buffer. Check what `pass/depth.rs` and
  `render.rs` create.
- Every depth pipeline flips to `CompareFunction::Greater` and clears to `0.0`
  instead of `1.0`: the main pass, the shadow pass, the depth debug view, the
  facet-id pass, the hemicube passes.
- The projection matrix swaps near and far. `Projection::resolved()` feeds
  `perspective_rh`; the reversed form is a separate constructor, and the
  orthographic path (the shadow map) needs deciding on separately.
- Anything reading depth back must be checked: `facet_shadow.rs` compares
  sampled depth against a computed light-space depth, and the sense of that
  comparison inverts. `shaders/mesh_shadow.wgsl` samples the shadow map with
  its own bias arithmetic -- the biases are calibrated (see
  `2026-09-08_shadow_bias.md`) and will need re-checking, not just re-signing.

How to know it worked: the crater at the camera in `b0a1989`'s commit message
should render clean with the near floor put *back* to `far * 1e-5`. And
`examples/crater_self_shadow/step.py`'s illumination trace must not move --
`shadow_ref.py` in that commit's scratch is the shape of the check: lit
fraction and illumination sum per iteration over a Sun sweep, compared to
twelve decimals.

### 2. Primitive picking

Facet picking is a CPU ray, O(facets) per click, which at 3.1M facets is slow
enough to feel. The alternative already in the tree, `facet_id_map`, costs a
second geometry pass plus a whole-framebuffer readback -- much worse. The
sample's approach is a 1x1 readback at the cursor.

`src/app/facet_id.rs` already renders facet ids; the work is to make it
readable at one pixel rather than as a full map, and to point
`sim.pick_facet`/the click handler at it. Keep the CPU ray: it answers
questions a screen pixel cannot (a ray from an arbitrary origin, e.g. a
spacecraft boresight), and `pick_facet` is public API.

### 3. Occlusion queries

The Visibility panel counts bodies with AABB-versus-frustum tests, so "visible"
currently means "its bounding box overlaps the frustum". A body wholly behind
another still counts. Occlusion queries would make the count exact.

`wgpu::RenderPassDescriptor` already carries `occlusion_query_set`, which is
sitting at `None` in every pass; the plumbing is the same shape as the
timestamp work in `src/app/gpu_timing.rs`, including the readback discipline.

Smallest of the three and the least consequential -- it improves a diagnostic
panel, not a render or a measurement.

## Traps this session hit, worth knowing

- **The per-pass GPU numbers overlap.** Four bodies report 4.6 ms of shadow
  passes inside a 3.6 ms frame. Quote `span`, and treat per-pass figures as
  "what moved", never as a budget. Putting a `"total"` in the API was the first
  version of this and it was wrong in the way a debugging tool must not be:
  plausible, precise, and larger than the thing it was part of.
- **Timestamps go in at pass boundaries only.** This adapter has
  `TIMESTAMP_QUERY` and neither `..._INSIDE_ENCODERS` nor `..._INSIDE_PASSES`.
  Timing half a pass means splitting the pass.
- **`git revert -n` stages its changes**, so `git checkout -- .` does not undo
  it. `git restore --source=HEAD --staged --worktree <paths>` does. Worth
  knowing when A/B-ing a committed change: build the reverted tree, measure,
  restore, rebuild.
- **A frame export is the way to compare renders**, not a screen grab. Fix the
  iteration, redirect `export_dir` to a scratch path, and diff the PNGs; that is
  what proved the z-fighting predated the shadow work.
- **`facet_illumination` agreeing is not proof a render is unchanged.** It goes
  through the compute path, not the shading path. Check both.
