# Handoff, 12 October 2026 — ray tracing, home PC back to the laptop

The home PC (RTX 5080, Windows, Vulkan) took the ray-tracing half of
`2026-10-09_HANDOFF_ray_tracing.md`. Everything is on `main`; the full
write-up, with every measurement and figure, is
`2026-10-10_ray_traced_shadows/README.md`. The user is moving to the work
laptop (M1 Pro, macOS, Metal) to try it: **none of this has run on Metal
yet.**

    git pull
    python tools/develop.py --release

## What there is now

| Setting | What it does |
|---|---|
| `shadows.rays` | The Sun's shadows traced with rays, the image's and the per-facet query's (`sim.facet_shadow`), against every body's full-resolution mesh. While on, the shadow maps are neither drawn nor kept at size. `sim.rays` says whether rays are used. |
| `shadows.ray_samples` (64), `shadows.ray_probe` (16) | Rays a point across the limb-darkened disc; the probe tried first, half on the rim, the rest only where they disagree. |
| `quality = "rays"` | The preset: rays, the disc, 64, 16. The maps' four presets now set `shadows.rays = False`. |
| `reference.enabled`, `.error` (1/510), `.max_samples` (4096) | A still image integrated over each pixel and the disc, a sample a frame, to a stated standard error (four replicas' spread); `sim.reference_image()`, `sim.reference_error_image()`, `sim.reference_samples`, `sim.reference_error`, `sim.reference_done`. |
| `reference.bounces` (0..4) | Sunlight bounced off the surfaces, in the reference only; a body is Lambert of its mean colour. |

Also: the shadow array falls back to the largest resolution that fits in
the GPU's memory (it crashed the app at 16384 with the disc); the bodies'
acceleration structures are compacted; the presets are a wrapping row of
their own, each button's hover listing its fields and values.

## Commits

`03da5b5` facet query traced · `d75b853` the image traced · `9e3d05c` no crash
at 16384, maps put away while rays answer · `ca17120` presets row and table,
Dimorphos's self-shadows characterised · `96049bc` compaction · `02b210a` the
reference image · `a52c383` bounces.

## To try on the Mac first

1. **Does Metal take ray queries?** `kalast.app.App()`, `config.shadows.rays =
   True`, a step: `app.simulation.rays` should be `True` (macOS 15+, the survey's
   probe said the M1 Pro reports it). If `False`, the kalast tab says the GPU
   has no ray queries, and everything below falls back to the maps.
2. **The tests**, in this order -- each skips itself without ray queries:
   `tests/test_facet_shadow_rays.py`, `tests/test_image_rays.py`,
   `tests/test_reference.py`, `tests/test_reference_bounce.py`. On the 5080 all
   pass; on Metal the pixel-exact ones (`test_image_rays`: no speck on a lit
   facet) are where a difference in Metal's intersector would show -- its
   watertightness is not documented (survey, section L).
3. **The Metal risks the survey named**, for if something fails:
   - gfx-rs/wgpu #9215, bottom- and top-level builds not ordered within one
     submission: they are in separate submissions here (`build_bodies`,
     `update`), and compaction waits for its own.
   - The 31-slot buffer table, shared with acceleration structures: the main
     pass now binds group 7 (the top level and, for bounces, four storage
     buffers, `raytrace::Geometry`). If a pipeline fails to build on Metal,
     count the fragment stage's buffers first.
   - #9414, timestamps all zero on macOS 26: time by wall clock.
4. **The settings panel**: Shadows has the presets row and, under it, a
   "reference" subheading with its rows and a progress line. Never seen on
   screen: posted clicks on the header were lost on the PC. Look at it.
5. **Cost**: on the 5080 a step of the Didymos pair with every facet's shadow
   is 7.4 ms with a point Sun (maps 7.2), 31 ms with the disc at 64 rays; an
   image of the pair at 1200 x 800 is 0.8 ms (point) and 5.2 (disc); a
   reference of Dimorphos up close 1.5 s. The survey expects the M1 Pro about
   19 times slower for rays (driver traversal, no RT hardware). Point-Sun rays
   may still be usable; the disc and the reference are offline there.

## Found, not fixed

- `tests/test_horizon_map.py` fails "and not more or less on the whole" (mean
  +0.058) on `main` from before this work (checked at 1aa1420, with the ray
  commits reverted too).
- The plate-and-wall mesh of the penumbra tests, drawn by its level of detail
  from 45 above, has holes (the background shows through), maps or rays alike.
- The camera's fitted near and far planes alternate by a unit in the last
  place every frame on Dimorphos up close (1.0789164 / 1.0789163); harmless to
  the image, but it is why the reference compares the camera's pose, not its
  matrix.

## Open, in the order I would take them

1. Metal: the four tests above, and timings.
2. Bounces in the real-time image and in the thermophysical model's shadows
   (the reference alone has them): for the TPM, light scattered onto a facet is
   the self-heating term, which may belong in the TPM's own view factors
   rather than in rays.
3. Coloured surfaces in bounces: a body's mean colour stands for all of it,
   and every law for Lambert's; per-facet albedo needs every body's attribute
   buffer bound (a binding array, or one shared buffer).
4. Rays only where a penumbra can be, for the disc in real time (the depth
   pyramid's tiles, as the handoff proposed), especially for the Mac.
5. A float export of the reference (EXR or TIFF) beside the PNG; today a
   script gets the floats through `sim.reference_image()`.

## Files both sessions touch

`src/app/window.rs` (the ray tracer's state, the reference's scene and
jitter, the shadow array's fallback), `src/app/config.rs` (`Shadows.rays...`,
`Reference`, `Quality::settings`), `shaders/mesh_shadow.wgsl` (`//@rt` lines,
the hook in the fragment stage, `ray_frame`), `src/app/pass/mod.rs` and
`render.rs` (the reference path, `render_reference`), `src/app/uniform.rs`
(Light's `rays`, `ray_probe` where `_peel_pad` was; Globals' `ray_frame` where
`_padding2` was), `src/app/gpu.rs` (`shader_for`'s `//@rt`, `//@rays`,
`//@rays_image`; no blend state for a format that cannot blend). New:
`src/app/raytrace.rs`, `src/app/reference.rs`, `shaders/raytrace.wgsl`,
`sun_rays.wgsl`, `sun_rays_image.wgsl`, `reference.wgsl`.
