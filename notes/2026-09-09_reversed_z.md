# Reversed-Z — 9 September 2026

The first of the three items `2026-09-09_HANDOFF_gpu_timing.md` left open, and
the one it recommended doing first.

The camera's depth buffer now stores **1.0 at the near plane and 0.0 at the
far one**, cleared to 0.0 and tested with `Greater`. The shadow map is
deliberately left as it was.

## Why

Depth precision used to be a function of `near / far`, and `b0a1989` had just
spent an afternoon on the consequence: the crater's flat plane and the bowl's
back face, nearly tangent where they meet, z-fought into a dashed grey line
once a bad fit put `near` at 2.8e-5 against a far of 2.8.

That commit fixed the fit and then floored `near` at `far * 1e-3` as a second
guard. The floor works, but it is an assumption rather than a fix: no scene may
need a near plane closer than a thousandth of its far one. A Hera close
approach with a 100 km far plane could therefore see nothing within 100 m of
the camera. That is a real configuration ruled out to protect against a
numerical artefact.

Reversed-Z removes the artefact. Float precision bunches up near zero; the
perspective divide bunches it near the near plane. Point them at opposite ends
and they cancel, so the ratio stops mattering. The floor is back to
`far * 1e-5` and is now only there to keep the projection non-singular.

## What the change is

Two constants say the sense, and every depth site reads them
(`src/app/gpu.rs`):

    DEPTH_COMPARE  = Greater   DEPTH_CLEAR  = 0.0    // the camera
    SHADOW_COMPARE = Less      SHADOW_CLEAR = 1.0    // the shadow map

`RenderPipeline::new` grew a `depth_compare` argument so the shadow pass can
opt out at its call site rather than by accident. The two hand-built pipelines
-- `facet_id.rs` and `hemicube.rs` -- take `DEPTH_COMPARE` directly.

The projection is `Mat4::perspective_rh` **with the two planes handed to it
swapped**, wrapped as `frame::perspective_rh_reversed`. No separate matrix form
is needed: `perspective_rh` puts a point `d` in front of the eye at
`f(n - d) / (d(n - f))`, and substituting `n` for `f` gives
`n(f - d) / (d(f - n))`; the two sum to 1 for every `d`, so the swap *is*
`z' = 1 - z`. `orthographic_rh_reversed` is the same swap, for the camera in
orthographic mode -- which gains no precision, being linear, but has to agree
with the pipeline it shares.

The depth format was already `Depth32Float`. Reversed-Z on a fixed-point buffer
would have been worthless, since evenly spaced values have nothing to cancel.

## The shadow map is not reversed, on purpose

The handoff listed the shadow pass among the pipelines to flip. It should not
be, for two reasons that point the same way:

- **It gains nothing.** The light's projection is orthographic, so stored depth
  is linear in view-space z and precision is already uniform across the range.
  The cancellation reversed-Z exploits only exists under a perspective divide.
- **It would cost something real.** The biases in `mesh_shadow.wgsl` are
  calibrated against this sense (`2026-09-08_shadow_bias.md`), and
  `facet_shadow.wgsl` re-derives the same comparison in compute to feed the
  thermophysical model. Flipping them means re-calibrating, and re-calibrating
  means moving the illumination the physics runs on.

This is cheap to do because the two chains were already separate: the light's
matrix comes from `fit_light_view_proj` calling `orthographic_rh` directly, and
never passes through `Projection::mat()`. Both sites now carry a comment saying
why they are not reversed, so the next person does not tidily "finish the job".

## The trap: a hardcoded clip-space z

`colorbar.wgsl` emitted `z = 0.0` — the near plane under forward-Z, which under
`Less` meant "always draw". Under reversed-Z 0.0 is the **far** plane, and
against a background cleared to 0.0 with `Greater` the bar fails the test at
every pixel and disappears entirely.

It emits `1.0` now, which is the same "always draw, never write depth" it
always had. Worth checking for in any screen-space overlay: geometry drawn
through the camera matrix takes care of itself, a hardcoded clip position does
not.

The axes and the light cube go through the camera matrix and needed nothing.

## Measurements

**The point of the exercise.** Same scene, same camera, planes pinned from
Python so the fit's floor is out of the picture and only the depth sense
differs. Punishing case `near = 1e-4, far = 100` (ratio 1e-6) against a
comfortable `near = 0.5, far = 100`, 800x600, `msaa = 1`:

| build | pixels differing from the comfortable render | max delta |
|---|---|---|
| forward-Z (`f3c1baf`) | 286 | 131 |
| reversed-Z | **0** | **0** |

At a ratio of 1e-6 the old build z-fights; the new one is bit-identical to a
render with a near plane 5,000x further out.

**Nothing else moved.** All against `f3c1baf`:

- The two builds are **pixel-identical** at a comfortable near plane, so this
  is inert where depth precision was never the problem.
- The crater's illumination trace over a 200-iteration Sun sweep — lit
  fraction, illumination sum and max, 17 significant digits — is **identical
  byte for byte**. Expected, since the shadow map was left alone, but checked
  rather than assumed: `facet_illumination` goes through the compute path, and
  the render agreeing is not proof the compute path does.
- `facet_id_map()` is **bit-identical** over the whole 1376x2116 array.
- The analytical hemicube validation (`examples/analytical/hemicube.py`) gives
  `F = 0.19990` against an exact `0.20004`, **0.07 % error on both builds**.

**View factors: the totals hold, 8 of 10240 pairs shift.** On the crater, with
five hemicubes at 128:

    per-facet total view factor    old                    new
      row 1                        0.48707725014537573    0.48707725014537573
      row 3                        0.48092244006693363    0.48092244006693363

Bit-identical. Underneath them, 8 individual pair entries differ by at most
3.1e-5 against a largest view factor of 0.0029, and four entries cross
zero — facets sitting exactly on a hemicube-pixel visibility boundary, where
which of two near-equal depths wins is arbitrary either way. The row totals
absorb it exactly, so the energy bookkeeping the physics uses is unchanged, and
the shift is two orders of magnitude below the hemicube's own 0.07 %
discretisation error.

## What this does not do

It does not remove the need to fit the planes sensibly. `far` still has to
reach the scene and `near` still has to be positive; what is gone is the
coupling between them. `debug_light_cube_fit` still stretches `far` to the Sun,
and that is now close to free rather than merely tolerable.

It also leaves the shadow map's own precision exactly where it was. If shadow
depth ever becomes the limit, the answer there is the depth *range* of the
light's frustum, not its direction.

## Guards

Two new tests in `src/app/frame.rs`:

- `reversed_z_maps_near_to_one_and_far_to_zero`, both projections. Worth having
  because getting it backwards does not fail — it renders, with the furthest
  surface winning every pixel, which reads as a culling or winding bug.
- `reversed_z_depth_falls_off_monotonically`, across the range rather than at
  the planes, since the endpoints alone are also satisfied by a matrix that
  does something odd in between.

`the_near_floor_keeps_depth_precision_when_the_eye_is_inside` is renamed to
`..._keeps_the_projection_non_singular_...` and asserts the slack floor, which
is what it now guards.
