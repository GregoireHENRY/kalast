# Handoff, 9 October 2026 — ray tracing, for the home PC (RTX 5080, Vulkan)

Work laptop to home PC. The user asked for the work to be split: the laptop
session carries on with the raster side, and a session on the home PC takes
ray tracing, which the RTX 5080 runs in hardware. Everything is on `main`
(users only download releases, so `main` is where the work goes):

    git pull
    python tools/develop.py --release      # Windows: not maturin directly (CLAUDE.md)

Both sessions push to `main`: pull before starting, commit in small steps,
and `git pull --rebase` before each push.

Read `CLAUDE.md`, `notes/TIMELINE.md` (the entries of 7-9 October) and
`notes/2026-10-09_rendering_survey/README.md` first. The survey's bottom line
is the design brief below, with sources.

## What the user asked for

Both, as options, so they can be compared:

- **Real-time ray-traced shadows**: the Sun's disc sampled by rays from each
  pixel, for interactive use on hardware that has ray tracing.
- **An offline, progressive reference**: images "absolutely error-free" --
  no shadow gaps, the exact limb-darkened disc, light bounced into shadowed
  craters (the Moon's permanently shadowed regions), no aliasing --
  accumulated to a stated error and stopped.
- **The thermophysical model follows whatever the shadows are**: with ray
  tracing on, the per-facet shadow query (`sim.facet_shadow`) traces too.
- **The laptop must run it**, if slower: "even if some features for perf/opti
  might work only on big pcs, my mac still needs to run most of them and
  fast". High-end-only extras are fine as options on top.

## What the platform gives (checked in the wgpu 30.0.1 sources, survey §L)

- `wgpu::Features::EXPERIMENTAL_RAY_QUERY`: BLAS/TLAS and inline ray queries
  in any stage, on Vulkan (`VK_KHR_ray_query`), DX12 (only with DXC; the
  default compiler choice falls back to FXC and drops ray queries) and Metal
  (macOS 15+; the M1 Pro reports it, traversal in Apple's driver, no RT
  hardware). Use Vulkan on Windows, or enable wgpu's `static-dxc`.
- No ray-tracing pipelines (raygen/hit/miss) through the public API: inline
  queries in compute or fragment shaders only. Enough for all of the above.
- Metal: one acceleration structure per shader stage.
- Experimental: may have bugs and breaking changes (gfx-rs/wgpu #1040, #6762).

## Design (survey §0.1, §2, §5)

1. **Geometry.** One BLAS per body from the **full-resolution** mesh (never
   the LOD cut), built once, compacted (`Queue::compact_blas`); a TLAS of the
   bodies' instances rebuilt per frame from `body.mat`. Rays offset robustly
   from the surface (Wächter & Binder, Ray Tracing Gems ch. 6): the 1 cm
   offset of an earlier reference passed over 1-2 cm bumps that 1 mm did not
   (`notes/2026-10-09_disc_gaps_hidden_relief/`).
2. **The Sun.** Next-event estimation over the limb-darkened disc (linear,
   `LIMB_DARKENING = 0.56` in `mesh_shadow.wgsl`): importance-sample the
   radial CDF, stratified or Owen-scrambled Sobol', the sin^2 form of
   `1 - cos theta` in f32. A point Sun is one ray.
3. **Real-time mode.** A few rays per pixel, only where a penumbra can be --
   the depth-pyramid check (`sun_reach`) already finds those pixels, as AMD's
   Hybrid Shadows does with cascades -- one ray elsewhere. No temporal
   denoiser by default: the user's exports must not depend on history.
4. **Reference mode.** Progressive: each frame adds jittered sub-pixel
   samples (box filter, the detector's pixel) and disc samples into an f32
   accumulation, fixed seeds, a plain mean, stopped on a per-pixel standard
   error. About 570 samples per pixel hold a lit limb to one 8-bit step. No
   denoiser, no TAA, no ML upscaling, no firefly clamp.
5. **Bounces** (later): single scattering dominates light in a shadowed
   crater (about 2 % per further bounce in a bowl). Either NEE to sunlit
   facets as emitters, or a final gather over a per-facet radiosity solved
   with the TPM's own view factors -- the route planetary studies validated.
6. **The TPM.** A compute pass tracing each facet's corners and centre to
   the disc, as `cs_facets` in `mesh_shadow.wgsl` does with the shadow maps;
   `Window::facet_shadow_fractions` picks the method.
7. **Validation** before trusting images: the sphere antumbra and wall
   penumbra the tests already check (`tests/test_penumbra*.py`), the plate
   and ridge of `tests/test_penumbra_hidden.py` (where the shadow maps needed
   the camera's image), and Ingersoll's spherical bowl for the bounces.

Expected cost (survey §0.1, extrapolated, measure it): a 4K image with 1024
disc samples per pixel, 0.6-1.7 s on the 5080, 15-45 s on the M1 Pro; three
bounces, 15-50 s against 6-28 min.

## What the laptop session is changing meanwhile

Keep ray tracing in modules of its own
(`src/app/raytrace.rs`, `shaders/raytrace.wgsl`) with small hooks:

- done since this note was first written: the per-facet query follows the
  image's lookups, PCF included (`cs_facets` for every Sun; the old
  `facet_shadow.wgsl` is deleted); `shadows.cache`; `shadows.cascades`;
  `config.quality` ("quick", "fast", "point", "accurate" -- add "reference"
  when ray tracing lands, in `Quality` in `src/app/config.rs`);
- next on the laptop: the disc's penumbra pass skipped when nothing changed,
  and cheaper image passes for 240 fps at 5.7 Mpx.

`src/app/window.rs`, `src/app/config.rs` and `shaders/mesh_shadow.wgsl` are
touched by both: merge often.

## The project's rules that matter here

- Never write into `out/`; benchmarks with `app.config.export_dir =
  "/tmp/..."`, `vsync` off, `open_in_background = True`, medians, the first
  run after a rebuild discarded (CLAUDE.md 24-27).
- A config field: `python tools/gen_config_panel.py`, `tools/gen_bindings.py`,
  `tools/gen_stubs.py`, and their tests; `docs/CONFIG.md` and `docs/API.md`
  entries; a `CHANGELOG.md` line for users.
- A dated note per feature under `notes/`, the timeline kept up.
- Commits carry only the user's name, no co-author trailer.
