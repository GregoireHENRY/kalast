# Timeline — 24 to 26 August 2026

Three days of renderer work, across two machines: a macOS work laptop (M1 Pro)
and a Windows personal machine (Ryzen 9800X3D / RTX 5080). Grouped by day and
theme rather than by commit, with the detailed write-ups linked.

Headline: **frame export went from a 25x brake to free, full-resolution
(3.1M-facet) shape models became usable, and the shadow map became a data
source the thermophysical model can read** — plus two measurement artifacts
that had been quietly corrupting every benchmark.

---

## 24 August — the frame exporter (macOS)

`sim.export_once()` was costing ~25x: **4.1 it/s with export on versus ~100
off**.

`export_frame` stalled the entire GPU pipeline every call
(`device.poll(PollType::Wait)`) and then PNG-encoded and wrote to disk
*synchronously on the render thread*.

- **`1a27821` Non-blocking export.** GPU→CPU copy polled without blocking;
  encode and disk write moved to a background thread; readback buffers pooled
  instead of reallocated per frame. 4.1 → ~89 it/s.
- **`14b59f8` Worker pool + guaranteed flush.** One encode thread was still
  the ceiling, so it became a pool (2-8 by CPU count). Added
  `FrameExporter::finish()`, called from `App::exit()`, which blocks until the
  queue drains — previously, closing the window silently discarded whatever
  was still queued.
- **`8b5bccc`** `Model::load` printed a debug line *per vertex* whenever an
  OBJ carried normals, then read them even though nothing downstream uses
  vertex normals. Now an explicit `unimplemented!()`.
- **`8722023`** New `examples/hera_didymos/` AFC scripts; stale meta-kernel
  path fixed in `examples/didymos/main.py`.

A bug in `afc_eclip_didy_manual.py` also surfaced: `export_once()` was called
*before* the `et > etf` check, so past the sweep end the app kept exporting
the same frozen frame forever.

---

## 25 August, morning — GPU buffers (macOS)

- **`4b35eb3` Split the mesh vertex buffer.** `MeshBuffer` re-uploaded the
  entire interleaved vertex array to a *freshly allocated* GPU buffer every
  frame, per body — ~23 MB per body per frame for a flattened 100k mesh, for
  data that never changed. Split into a static `geometry_buffer` (uploaded
  once) and a dynamic `attrib_buffer` (colours, re-uploaded only when
  `Mesh.colors_dirty` is set). Instance transforms now update in place.
- **`1ed2e18` Full-resolution meshes.** Two fixes made 3.1M-facet models
  usable: the buffer split above, and requesting `adapter.limits()` instead of
  wgpu's conservative 256 MiB default, which had been panicking on load.
- **`a429a6d`** Handoff notes for continuing the benchmark on the other
  machine.

---

## 25 August, afternoon → 26 early — renderer quality (Windows)

Work done on the personal machine, pulled to the laptop on the 26th.

- **`be4f66e` Config options and two real bugs.**
  - `vsync`, `export_sync`, `export_max_queued` added.
  - **PCF shadow filtering was wrong**: `shadow_pcf > 0` averaged taps onto a
    variable pre-set to `1.0`, adding unshadowed light to every filtered
    fragment — the umbra measured 93/255 instead of 7/255, a 13x
    over-brightening. *Any older render made with `shadow_pcf > 0` has shadows
    that are too light.*
  - **`render_back_face` was never wired up** — every pipeline hardcoded
    `cull_mode: None`.
- **`8fa3a44` Automatic frustum and shadow-bias fitting, wireframe.**
  Camera/light `near`/`far`/`side` and the three shadow constants default to
  `None` = fitted per frame from scene bounds, expressed relative to one
  shadow texel so they hold at any scene scale. Barycentric wireframe overlay
  added. The arcball also stopped calling `look_anchor()` every frame, which
  had been silently discarding any `dir` a script assigned.
- **`66529ae`, `1fbe65a`, `a7b0dc9`** `afc_eclip_didy_auto.py` (the same scene
  with nothing tuned by hand), wireframe enabled in two examples.

Write-ups: `pcf_shadow_comparison/`, `renderer_auto_fit_wireframe/`,
`CONFIG_options.md`, `BENCH_mesh_resolution_results.md`.

---

## 26 August — shadow map as a data source (macOS)

### Benchmarks were measuring the wrong thing

- **`a6a7688`** Re-ran the mesh-resolution benchmark with the new `vsync`
  option. The laptop's earlier figures (119.5 and 60 it/s) were **artifacts**:
  119.5 ≈ its 120 Hz ProMotion refresh rate, 60 = the Fifo half-rate cliff.
  Uncapped and with export off: **333 it/s at 100k vs 55 at 3.1M**, so
  full-resolution costs ~6x, not the ~2x previously concluded. A second
  artifact compounded it — the then-unbounded export queue meant the old
  figure partly measured queue growth rather than work done.

### Shadow-mesh proxies

- **`acd5f18`** `load_mesh(..., shadow_path=...)`: render at full resolution,
  shadow with a coarser mesh. Safe in a way LOD is not, because the shadow map
  never carries facet identity.
- **`8006a15`** Measured the ceiling with a 12-facet cube as occluder: 100k
  already captures 90-96% of everything the shadow pass has to give, so a
  silhouette-matching decimator below 100k has nothing to win.
- **`5dcc66f`** Full report with renders and figures:
  `shadow_mesh_comparison/`. **~1.5x for 9 differing pixels of 1,040,400** in
  the demanding case, 0 in the ordinary one.

### Per-facet shadow queries — the significant new feature

- **`cb1b7da`** A compute pass reads the shadow map back *per facet*, giving
  the occluded fraction of every facet — replacing an
  `O(n_facets x n_triangles)` ray sweep. Validated against vectorised
  Möller-Trumbore ray tracing: **98.7% per-facet agreement, and 0.013% error
  on the absorbed-flux integral the TPM boundary condition actually
  integrates**. 1.6 ms at 100k facets, 7.3 ms at 3.1M, versus an extrapolated
  ~36 days for the brute-force trace. Write-up: `facet_shadow_query/`.
- **`c640f7a`** `before_render` / `after_render` callbacks. The query result
  only exists once the frame has rendered, so a single pre-render callback
  could never see its own answer. Splitting the frame removed the lag.
  `app.tick` stays as an alias. **This also fixed a latent bug**:
  `state.iteration` was incrementing *between* the two callbacks, so one frame
  reported two different iteration numbers — enough to silently desynchronise
  any loop deriving an epoch from it.
- **`fd4268d`**, **`f125bdc`** Turned the per-frame `request_facet_shadow(0)`
  call into a one-time `config.access_shadow_map` flag covering *every* body —
  a two-body scene had silently had no data for the second.

### Camera

- **`4306827`** Two fixes. Orbiting had come to require a middle button, which
  a trackpad does not have, so the arcball was **completely unusable on
  macOS**. Added `config.emulate_middle_button` (default on for macOS):
  alt + left-drag stands in, matching Blender's "Emulate 3 Button Mouse".
  Separately, assigning `up` parallel to `dir` made `fix_up` normalise a zero
  vector, producing NaN that propagated into the camera and froze it
  permanently.

---

## Things that were measured and overturned

Worth recording separately, because each had been believed and acted on:

| Claim | What measurement showed |
|---|---|
| "3.1M costs ~2x over 100k, that's just vertex cost" | Both figures were vsync artifacts; the real ratio is ~6x |
| "The async export queue is fine unbounded" | 30 GB RSS growing ~2 GB/s, ~5.6 frames/s actually reaching disk while the loop claimed 626 it/s |
| "100k shadow proxy captures ~99% of the gain" | Single-run noise; repeated measurement says 90-96% |
| "A shadow proxy can't be combined with the facet query" | Asserted, not measured, and wrong: 99.98% of facets agree |
| "10k shadow proxy is 3% faster than 100k" | Noise — 10k, 100k and a 12-facet cube are indistinguishable |
| "`lto = "fat"` will speed up release builds" | No improvement, 1.5x the build time |

Two recurring measurement traps, both now documented:

- **vsync.** `caps.present_modes[0]` is `Fifo` on both machines, so every
  timing run was pinned to the display refresh rate. Set
  `config.vsync = False` for anything measured.
- **Window occlusion.** macOS throttles rendering for occluded or backgrounded
  windows, producing runs at 1.8-64 it/s beside siblings agreeing within
  1 it/s, and occasionally an indefinite stall. Keep the render window visible
  and frontmost, take medians, and discard the first run after a rebuild.

---

## Standing guidance that came out of this

- **Build with `--release`** for anything measured or run in earnest — 2-15x,
  worst on the per-pixel export loops. Debug is for implementing.
- **Full-resolution meshes are oversampled at typical range.** At 25.8 km the
  bodies cover ~71,800 of 1,040,400 pixels, so a 3.1M mesh is ~44 facets per
  pixel. Break-even is ~4 km. That number is what distance-driven LOD should
  key off, if it is ever built.
- **The largest remaining approximation is not numerical.** The Sun is treated
  as a point source by both the shadow map *and* the ray tracer, so the ~6.7 m
  penumbra Dimorphos casts on Didymos (~6 facets wide) is rendered as a hard
  edge — an order of magnitude larger than any sampling error measured here.
  Fixing it has to change the TPM illumination term at the same time to stay
  consistent. See `facet_shadow_query/` §7.
