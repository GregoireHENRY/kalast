# Timeline — from 24 August 2026

The running summary. Undated in its filename because it keeps growing; each
day is a section below, newest last, and the open items at the end of each are
the place to look for what is unfinished.

Work runs across two machines: a macOS work laptop (M1 Pro) and a Windows
personal machine (Ryzen 9800X3D / RTX 5080). Grouped by day and theme rather
than by commit, with the detailed write-ups linked.

Headline for the opening three days: **frame export went from a 25x brake to
free, full-resolution (3.1M-facet) shape models became usable, and the shadow
map became a data source the thermophysical model can read** — plus two
measurement artifacts that had been quietly corrupting every benchmark.

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
  path fixed in `examples/didymos/main.py`. Loading both full-resolution
  shape models panicked on wgpu's default 256 MiB buffer cap, recorded in
  `2026-08-24_hera_didymos_mesh_limits.md` and fixed the next day.

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

Write-ups: `2026-08-25_pcf_shadow_comparison/`, `2026-08-25_renderer_auto_fit_wireframe/`,
`CONFIG.md`, `2026-08-25_BENCH_mesh_resolution_results.md`.

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
  `2026-08-26_shadow_mesh_comparison/`. **~1.5x for 9 differing pixels of 1,040,400** in
  the demanding case, 0 in the ordinary one.

### Per-facet shadow queries — the significant new feature

- **`cb1b7da`** A compute pass reads the shadow map back *per facet*, giving
  the occluded fraction of every facet — replacing an
  `O(n_facets x n_triangles)` ray sweep. Validated against vectorised
  Möller-Trumbore ray tracing: **98.7% per-facet agreement, and 0.013% error
  on the absorbed-flux integral the TPM boundary condition actually
  integrates**. 1.6 ms at 100k facets, 7.3 ms at 3.1M, versus an extrapolated
  ~36 days for the brute-force trace. Write-up: `2026-08-26_facet_shadow_query/`.
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
  consistent. See `2026-08-26_facet_shadow_query/` §7.

---

## Open at the end of 26 August

Status as reviewed at the close of the 26th, so the thread can be picked up
without re-deriving it.

### Active

- **Deimos / TIRI swing-by — the original objective, still mostly ahead.**
  *Correction, 2026-08-27:* the `deimos_tpm_4` convergence run (2025-03-11 →
  03-13, dt 300 s) **did complete** — 577 timesteps x 5,040 facets, 67-302 K.
  It was reported missing because `tpm.py` writes to `out/` root by default,
  so it never landed in `out/hera_mars_swingby/deimos_tpm_4/` and looked
  absent. Move it into a named directory before the next run overwrites those
  files silently. `rad_sum.py` is written but has
  never been run. `rad.py` works but is wide-filter only, iterates TPM
  timesteps rather than the real image epochs, and computes `fwpos` without
  using it. `rad_campx.py` (per-pixel projection) is still only in
  `examples/old/`. The comparison against the real TIRI FITS has not started.
- **Per-body colour mode.** `mesh.color_modes[:] = 1` in
  `examples/hera_mars_swingby/tiri_data.py` has no effect: the fragment
  shader reads only the scene-wide `globals.color_mode`, and the per-vertex
  attribute is uploaded but never read. That scene therefore renders the
  Deimos radiance colormap multiplied by diffuse lighting and shadow instead
  of raw — a physical error in frames intended for comparison against real
  TIRI data, and it affects everything already in
  `out/hera_mars_swingby/frames_1000_to_1400_rad_gray/`. Fixing it means
  either wiring the per-vertex attribute into the shader or adding a per-body
  mode; `tiri_data_deimos_only.py` sidesteps it with a global
  `color_mode = 1`, which only works because Deimos is the sole body there.

### Paused deliberately

- **Solar penumbra** (`2026-08-26_facet_shadow_query/` §7) — the largest remaining
  approximation, and it has to change the TPM illumination term at the same
  time to stay consistent.
- **LOD**, including interpolating per-facet data between resolutions. The
  ~4 km break-even distance is the number to key it off.
- **Camera-POV occultation query** for dropping occulted facets in the
  radiance step. The light-POV query exists; this one does not.
- **`~/.claude/CLAUDE.md`** — the destructive-command rule was only on the
  macOS laptop. Resolved for this project on 2026-08-27 by putting it in the
  repo's own `CLAUDE.md`, which is committed and so reaches every machine on
  `git pull`. Still absent globally on the Windows machine, i.e. it does not
  cover that machine's *other* projects.

### Closed

- Cosmographia cross-check: concluded and positive, no further work planned.
- Arcball camera on trackpad: fixed and confirmed working.
- `intersect_mesh` allocating a `Vec` per call: known, left as is — it is not
  on any active path now that the GPU query replaced it.

---

## 27 August — conduction solvers (macOS)

Preparing a Didymos run that must resolve both the diurnal (2.26 h) and
seasonal (700 d) waves. Write-up: `2026-08-27_conduction_solvers/`.

- **`nonuniform.column()` and `core::conduction_1d` did not compose.** The
  grid builder produces geometric spacing; the solver implements the
  equal-spacing second difference. Validated against the analytical damped
  wave, that combination errs by **12.1 K** where a uniform grid errs by
  0.3 K — and it fails silently, since nothing checks the grid it is handed.
  Neither piece was at fault alone; nothing had ever used them together.
- **Added `core::conduction_1d_nonuniform`**, the variable-spacing stencil.
  A 16-node geometric grid now reaches 0.48 K against 0.32 K for 81 uniform
  nodes — comparable accuracy at a fifth of the nodes, which is what makes a
  seasonal column affordable.
- **Filled in `kalast/tpm/routine.py`**, which was empty: coefficient
  builders, the non-uniform stability limit, and a grid resolution report.
- **Rewrote `kalast/tpm/implicit.py`**, which could never have run — its
  boundary helpers were module-level functions taking `self` and
  dereferencing fields that do not exist, called with two arguments against a
  seven-argument signature, and nothing solved the system. Now a working
  backward-Euler solver, validated at 9.1x the explicit timestep. The
  radiative surface boundary was still missing at this point — completed on
  28 August, below.
- **Brought the analytical examples out of `old/`**:
  `examples/analytical/sinusoidal.py` (five solver/grid combinations) and
  `slab_relaxation.py` (Fourier series, both boundary conditions), updated
  from an API where `diffusivity`/`skin_depth_1` still lived in `tpm.core`.

### Didymos TPM and the staged run

- **`examples/hera_didymos/tpm.py`**: two-orbit spin-up (2023-03-23 ->
  2027-01-21, 14,867 rotations) on the 10k-facet mesh, grid selectable so the
  same physics can be timed both ways. Benchmarked:

  | grid | nodes | dt | ms/step | total |
  |---|---|---|---|---|
  | uniform | 2,168 | 32.4 s | 66.1 | 68.6 h |
  | geometric | 34 | 55.9 s | 46.3 | 27.8 h |

- **Coverage trap**: `hera_plan_local.tm` carries only the Hera proximity-phase
  Didymos SPK (2026-07 -> 2027-07), so a two-orbit spin-up throws
  `SPKINSUFFDATA`. The script furnishes the Horizons 1999-2050 ephemeris
  explicitly.
- **Vectorised the facet loop — 13.5x measured** (20.04 -> 1.49 ms/step). 64x
  fewer nodes had bought only 1.4x, because the cost was per-facet Python and
  FFI overhead, not conduction arithmetic. `routine.py` gains
  `step_surface_newton` and `step_conduction`. This should take the two-orbit
  run from ~28 h to about an hour, and it weakens the case for the implicit
  solver, whose advantage is a bigger timestep.
- **Caught before launching**: `tpm.py`'s non-benchmark branch printed min/max
  and saved nothing — a 28-hour run would have produced no output.
- **No permanently shadowed regions on Didymos.** Sampling one orbit at 20,000
  epochs, no facet of 10,000 ever fails to face the Sun and the worst peaks at
  `cos i = 0.27`. Obliquity is 14.6 deg off the orbit normal, an order of
  magnitude more than the Moon's ~1.5 deg, so the poles get a real seasonal
  cycle. This is what makes a coarse spin-up without self-heating defensible.
- **Two-phase strategy agreed**: coarse 1D spin-up with direct insolation
  only, save the equilibrated state, then a short high-fidelity segment near
  the study epoch with shadowing, mutual and self heating. Enabling terms one
  at a time turns it into an ablation that quantifies each binary effect.

### Next steps written up

`2026-08-27_conduction_solvers/` §7-9 covers what the TPM still lacks (eclipse
shadowing via the validated facet-shadow query, mutual heating, self-heating),
thermal roughness in two stages (a geometric correction to radiance, then
sub-facet columns in the boundary condition), Hapke bidirectional reflectance
as a thermal/optical consistency test, FEM and lateral conduction (motivated
by Dimorphos being only ~10-60 seasonal skin depths across while tidal locking
sustains a permanent lateral gradient), and a review of `view_factor_facets` —
whose proximity guard returns zero exactly where the view factor is largest,
so neighbouring facets contribute nothing to self-heating, with the hemicube
proposed as the GPU route.

Measured constraint that outranks the solver choice: the TPM looped over
facets in Python, 6.6 ms/step at 3,072 facets and 10.4 ms at 5,040. Now
addressed by the vectorised path above.

---

## 28 August — phase 2, and finishing the solver family (macOS)

### Phase 2: the eclipse segment, run and measured

`examples/hera_didymos/tpm_phase2.py` — restarts from the three-orbit spin-up
on the identical grid, loads both bodies so Dimorphos occludes, and runs the
last six Didymos rotations to 2027-01-21T05:36 UTC inside the render loop
(`before_render` places bodies from spice, `after_render` reads
`sim.facet_shadow(0)` and steps the TPM).

**The first run was invalid and the numbers first reported were wrong.** It
furnished the Horizons Didymos SPK alongside the meta-kernel, copying
`tpm.py`. That file carries the same body id (`-658030`) as the mission's
`didymos_flp_*.bsp`, and SPICE serves the last-loaded file for an id — so it
replaced the mission solution, which disagrees by **106 km**. Dimorphos was
placed 106 km away on the anti-sunward side, casting no shadow; the 64
"shadowed" facets reported were Didymos shadowing itself in its concavities.
Nothing errored. The spin-up is unaffected (106 km against 1.5e8 km changes
no Sun direction) and genuinely needs that kernel; this segment does not.

Corrected, the study epoch turns out to be a **dead-centre eclipse**:
Dimorphos 1.151 km sunward of Didymos, perpendicular offset from the
Didymos–Sun line of **1 metre**. Three-way ablation over `SHADOW_MODE`:

| term | facets changed | worst ΔT | disk-mean ΔT | worst band-radiance drop |
|---|---|---|---|---|
| self-shadowing | 4,105 | −12.4 K | −0.16 K | −9.4 % |
| **eclipse** | 3,980 | **−95.9 K** | −1.34 K | **−77.7 %** |
| both | 4,240 | −95.9 K | −1.49 K | −77.7 % |

Radiance integrated over TIRI's 8–14 µm band. At the study epoch 257 facets
are still in shadow and 383 carry a >5 % band-radiance drop. The worst facet
falls 343.8 → 247.9 K, a 78 % radiance deficit — the eclipse is the dominant
feature of the image, not a correction to it.

Segment cost 4.8–6.3 s wall, so the geometric eclipse-window optimisation is
unnecessary at this length. It stays on the list for the longer segment
Dimorphos will need: tidally locked to an 11.9 h orbit, 13.6 hours is only
1.1 of its days, and its own temperatures are not computed at all yet — the
mesh is loaded purely as an occluder.

### Implicit finished, and given a family

The radiative surface boundary is implemented, which was the one thing
blocking the implicit path from a real thermophysical run. It is non-linear
in `T₀` and, unlike the explicit path, cannot be applied after the solve —
`T₁` and `T₂` in the balance belong to the new profile. Solved exactly rather
than by lagging: the interior is written `U + T₀·V`, with `V` (the response
to a unit surface temperature) computed once in the constructor and `U` one
banded solve batched across all facets, leaving a *scalar* Newton per facet.
Without that decomposition it would be 10,000 banded solves per step.

`implicit.Solver(scheme=...)` now offers **backward Euler, Crank-Nicolson and
BDF2**, and `explicit.Solver(scheme=...)` offers **forward Euler and RKC**
super-time-stepping. Measured at 2,000 facets over 4 spins against a
time-converged reference:

| scheme | dt [s] | steps | wall [s] | max err [K] |
|---|---|---|---|---|
| explicit forward Euler | 9.0 | 3636 | 0.47 | 0.588 |
| explicit RKC (3 stages) | 89.7 | 363 | 0.15 | 4.489 |
| implicit backward Euler | 81.4 | 400 | 0.08 | 0.805 |
| implicit Crank-Nicolson | 81.4 | 400 | 0.13 | 0.006 |
| implicit BDF2 | 81.4 | 400 | 0.10 | 0.023 |
| **implicit BDF2, coarse** | **325.4** | **100** | **0.02** | **0.393** |

**19x faster than the explicit path and still more accurate.** BDF2 is the
production recommendation: the only one that is both second-order and
L-stable.

### Three things measured rather than assumed

- **Crank-Nicolson ringing is real but narrower than the textbook warning.**
  Stepping a *prescribed* surface temperature, CN oscillates (10 direction
  reversals in 12 steps) where BE and BDF2 are monotone. Stepping the *flux*
  — an eclipse ingress — none of the three rings, at any dt out to 61x the
  explicit limit, because the radiative surface node is algebraic and
  re-anchors the column each step. CN is safe for radiative runs.
- **RKC works and still loses here.** Its stability boundary grows like `s²`
  (verified numerically from the recursion, not quoted), giving 91x the
  forward-Euler timestep at 10 stages. But a batched tridiagonal solve costs
  less than three explicit stages, so implicit wins outright. Kept because it
  becomes the first choice wherever the solve stops being cheap — lateral or
  FEM coupling, or a GPU implementation with no banded solver.
- **DuFort-Frankel deliberately not implemented.** Explicit and
  unconditionally stable, but its truncation error carries `(dt/h)²`, so it
  converges to the heat equation only as `dt/h → 0`. On a grid with a 1.2 mm
  first layer, any timestep worth having makes it solve a different equation.

### A convergence test that was measuring itself

Order-of-accuracy verification first reported 1.0 for all three schemes with
identical errors. That was the harness: snapshots were compared against the
analytical solution at the *requested* time, but a snapshot lands up to `dt`
late, and at 100 K amplitude that phase offset is ~6 K at `dt = P/100` —
first order in dt, identical for every scheme, swamping the measurement.
Fixed, then the second-order schemes sat on the grid's *spatial* error floor,
which hid it again. Only a reference on the same grid stepped to convergence
isolates the temporal error: 1, 2, 2 as they should be.

---

## 31 August — the GIS3D TIRI product (macOS)

Seven simulated TIRI FITS of the Didymos system at 2027-01-21T05:36, one per
filter, in physical radiance. Write-up: `2026-08-27_conduction_solvers/` §7.7.

- **Dimorphos now has its own thermophysical state.** `tpm.py` is
  parameterised by `BODY`; the grid follows the diurnal skin depth, and
  tidal locking at 11.37 h makes Dimorphos's sqrt(5) larger, so 29 nodes and
  a 703 s stability limit against Didymos's 34 and 140 s. Two traps:
  `DIMORPHOS.orbit_period` is its 11.9 h orbit around Didymos, not a year,
  and using it for the seasonal skin depth would build a column centimetres
  deep; and no kernel covers Dimorphos before 2026-07, so the spin-up uses a
  uniform tidally-locked frame anchored at the study epoch, measured to
  drift 0.013 deg/day against the kernels where both exist.
- **Facet-index buffer** (`src/app/facet_id.rs`, `shaders/facet_id.wgsl`,
  `sim.request_facet_id()`). The scene renders a second time into an
  `R32Uint` target holding `1 + offset + facet` per pixel; radiance is then
  looked up per facet in numpy at full precision. Chosen over reading back
  the colour image, which quantises to 8 bits, mixes in lighting, and would
  have hit the per-body colour-mode bug. Its own depth buffer gives
  visibility and inter-body occlusion for free.
- **A units error, caught by the user on opening the files.** The first
  version wrote band-*averaged* spectral radiance in W/m2/sr/um, justified
  by a claimed factor 0.5 in `Response_Fil-a..f`. That factor does not
  exist -- it came from dividing by a near-zero denominator in four-decimal
  columns and taking the minimum of what is quantisation noise; the median
  ratio is 1.000000 for all seven filters. The symptom was that the wide
  band `g` read no brighter than the narrow `a`, which is absurd for a 5x
  wider filter. Real calibrated TIRI carries `BUNIT = 'W m^-2 sr^-1'`, so
  the product is now band-integrated, and `g` reads 4.7x `a` as it should.
- **Rich headers**: time and kernels, observing geometry, boresight
  intercept (body, facet, lat/lon, temperature), mutual-event flags
  (eclipse on primary, secondary in umbra, totality, occultation), what is
  in frame, both shape models, and the full method provenance including
  that mutual and self heating are absent.
- **"In FOV" and "resolved" separated.** Didymos spans ~133 px while
  carrying 5,889 camera-facing facets, so most are sub-pixel: 100 % in the
  field of view, 83 % sampled. An earlier version reported the sampled
  fraction as coverage and flagged a fully-imaged body as clipped.
- **Two-body ablation.** At the study epoch the bodies are dominated by
  different terms: Didymos by the eclipse (-93.7 K, -78.9 % band radiance),
  Dimorphos by *self*-shadowing (-116.2 K), since it is at conjunction and
  fully lit. Dimorphos self-shadows 6.3x more facets and holds each in
  shadow five times longer, being tidally locked -- though the two meshes
  are not at equal ground resolution (13.1 m against 2.73 m facets), so
  part of the facet-count ratio is resolution rather than shape.
- **A frame bug the nested ablation caught.** `self` came out colder than
  `mutual`, which is impossible since mutual is self plus an occluder. The
  cause: with Dimorphos alone in the scene, `before_render` still placed the
  Sun in Didymos's frame, so the shadow map and the TPM disagreed about
  which facets were lit. Nested ablations give a free monotonicity check.
- **Context frames for the delivered FITS.** 168 frames across +/-6.5 h of
  the study epoch (1.14 Dimorphos orbits) through the real TIRI pointing,
  as diffuse, temperature and wide-band radiance. The sequence contains all
  three mutual events of that orbit -- two umbra passages of the secondary
  at -5.7 h and +5.7 h, and the shadow transit on the primary at the epoch
  -- each 93 min, matching the geometric prediction. Scales fixed and the
  crop shared across frames, since autoscaling would hide the cooling the
  sequence exists to show.
- **An apparent extra eclipse: two bugs, and a bad check.** (a) Frame
  filenames were unpadded `{N}.png`, so viewers sorted them 0, 1, 10, 100,
  ... scattering the 40 umbra frames into 14 clusters. Now `{N:06}.png`.
  (b) **The shadow frustum was centred on the light's view axis while sized
  from the bounding sphere of the scene.** For a binary those differ:
  Dimorphos sits up to 1.15 km off-axis, its far edge at 1.246 km against a
  1.056 km half-width, so it was clipped out of the shadow map at 37 of 131
  epochs -- and clipped geometry reads as shadowed. Not only cosmetic:
  `facet_shadow` reads the same map, so the TPM got the same wrong lit
  fraction. Fixed by offsetting the orthographic box rather than enlarging
  it; enlarging also works but doubles the world-per-texel and moved
  Didymos, which was never clipped, by 8 K. After the fix Didymos differs
  from the original by 0.01 K on 9 facets and Dimorphos by up to -3.4 K.
  Every §7.7 conclusion survives; numbers move 1-3 K.
  In between, a check that was not one: spurious darkening was tested with
  *peak* brightness in a box, which stays 1.000 while half the body is
  black. The mean was in the same table at half its neighbours and went
  unread. A statistic that cannot fall when the defect is present is not
  evidence of absence.

---

## 31 August - 1 September — view factors and radiative heating (macOS)

Full write-up: **`2026-08-31_view_factors/`**. The GIS3D TIRI product moved to
its own note too, **`2026-08-31_gis3d_tiri_product/`** — both had been living
inside the conduction-solver note and were separate tasks.

- **Self and mutual heating are in the TPM.** A GPU hemicube gives the view
  factors at 0.20 ms/facet, every loaded body in one shared index space, so a
  single row carries self and mutual and occlusion is shared — a mutual
  eclipse blocks mutual heating with no extra machinery. `kalast.tpm.heating`
  consumes it, sparse (0.31 % dense) and chunked so the 0.80 GB dense form is
  never built.
- **Validated where the answer needs no reference**: closure 1.00001 on a
  sealed box, and an isothermal black cavity balancing to +0.001 % while
  eps=0.9 falls short by exactly the 1-eps a single bounce never re-absorbs.
  Monotonicity `none <= self <= mutual` is exact — zero facets cooled.
- **What it is worth**: Didymos +0.07 K mean, +2.05 K peak. Dimorphos
  **+2.92 K mean, +30.18 K peak**, 6,578 of 10,000 facets moved by over 1 K.
  Thermal re-emission is ~90 % of it. **Didymos needs self only; Dimorphos
  needs both** — `heating_preflight.py` reaches that verdict in a minute at any
  epoch.
- **Mutual heating is a night-side effect**: +2.34 K on Dimorphos's coldest
  quartile, -0.001 K on its warmest. Tidal locking points the Didymos-facing
  hemisphere away from the sun at conjunction, and `dT = dF/(4 eps sigma T^3)`
  makes a given flux worth four times as much at 200 K as at 320 K.
- **Four bugs.** The hemicube far plane was sized from the requesting body, so
  Dimorphos's primary fell outside it — the mutual term read 0.017 against a
  true 0.115, and exactly zero past 1.5 km. **An occluded window stopped the
  simulation dead**, not throttled it: the frame handler returned before the
  callbacks ever ran, so a covered window did no work at all while wall time
  accrued. The hemicube reallocated 40 MB of scratch per call until the driver
  stalled. And `os._exit` was discarding buffered stdout.
- **A correction.** The 22 "reversed" Dimorphos facets reported on 31 August
  are one, not 22. Flipping all 22 sent their self view factor to exactly 1.0
  and their solar incidence from +0.58 to -0.58 — 21 were real concavities
  that the star-shaped heuristic misjudged, exactly as its own docstring
  warned. The hemicube is the detector: self VF > 0.5, ground truth.
- **It is a decimation artefact.** Every Didymos model and the full 3.1M
  Dimorphos flag zero; only the decimated Dimorphos models carry any. MeshLab's
  `preservenormal` defaults to off. Re-cut with it on plus `qualitythr` 0.6,
  the max self view factor drops 0.996 -> 0.319 with no loss of area fidelity.
  All four decimated models were replaced on 1 September, originals kept
  alongside — **so the phase-1 spin-up states are stale and must be re-run.**

---

# Still to do

A standing list, not tied to a date. Everything here is open as of
1 September 2026.

## The paper — `/Users/gregoireh/projects/paper-kalast`

**Started, never finished. Last commit 2025-12-05**, "finished transition from
gdoc to latex". MNRAS template, `main.tex` at 44 kB, 23 figures under `fig/`.

Three separate problems, and they are worth keeping separate because they need
different kinds of work:

1. **The text reads poorly.** The author's own judgement; it wants rewriting
   rather than editing.
2. **The figures are weak** and could be much better. 23 of them.
3. **The code and the methods have moved a long way since December**, and the
   paper does not reflect any of it.

On that third point, what has changed under the paper's feet — this is the
part that will silently go stale, so it is written down rather than
remembered:

- **View factors are a different method entirely.** The paper's
  `fig/view-factor.png` and `fig/mutual-heating-90°.png` predate the GPU
  hemicube. The old `view_factor_facets` is now known to read **37.8 % low**
  on a configuration with a closed-form answer, because its proximity guard
  returned zero exactly where the view factor is largest. Anything in the
  paper resting on it needs redoing, not just re-plotting.
  See `2026-08-31_view_factors/`.
- **Self and mutual heating exist now** and are quantified: negligible on
  Didymos (+0.07 K mean), +2.92 K mean and +30 K peak on Dimorphos, with the
  mutual part almost entirely a night-side effect.
- **The conduction solvers changed**: implicit BDF2 is 19x faster than the
  explicit path and more accurate. See `2026-08-27_conduction_solvers/`.
- **The TPM and radiance now run on the GPU**, 27x and 3x, which makes the
  full-resolution 3.1M shape model usable where it was a 20-day run.
  See `2026-09-01_gpu_tpm/`.
- **A shape-model correction**: the decimated Dimorphos meshes carried a
  decimation artefact, since re-cut. If any figure used the old 10k mesh its
  Dimorphos temperatures are affected.

## Model and code

- ~~Synodic-phase table~~ **tried and rejected** — built, measured, off by
  0.66 K mean and 19 K worst on Dimorphos against a 2.92 K effect, and no
  better at twice the density. Dimorphos's libration breaks the recurrence.
  Written up in `2026-08-31_view_factors/`; the code is kept for a pair that
  locks rigidly.
- ~~Insolation on the GPU~~ **done** — 61 ms/step to 8.4 at 3.1M, taking the
  full-resolution spin-up to 7.6 h. `2026-09-01_gpu_tpm/`.
- **Choose the mesh resolution per body for phase 1.** Measured against TIRI
  at the study epoch (5.87 m/px, 25.8 km range): **Didymos wants the 100k**
  mesh — the 10k gives one facet per 5 pixels and reads blocky — while
  **Dimorphos is already well matched at 10k** (4.6 facets/px), its 100k
  being 46x oversampled. The current setup uses 10k for both, so the primary
  is under-resolved.
- **Re-run the phase-1 spin-up for both bodies.** The decimated meshes were
  replaced on 1 September, so the saved states no longer match;
  `tpm_phase2.py` now refuses to start rather than running against the wrong
  geometry.
- **Re-run the GIS3D TIRI product** with heating on — Dimorphos self and
  mutual, Didymos self only. `2026-08-31_gis3d_tiri_product/`.
- **Thermal surface roughness**, in two stages, then Hapke as a
  thermal/optical consistency test. `2026-08-27_conduction_solvers/` §8.
- **FEM and lateral conduction**, motivated by Dimorphos being only ~10-60
  seasonal skin depths across while tidal locking sustains a permanent lateral
  gradient. Same note, §8.5.

## Loose ends

- **Figures quoted before 1 September are facet-count means**, not area-
  weighted, and facet areas span 226-541x on these meshes. The corrections are
  not uniform or even one-signed: Dimorphos's phase-1 surface temperature goes
  **237.8 -> 250.6 K** (+12.7) while Didymos's goes 263.5 -> 259.2 (-4.3), and
  the heating effect goes +2.92 -> +2.56 K. Anything restated from an older
  note should be recomputed with `routine.area_mean` rather than trusted.

- **The 2.0 km sweep anomaly**: the mutual view-factor distribution drops ~4x
  below its neighbours at 1.5 and 2.5 km while the maximum stays on the
  `(R/d)^2` curve. Outside this system's orbital range, so it affects nothing
  here, and unexplained.
- **A view-factor rebuild every step (cadence 1) hangs** on its final rebuild.
  Cadence 2 and up complete; not chased, since 12 deg is the working point.
- ~~Pushed commits carrying a `Co-Authored-By: Claude` trailer~~ **stripped
  on 4 September.** The count was **31**, not the 14 recorded here, reaching
  back to `be4f66e` on 25 August. `git filter-branch --msg-filter` over
  `be4f66e^..HEAD` rewrote 113 commits; trees are byte-identical to the
  pre-rewrite state, the commit count is unchanged at 166, and author and
  committer dates are preserved. The pre-rewrite history is kept locally at
  `backup-before-strip` (and the 1 September attempt at
  `backup-before-trailer-strip`).

---

## 3 September — MSAA, and sorting the examples (macOS)

- **Antialiasing on the main render pass**, `config.msaa`, default 4.
  Silhouettes here are measurements — limbs, terminators, apparent diameters —
  and at one sample per pixel each is quantised to whole pixels. Only the main
  pass is multisampled: the shadow, facet-id and hemicube passes carry ids and
  depths, and averaging an id gives an id belonging to no facet. Exports are
  unchanged, the pass resolving into the same single-sample target.
  `notes/2026-09-03_msaa.md`.
- **`examples/hera_mars_swingby` reorganisation started.** `analysis/` for
  deep-dive work, `old/` for superseded scripts, and a new short
  `diffuse_lighting_one_image.py` as the quick-look entry point.
  `notes/2026-09-03_examples_reorg.md`.

### Open at the end of 3 September

- **The FITS and TPM/radiance side of `hera_mars_swingby` is not reorganised.**
- ~~Per-body shadow frusta belong in kalast, not in example scripts~~ **done
  the same evening**, 22:50 and 22:54. `config.shadow_per_body`, on by
  default, gives each body its own shadow layer, and the Sun no longer needs
  aiming — each layer derives its direction from `sun.pos` and the body it
  targets, so `sun.dir` and `sun.anchor` are no longer consulted for
  shadowing. `Eye::anchor_body` replaces the anchor snapshot that went stale
  as soon as a body moved. Measured on Deimos beside Mars: the shared map
  found 49 shadowed pixels where the per-body map finds 249 — it was missing
  most of the shadow, not adding to it. Mutual shadowing verified intact on
  the Didymos/Dimorphos transit (714 px of 1,040,400 differ, all at shadow
  edges). See the 4 September section.
- **`out/hera_mars_swingby/tiri_deimos_fits/` is the pre-timing-correction
  set.** The corrected products live only under `2026-09-03_timing_update/`,
  under `tiri_rad_*` names this machine's image CSV will not reproduce. So
  `tiri_deimos_png.py` and `analysis/tiri_deimos_compare.py` currently read
  stale FITS. Re-running `tiri_deimos_fits.py` here resolves it.
- **`analysis/tiri_deimos_compare.py` mixes epochs** — markers and Mars limb at
  the label epoch, simulated panel at label − 24.89 s — and its docstring still
  states the retracted 0.7 deg pointing conclusion.
- **MSAA is unmeasured.** No before/after on a fitted limb radius or centroid.
  It changes edge pixel values, so measurements from 1-sample and 4-sample
  exports are not the same measurement.
- **Cosmographia: Mars goes almost fully dark when the Hera catalog loads.**
  Cause unknown; that work stopped there.

---

## 4 September — per-body shadow maps, and the trailer strip (macOS)

Landed late on 3 September, so recorded here.

- **One shadow map per body**, `config.shadow_per_body`, on by default. A
  single map has to be fitted to the whole scene, so a small body beside a
  large one gets almost no texels. On Deimos (6 km) beside Mars (3,396 km) the
  shared map broke the terminator into ragged stripes and found **49 shadowed
  pixels where the per-body map finds 249** — it was missing most of the
  shadow, not adding to it. Each layer is *aimed* at one body and sized to it
  but spans the whole scene in depth, so mutual shadowing survives: verified on
  the Didymos/Dimorphos transit, 714 px of 1,040,400 differing, all at shadow
  edges. Bias is per layer, since one texel is a different world distance in
  each. `facet_shadow` reads the queried body's own layer — it feeds the TPM,
  so the wrong layer would have been silently wrong physics. Layers cap at 8;
  beyond that bodies share the last one. `shadow_per_body = false` restores the
  old behaviour for reproducing older output.
- **The Sun stopped needing to be aimed.** It is a light source, not a camera:
  giving it a single `dir` was never physical, and `sun.look_anchor()` was a
  trap — forget it and the Sun points at whatever `anchor` held, which in these
  scripts is the spacecraft. `sun.pos` alone now determines the lighting.
- **`Eye::anchor_body`** — an anchor that tracks a body rather than
  snapshotting where it was, which every animating script previously had to
  remember to redo by hand.
- **`diffuse_lighting_one_image.py` updated** to both, dropping
  `sun.look_anchor()` and the anchor snapshot.

This is the engine change the 3 September reorganisation argued for, and it
deletes exactly the boilerplate that made the examples long.

### Housekeeping

- **The `Co-Authored-By: Claude` trailers are gone**, 31 of them across 113
  rewritten commits back to 25 August. Content is unchanged and dates are
  preserved; `backup-before-strip` holds the pre-rewrite history.

### Still open

- **The FITS and TPM/radiance side of `hera_mars_swingby` is not reorganised.**
- **`out/hera_mars_swingby/tiri_deimos_fits/` is still the
  pre-timing-correction set**, so `tiri_deimos_png.py` and
  `analysis/tiri_deimos_compare.py` read stale FITS on this machine.
- **`analysis/tiri_deimos_compare.py` mixes epochs**, and its docstring still
  states the retracted 0.7 deg pointing conclusion.
- **MSAA is unmeasured** against a fitted limb radius or centroid.
- **`tiri_deimos_frame.py` and `tiri_deimos_movie.py` still hand-roll the
  two-pass composite** they no longer need. Simplifying them onto
  `shadow_per_body` should reproduce their output and would be the test that
  the new path covers what they were doing by hand.
- **Cosmographia: Mars goes almost fully dark when the Hera catalog loads.**

---

## 4 September, later — shadow map: four bugs, one calibration left open

Full write-up in `2026-09-04_shadow_fixes.md`; the short version.

- **The shadow layer's extent was reverse-engineered from its own matrix**,
  as `1.0 / view_proj.x_axis.x`, which yields `side / |R[0][0]|` rather than
  `side`. On Mars `R[0][0]` fell below `f32::EPSILON` and a `1.0` fallback took
  over: a 3,788 km body biased as a 1 km one, **3,788x wrong**, giving a 0.35 m
  normal offset and acne no setting could clear. `fit_light_view_proj` now
  returns its extents. Dark-pixel fraction 8.80% -> 0.07%.
- **`facet_shadow` was using scene-wide bias while the render used per-layer** —
  one scalar across a 403x spread of body sizes. Deimos self-shadowing read
  **0.55%** where it should be ~46%, because a scene-fitted 3.45 km offset on a
  6.6 km body pushes every sample off the geometry. **This is the TPM's
  occlusion input, so it was wrong physics.**
- **The debug light cube corrupted the scene.** `light_render.wgsl` declared
  stale copies of `Globals` and `Light`; a missing field in a second
  declaration shifts everything after it silently. `light_cube_scale` read
  `gamma`, and `pos` came out of matrix data. Regressed at `452ac5d`.
- **Two PCF artefacts**, both absent at `shadow_pcf = 0` (which stays
  bit-identical): a grey crater floor, fixed by scaling the normal offset with
  the kernel radius; and acne on the lit wall, fixed by a per-tap
  receiver-plane bias derived analytically from the facet normal — a
  `dpdx`/`dpdy` version made it 6x worse, since screen-space derivatives are
  meaningless on a flat-shaded mesh.

### Open at the end of 4 September

- **The automatic shadow bias is not calibrated, and today made it worse.**
  The crater self-shadow example has an exact answer — 63.281% of facets
  shadowed at grazing Sun — and automatic scores **38.57%**, against 47.07%
  before today and 63.281% with hand-pinned values. The per-layer fix is
  correct and is what moved it, but the destination is still wrong. **Calibrate
  against the crater**, which is exact, fast and already in the repo.
- ~~`tan(theta)` slope factor~~ **tried and rejected.** `1 - N.L` saturates at
  1 while the required bias diverges as `tan(theta)`, which is real, and it did
  fix the Mars/Phobos comb teeth — but it scored 37.60% on the crater, worse
  than what it replaced. Parked, not committed.
- **`facet_shadow` results from before 4 September are affected**: the Didymos
  phase-1/phase-2 spin-ups, the view-factor work, and the Deimos preroll. The
  mutual eclipse shadowed count moves -13.8%/-14.1%, raising absorbed flux.
- **The Mars/Phobos comb teeth are still there** — they were only fixed by the
  rejected `tan` change.
- **Toggling into macOS fullscreen stalls** 1001 ms (Metal's `nextDrawable`
  timeout) to 3725 ms in `acquire drawable`. Not fixed; `config.fullscreen =
  True` avoids it and is documented.
- **PCF benchmarks must be taken at the working resolution.** Cost is
  per-fragment: +2.5 ms at 800x600, +7.8 ms at 3024x1964.

Discussed and deliberately not done, so they are not rediscovered as
surprises:

- **The `tan(theta)` patch exists only in a scratch directory** under
  `/private/tmp`, which does not survive a reboot. If it is wanted as a
  starting point for the calibration it needs re-deriving, which is cheap:
  replace `1 - N.L` with `min(sin/cos, SLOPE_MAX)` in `mesh_shadow.wgsl`.
- **Shading is single-sided, so `render_back_face` shows a lit underside.**
  Normals are not flipped for back faces, so viewing the crater from below
  draws the plane as though you were seeing its top. Fine for inspecting
  shape, wrong for anything photometric. The fix is one `select()` on
  `@builtin(front_facing)`; it only affects geometry deliberately made
  visible, since closed bodies cull their back faces anyway. Raised, never
  decided.
- **`hud_font` matches on filename, not the family name inside the font.**
  Good enough for the names people type -- Arial, Helvetica, Times New Roman
  all resolve -- but a family whose file is named nothing like it will not.
  Proper matching means a font-database dependency (`fontdb`, `font-kit`),
  which was judged not worth four or five crates for a debug overlay.
- **Residual shadow artefacts are not zero.** At `shadow_pcf = 4`: 710 px of
  acne on the crater's lit wall and 388 px of leak on its floor, both
  concentrated on facet edges. 2% and 5% of what they were, but present.
- **`cargo test` fails on a pre-existing doctest**, `src/py/tpm/gpu.rs:204`
  -- an ASCII diagram in a doc comment that rustdoc tries to compile. It
  masks any real doctest failure, so use `cargo test --lib` meanwhile.
- **Two pre-existing warnings**, `unused variable: shadow` and
  `shadow_meshes`. Harmless, but they are the noise a real warning would
  hide in.
- **Stubs and docs cover the core scripting API, not the whole module.**
  `App`, `Config`, `Hud`, `Simulation`, `State`, `Eye` and `Projection` are
  at 111/111 documented; `mesh`, `entity`, `routines` and the `tpm` classes
  are stubbed but largely undocumented. `python tools/gen_stubs.py` picks up
  any `///` added to them.

---

## 4 to 6 September — figure furniture: axes, colour bar, data colouring

Done on the other machine, pulled on the 7th. These are what turns a render
into something publishable: a measured frame, a labelled scale, and facets
coloured by a physical quantity rather than by shading.

- **Reference axes in four styles**, `config.axes`: `box` (MATLAB's `box on`,
  every edge a ruler), `panes` (matplotlib's `Axes3D`, gridded far panes that
  give depth cues a bare box does not), `gizmo` (three arrows at the origin,
  for fly-throughs where a box would occlude the subject), and `blender` (a
  ground grid with Z picked out). Tick steps round to 1, 2 or 5 times a power
  of ten, so a count lands near `axes_ticks` rather than on it -- ticks at
  0.0347 are unreadable.
- **Facet colouring from data**, `mesh.values` plus `config.value_mode`,
  `colormap`, `value_min`, `value_max`. Colormaps by name or from any 256x3
  array, so a matplotlib one can be handed over unchanged. Orthogonal to
  `color_mode`, which still decides whether the result is shaded -- flat is
  what a quantitative figure wants. *(`value_mode` was removed on the 7th;
  see below.)*
- **A colour bar**, drawn through the same lookup table as the surface so the
  two cannot disagree. `colorbar_source` picks between the data map and the
  diffuse shading, the latter labelled 0..1 and explicitly **not** radiance or
  temperature. *(`colorbar_source` was removed on the 7th; see below.)*
- **Nine HUD anchors**, with `align_h`/`align_v` separating alignment within
  the block from where the block sits.
- **Axis-aligned plane views**, `camera.view_along(axis)`, orthographic by
  default because a perspective plane view is not measurable.
- **The camera reports what it can see**, and warns when the light cube is
  clipped -- which is the failure that made the cube invisible rather than
  wrong, since the camera's far plane fits to scene bounds and the Sun is
  outside them.

### Documented on the 7th

The 22 new options had no `CONFIG.md` entries, `view_along` and `mesh.values`
were not in `API.md`, and `CONFIG.md` still claimed four HUD anchors where the
code now takes nine. All written up now.

Two things worth carrying: `axes_unit` and `colorbar_label` are free text that
**nothing checks**, so a wrong unit mislabels a figure silently; and an
automatic `value_min`/`value_max` rescales per frame, which is the quiet way
to make two images non-comparable.

---

## 7 September — one switch instead of three, and a Python surface you can see

### Three settings collapsed into one

`color_mode`, `value_mode` and `colorbar_source` were three answers to one
question, so the wrong combinations were reachable: a shaded data map, or a
colour bar labelled 0..1 sitting beside a surface showing temperature. None of
them were errors, they just drew a figure that lied.

`color_mode` decides all of it now, and the other two are gone. Mode 1 -- the
unlit mode -- *is* the data map for any mesh carrying `mesh.values`, falling
back to vertex colours when it has none, which is what mode 1 always meant.
Modes 0 and 3 shade, so the bar labels lighting. Mode 2 is a single flat
colour, so **the bar is not drawn at all** -- a colour bar for one colour
carries no information.

Unlit is what a quantitative figure wants anyway: shading a data map makes one
value read as two colours.

### Colormaps take arrays, and there is a function to make them

`config.colormap` accepts a name or a 256x3 array; `kalast.app.colormap(name)`
returns the array, `colormap_names()` lists what is built in. The matplotlib
route -- `plt.get_cmap("inferno")(numpy.linspace(0, 1, 256))[:, :3]` -- works
unchanged, which it did not before: see the dtype trap below.

### HUDs are a list, and they are written from the callback

`config.hud_text` and the `{hud}` placeholder are gone. `config.huds` is a list
of `Hud`, each with its own text, anchor, position and colour; `sim.huds` hands
the same objects to `before_render` so a script can rewrite them per frame, and
a HUD not touched there keeps what it had.

Text is a template: `{it}`, `{nit}`, `{its}`, `{fps}`, `{ms}`, `{et}`, `{time}`
and the rest, all listed in `CONFIG.md`. Rates are averaged over at least a
second and printed as integers unless a format says otherwise -- `{its}`
flickering through four significant figures is unreadable, and nobody needs
1/1000 s of an iteration rate. Anchor is inferred when `x`/`y` are set, so
`Hud(text=..., x=20, y=20)` does not also need `anchor="custom"`.

Font is global, `config.hud_font`, by installed name (`"Arial"`) as well as by
path -- resolved against the platform font directories.

### The Python surface is now visible to an editor

`app.config.<tab>` offered nothing, because an editor cannot introspect a
compiled extension. The repo already carried hand-written `.pyi` files, and
they were **commented out in their entirety** -- so they completed nothing
while looking like the surface was covered, which is worse than having none.

`tools/gen_stubs.py` now generates them from the Rust source for 11 modules,
carrying `///` docs across as docstrings, and `tests/test_stubs.py` checks two
ways that must both pass: the committed stubs match what the generator
produces, *and* every class matches `dir()` on a live object -- the second
catching the generator misreading an attribute, which it did, twice.

Everything reachable from Python now has a doc comment, arguments included.

### Callbacks receive the app

`before_render(sim, dt)` could not reach the config, so nothing could be
changed once the simulation had started. Callbacks now take the app --
`before_render(app, dt)`, with `app.config` and `app.simulation` -- and the
config is a shared handle, so a write from inside a frame takes effect on the
next one. 30 callbacks across 18 examples rewritten.

A first attempt hung the config off the simulation as `sim.config`. That was
wrong and was reverted: `App` owns both, and the callback should see the same
shape the script does before `start()`.

### Two faults found by rendering, not by reading

**`f4bab94` committed the Rust half of the `color_mode` change without the
shader.** `git add src notes` and not `shaders`. The uniform layout still
matched, so it compiled and ran, and drew vertex colours where a data map was
asked for. Committed in `7aedbb6`.

**numpy's default dtype is rejected almost everywhere.** `Float` is `f32`
unless the `use_f64` feature is on, so `PyReadonlyArray1<Float>` refuses the
float64 that `numpy.linspace`, `numpy.zeros` and `plt.get_cmap` all produce.
The message names the problem badly:

    argument 'y': 'ndarray' object is not an instance of 'ndarray'

`config.colormap` and `mesh.values` now take f64, f32 or a plain sequence.
**About 32 other Python-facing arguments across seven files still do not** --
`src/math.rs`, `src/py/mesh.rs`, `src/py/routines/setup.rs`,
`src/py/tpm/column.rs`, `src/tpm/core.rs`, `src/tpm/routine.rs`,
`src/tpm/emit.rs`. Confirmed live, not inferred from the types:
`kalast.math.trapez(numpy.linspace(0, 1, 5), numpy.linspace(0, 1, 5))` fails,
the same call with `.astype(numpy.float32)` returns 0.5.

The fix is mechanical -- extract f64, then f32, then a sequence, as
`mesh.values` does -- but it is 32 sites and none of them were today's subject.
Open.

### `app.step()` — the loop can live in the script now

`start()` owns the loop and calls back into Python, which is why
`before_render`/`after_render` exist at all: they are the only places a script
can reach a frame. `app.step()` inverts that. It draws exactly one frame and
returns `False` once the window has closed, so:

```python
while app.running:
    sim.bodies[0].mat = pose(...)      # what before_render did
    if not app.step():
        break
    lit = sim.facet_shadow(0)          # what after_render did
```

Built on winit's `pump_app_events`. Rendering stays inside winit's handler --
macOS drives drawing from `drawRect` and expects it finished before the
callback returns -- and only the caller's own work happens outside, which is
the arrangement that API is designed for. One `step()` is one *frame*, not one
pump: the redraw a pump requests is delivered by the next one, so it pumps
until the redraw handler has actually run.

This also answers the question the callbacks raised. The `before`/`after`
split existed to enforce one ordering rule -- a GPU result only exists after
the frame is drawn, so request before, read after. A driven loop expresses the
same rule as a position in the body, which is why `step()` belongs in the
*middle* of it and not in the `while` line.

I got that wrong first and wrote `while app.step():`, which puts every line
after the draw: the pose set in a pass applies to the next frame while the
result read in it describes the previous one, a frame apart with nothing
saying so. Caught by the user reading the example, then settled by measurement
rather than argument -- a sun alternating between two elevations on the
crater, with the callback path as ground truth:

| | sun set this pass | `facet_shadow` read this pass |
|---|---|---|
| `before_render`/`after_render` | `y = 20` | `lit = 0.741` |
| `step()` in the middle | `y = 20` | `lit = 0.741` |
| `step()` in the `while` line | `y = 20` | `lit = 1.000` |

`while True:` is wrong for a second reason: after the window closes `step()`
returns at once without drawing, so `state.iteration` stops advancing and a
loop keyed on it spins forever. `while app.running:` with
`if not app.step(): break` is the shape.

The callbacks still run if set; nothing existing changes.

**Cost**: a little more than `start()`, consistently, and not by much. 400
frames a run, release, `vsync = False`, on the 2048-facet crater, run back to
back so each pair meets the same conditions. Quiet pairs, medians of per-frame
time: 0.556/0.584, 0.489/0.537, 0.471/0.505, 0.519/0.681 ms -- `step()` slower
in every pair, by 5--31 %. Under load both inflate and the gap widens
(1.39 → 2.11 ms).

An earlier figure of "1.51 vs 2.06 ms, half a millisecond a frame" is
withdrawn: it came from a loaded window and overstates it. Unpaired, `start()`
alone ranged 0.33--8.30 ms on this machine, which says more about the machine
than about either mode, so only back-to-back pairs are worth quoting. The
window was not frontmost, which makes every figure here a lower bound.

The overhead is what winit documents for macOS, where a pump stops and
restarts the `NSApplication` rather than polling. Irrelevant interactively;
`start()` stays the cheaper way to spend a few hours on an export.

`app.close()` and `app.running` come with it. `close()` cannot exit the loop
itself -- that needs the `ActiveEventLoop`, which only exists inside a handler
-- so it raises a flag that `about_to_wait` acts on, which also means closing
runs the same shutdown the window button does, including flushing queued frame
exports.

Not usable after `start()`: a platform event loop cannot be created twice in
one process. `step()` afterwards reports the app stopped rather than
panicking. Not available on web or iOS.

`examples/crater_self_shadow/step.py` is the worked example: the crater scene
again, so it sits beside the `start()` version of the same thing. `res/` data
only.

Found while writing it: **a non-unit `camera.dir` aborts the process.** The
check panics inside winit's launch callback, which is declared non-unwinding,
so a rounded vector in a script gives `panic in a function that cannot unwind`
and a hard abort rather than a Python exception. Worth normalising in the
setter instead. Open.

### Startup-only options are nearly all live now

The other half of what an interactive editor needs. Seventeen options were
read once, when the window was made, and a later change updated the
Python-visible field while having no effect on anything -- which is worse than
refusing it, since the script and the render disagree silently.

Each frame now compares the config against what the window was actually built
with, and acts only on a difference. The comparison is field-by-field against
the config rather than against a freshly built snapshot, so the common case --
every frame of an ordinary run -- allocates nothing.

| | realised by |
|---|---|
| `title`, `fullscreen`, `width`, `height` | a winit call on the window |
| `vsync` | reconfiguring the surface; nothing recreated |
| `msaa`, `render_back_face` | rebuilding the pipelines -- sample count and cull mode are fixed at creation |
| `shadow_resolution` | a new depth texture, plus the pipeline rebuild that rebinds it |
| `hud_font` | a new brush; a brush owns its glyph atlas |
| `export_dir`, `export_sync`, `export_max_queued` | a new exporter, after finishing the old one |
| the four `sensitivity_*`, `emulate_middle_button` | copied to the controller every frame |

Only `debug_window` and `debug_window_mesh` stay startup-only, and only for
what they print while the window is being built.

Verified by measuring the effect, not by checking it did not crash:

- `width`/`height` -- exported frames go 640x480 then 800x600 across the
  change.
- `export_dir` -- frames land in one directory before it and the other after.
- `shadow_resolution` -- the occluded fraction moves 0.3821 to 0.3770 when the
  map drops from 8192 to 1024, which is what a coarser map should do.
- `msaa` -- 4x to 1x changes 2,750 pixels, all on edges.
- `render_back_face` -- with the camera *inside* `res/cube.obj`, so every face
  presents its back: 0 non-background pixels culled, 65,536 (the whole frame)
  once culling is off. The crater scene showed no difference at all, which is
  correct and proves nothing -- a scene with no visible back faces cannot.

`width`/`height` are a request the window manager may refuse, and the surface
follows the `Resized` event that a granted one produces, so that change lands
a frame or two later rather than instantly.

### The editor -- done, and it is a mode rather than a second way to run

The **interactive GUI** discussed here is built: `python -m kalast` opens a
Blender-shaped window -- scene in the middle, script on the left, log along
the bottom, config and live simulation variables on the right -- with Play,
Restart and Step across the top (`P` and `K`). `python examples/.../main.py`
still gives the plain window it always did; the editor is a flag on the same
`App`, not a second application.

What made it possible was already here: `step()` for control of the loop, and
a live config for changing anything while it runs.

Two of its panels are **generated** rather than written, for the same reason
in both cases -- a panel that silently omits a new field looks complete:

- `src/app/gui/config_panel.rs` from `src/app/config.rs`, guarded by
  `tests/test_config_panel.py`.
- `kalast/**.pyi` from the Rust source, guarded by `tests/test_stubs.py`.

The simulation panel beside it is hand-written on purpose: it shows runtime
state -- facet counts, fitted frustums, HUD text -- where a generator reading
field names would have nothing useful to say about a `Mat4`.

### The editor grew teeth, and four bugs came out of it

Using the editor on real scenes found things that scripts had been quietly
living with. Each has its own note or its own entry above; together they are
the reason this stretch is worth reading.

- **`step()` drew more than one frame.** The redraw handler re-requests a
  redraw on entry, so one pump could dispatch five, and the work done before
  the call applied to only the first. Python's slower loop happened to get
  one, so it took a Rust example to show it.
  `notes/2026-09-08_step_one_frame_and_a_bad_benchmark.md`.
- **The shadow bias lit the night side.** Ten texel-depths of slope term,
  largest exactly at grazing incidence. Calibrated against ray-traced truth;
  `notes/2026-09-08_shadow_bias.md`.
- **The editor viewport was a gamma too dark**, all of it, from one sRGB
  decode too many. `notes/2026-09-08_editor_gamma.md`.
- **`lit` was counting facets nothing was blocking**, not facets in sunlight.
  With the Sun on the far side it read 99.2 % where the answer is 0.
  `sim.facet_illumination` is the quantity; see `API.md`.

What the editor gained, all documented where it belongs rather than here:
bodies that can be reloaded, reshaded, added and removed; HUDs that can be
added, shaped and pinned away from a script; `app.config.toolbar` as a
template; facet selection by clicking; and Rust examples that compile and
launch from the Script panel.

### The editor became a front end for both languages

`python -m kalast` and `cargo run --bin kalast` are the same loop --
`App::run_editor` in the engine -- and both run both kinds of example **in the
window you are looking at**. A Rust example is compiled to a cdylib and
loaded; a Python script is executed by an interpreter the binary embeds.
Nothing is written in an example to make either work, and both still run
standalone from a terminal.

`notes/2026-09-09_editor_hosts_both_languages.md` has the mechanism, the two
checks that keep the two copies of the crate honest, and the three approaches
that were tried and are wrong.

Python is a **default** feature now rather than an optional one, because
running Python needs an interpreter in the process. The property the audit was
written for survives as `--no-default-features`, which is what a cargo feature
is for.

### Still open

- ~32 Python-facing arguments reject float64, so a numpy scalar has to be
  cast at the call.
- A non-unit `camera.dir` aborts the process rather than raising: the panic
  happens inside winit's launch callback, which cannot unwind.
- Shadow bias is not calibrated against the crater's exact 63.281 %. The
  *slope* term now is, against ray-traced truth -- see
  `notes/2026-09-08_shadow_bias.md` -- which leaves the absolute figure open.
- **250 facet-instances over 15 Sun angles** still disagree with a
  centroid ray, all reporting occlusion of 0.5 or 0.75. They are facets
  straddling a shadow edge and a centroid ray cannot adjudicate them; a
  vertex ray is worse, because one launched from a shared vertex escapes
  between facets.
- **A Rust example and the Python module do not compare end to end.** The
  example binary sits pinned at the display refresh whatever the workload
  while the module does not, with the same adapter, the same surface and
  `Immediate` granted to both. Until that is understood, compare the loop
  *body* and nothing else.

### Facet index labels, and a fourth unbound option

`facet_labels` draws each facet's index at its centre, so you can read off
which facet a number in a data product refers to. Two limits, both deliberate:
only facets turned towards the camera are labelled -- the text is a
screen-space overlay with no depth test, so labelling the far side would print
numbers over the surface hiding them -- and no more than `facet_labels_max`
(2000) per body, since a label is a text draw and a shape model has millions
of facets. `facet_label_size` and `facet_label_color` to taste.

Adding it hit **the same trap as `debug_light_cube_fit` the day before**: the
field existed in Rust, the editor had a widget for it, the `.pyi` had a line
for it, and a script still got

    AttributeError: 'builtins.Config' object has no attribute 'facet_labels'

because the `#[getter]`/`#[setter]` pair is the one part of the chain written
by hand, and the stubs are generated *from* the wrapper, so they agree with it
that the field does not exist.

`tests/test_config_bindings.py` closes that: `dir()` on a real `Config`
against the Rust struct, plus a setter check. It found two more the moment it
ran -- **`selection_color`**, documented in `CONFIG.md` as if it were usable
and never bound at all, and **`colorbar_border`**, a checkbox in the editor
and nothing in Python. Both bound now.

That makes three generated-or-checked mirrors of `Config`: the panel, the
stubs, and now the bindings. Adding a field touches one place and the three
guards say so if it does not.

### Per-pass GPU timings

`config.gpu_timing` turns on timestamp queries; `sim.gpu_timings()` and the
`{gpu}` HUD placeholders read them back. From the WebGPU samples'
`timestampQuery`, and the first thing here to measure where a frame goes
rather than infer it.

Two traps, both found by checking rather than trusting the first plausible
number. The per-pass figures **overlap and must not be summed** -- four
bodies report 4.6 ms of shadow passes inside a 3.6 ms frame, since each
figure includes waiting for the GPU to reach that pass -- so what is exposed
is `span`, first timestamp to last, which stays under the wall clock. And
they are a few frames old, because reading them back without blocking means
reading what has already finished; `"frame"` says which iteration they belong
to.

First finding: at one body the GPU spans **1.8 ms of a 3.6 ms frame**. Half
the frame is not the GPU. And `shadow` triples from one body to four while
the geometry does not.

That pointed at the per-layer submits, so they were collapsed into one
encoder -- the layer index reaches the shadow pass as a dynamic offset now,
which removes the uniform-write workaround that forced a submit each. **It
bought nothing**: every difference across one, two and four bodies sits
inside the run-to-run spread. The cost is per *render pass*, not per submit,
which is a question the instrument answered in an afternoon and no amount of
wall-clock timing could have. Collapsing the passes themselves -- multiview,
which this adapter supports, or a shadow atlas with a viewport per body -- is
what would remove it, and is not attempted.

Measured afterwards on the real scene, which changes the recommendation:
Didymos and Dimorphos at 3.1M facets each spend 28.5 ms of a 34.5 ms frame in
shadow passes, of which the fixed per-pass cost is a few percent. So
multiview would be worth single digits there, against roughly half the frame
on the cube. What *is* worth having is already built: `shadow_path` proxies
take the shadow passes from 28.5 ms to 6.5 and the frame from 34.5 to 17.4,
a 2x on the whole frame.

`notes/2026-09-09_gpu_pass_timings.md` has the mechanism, the measurements
and the two bugs the editor path exposed.

### Handed off

`notes/2026-09-09_HANDOFF_gpu_timing.md` closes the day: what landed, what was
tried and abandoned with the measurement that abandoned it, and the three
WebGPU-sample items still open -- reversed-Z first, then primitive picking,
then occlusion queries.


### Reversed-Z

The camera's depth buffer holds 1.0 at the near plane and 0.0 at the far one
now, cleared to 0.0 and tested with `Greater`. First of the three items the
GPU-timing handoff left open, and the one it put first.

`b0a1989` had just floored the near plane at `far * 1e-3` to stop the crater
z-fighting. That works, and it is an assumption rather than a fix: it says no
scene may want a near plane closer than a thousandth of its far one, which
rules out a Hera close approach seeing anything within 100 m of the camera at
a 100 km far plane. Reversed-Z removes the coupling instead of tuning it --
float precision bunches near zero, the perspective divide bunches it near the
near plane, and pointing them at opposite ends cancels the two. The floor is
back to `far * 1e-5` and now only keeps the projection non-singular.

The measurement that says it worked: with the planes pinned from Python so the
floor is out of the picture, a punishing `near/far` of 1e-6 renders **286 px
of z-fighting on the old build and 0 on the new one** -- bit-identical to the
same scene with the near plane 5,000x further out. At a comfortable near plane
the two builds are pixel-identical, so it is inert where precision was never
the problem.

**The shadow map is deliberately not reversed**, which reverses the handoff's
own recommendation. Its projection is orthographic, so its depth is linear and
its precision already uniform -- the cancellation only exists under a
perspective divide, so there is nothing to gain. Against that, the biases in
`mesh_shadow.wgsl` are calibrated against this sense and `facet_shadow.wgsl`
re-derives the same comparison in compute, so flipping it would move the
illumination the thermophysical model runs on for no precision at all. The two
chains were already separate -- the light's matrix never passes through
`Projection::mat()` -- and both sites now say why they stay put.

One trap, of the kind worth remembering: **`colorbar.wgsl` emitted a hardcoded
clip-space `z = 0.0`**. Under `Less` that meant "always draw"; under `Greater`
against a background cleared to 0.0 it meant the bar vanished entirely.
Geometry drawn through the camera matrix takes care of itself; a hardcoded clip
position does not.

Checked for movement four ways against `f3c1baf`, and it moved almost nowhere:
the crater's illumination trace over a 200-iteration Sun sweep is identical
byte for byte at 17 digits, `facet_id_map()` is bit-identical, and the
analytical hemicube validation gives the same 0.07 % error. The one thing that
does move is the view-factor *distribution*: 8 of 10240 pair entries shift by
at most 3.1e-5, four of them across zero, where a facet sits exactly on a
hemicube-pixel visibility boundary. The per-facet totals -- the quantity the
physics uses -- are bit-identical, so the shift is a reassignment between
neighbours two orders of magnitude below the hemicube's own discretisation
error.

Full write-up, including why swapping the two planes in `perspective_rh` *is*
the reversed form, in `notes/2026-09-09_reversed_z.md`.

### Primitive picking

Second of the three items the GPU-timing handoff left open. Most of it already
existed: `src/app/facet_id.rs` has rendered facet indices into an R32Uint
target for a while. What it could not do was answer about **one pixel** --
`render_and_read` copies the whole target, ~12 MB and 2.9M texels unpadded on
the CPU, to answer a question about four bytes. So the work was the readback
shape, not the pass: `read_at` beside `render_and_read`, sharing the render
through a new `record`, and `sim.request_facet_pick` / `sim.facet_pick` in
Python.

The GPU answers *which* facet; one triangle test (`Mesh::intersect_facet`)
answers *where*, so the click handler keeps the lat/lon it prints. `pick_facet`
is unchanged and still public -- it answers what a screen pixel cannot, a ray
from an arbitrary origin such as an instrument boresight.

**The measurement changes the recommendation.** One Didymos body, medians of
60 frames:

| facets | plain frame | + 1x1 pick | + whole map | `pick_facet` |
|---|---|---|---|---|
| 81,708 | 2.98 ms | +1.09 ms | +3.27 ms | 0.83 ms |
| 2,621,156 | 14.24 ms | +3.74 ms | +5.48 ms | 23.08 ms |

At full resolution the GPU pick is **6.2x faster** and nearly flat in mesh
size where the ray is linear; at 100k the ray still wins and both are under a
millisecond. They cross around 100k. Shrinking the readback bought 1.7-2.2 ms
of that -- the second geometry pass is the rest, and is why this is per *click*
rather than something to leave on.

**The first measurement was wrong in a way worth remembering.** Interleaving
the modes round-robin in one run had a 1x1 copy costing *more* than a 12 MB one
-- 33.81 ms against 20.69 -- which is impossible, and that is what made it
obvious. A frame that blocks on a readback drains whatever the previous frame
left in flight, so round-robin charges whichever mode follows a non-blocking
frame for that frame's work. The plain frame reading 0.55 ms at 2.6M facets was
the same artefact: it timed queueing the work, not doing it. One mode per
process fixes it, and the plain frame then reads 14.24 ms. **Any benchmark that
mixes blocking and non-blocking frames measures the ordering, not the work.**

One trap: the id pass draws only flattened meshes, and an indexed body is not
merely unpickable but **absent from the target**, so a body behind it would be
picked straight through it. `pickable_on_gpu` is therefore all-or-nothing --
one indexed body and the whole scene falls back to the ray.

And one regression found by doing this: `select_at_cursor` unprojected
hardcoded clip depths, 0.0 near and 1.0 far, which reversed-Z had just swapped.
The picking ray started on the far plane pointing back at the camera, so a
click would have selected the far side of the body. `pick_facet`'s tests pass
an explicit ray and never touch the unprojection, so nothing caught it. The
construction lives in `Eye::ray_through_ndc` now, beside the matrices that
define the convention, and is tested.

`notes/2026-09-09_primitive_picking.md` has the full write-up.

### Occlusion queries

Last of the three, and the smallest. `config.occlusion_queries` adds a row to
the Visibility panel, and `sim.visibility()` to Python.

The panel's existing counts come from `diagnose`, which tests each body's
bounding box against the frustum -- so "visible" means *could be seen*, and a
body wholly behind another still counts. The queries answer the other question.

**Where they go is the whole design.** Wrapping each body's own draw does not
work: a query counts samples that passed depth *when that body was drawn*, and
bodies are drawn in order, so one drawn first and covered later still reports
its full silhouette. Instead each body's bounding box is drawn at the **end of
the main pass** -- depth test on, depth writes off, colour mask empty -- once
every body has written depth. That makes the answer occlusion against the
finished scene, order-independent, for 36 vertices a body rather than a second
geometry pass. Unculled, `GreaterEqual` rather than `Greater`, and with a real
fragment stage writing to a masked target; the note says why each.

**Free on the GPU.** Span with the queries on and off is indistinguishable --
2.9 ms at 100k facets, 14.2 ms at 2.6M, either way. Wall-clock frame time is
not: the naive A/B gave off 8.84 ms against on 12.54, and repeats put *off*
anywhere from 1.02 to 15.31 while *on* stayed at 13.7-14.6. Adding a readback
forces a sync, so the A/B measured whether a sync happened. Same artefact the
picking benchmark hit that morning, from the other side.

**Two more reversed-Z bugs, both in `diagnose`.** `n &= p.z < 0.0` and
`f &= p.z > p.w` were swapped once near moved to `z = w`, so every body clipped
near was reported clipped far and vice versa -- invisible, since the visible
count is right either way and only the labels exchange. And
`light_cube_clipped`, commented "only the far plane", became the near test, so
it warned on the wrong condition and in practice never fired. Both fixed.

The guard for that needed a second attempt and is the lesson worth keeping:
the first version had one body behind the eye and one past the far plane, and
**passed with the bug reinstated** -- a swap just exchanges which body is
which, and both counts still read 1. Two bodies behind and one beyond makes the
counts unequal, so a swap shows. Always check a regression test fails without
the fix.

`notes/2026-09-09_occlusion_queries.md` has the rest.

### A window that does not steal focus

`app.config.open_in_background` opens the render window without taking the
keyboard, so a script that opens a window per case — or a long run started
while you are doing something else — no longer interrupts what is in front of
it. Asked for directly: "make the kalast app window not take focus so I can
keep my personal work uninterrupted".

**Two things are needed on macOS**, and the first alone does nothing useful.
`WindowAttributes::with_active(false)` orders the window in rather than making
it key. But the *application* activates at launch independently, and winit asks
it to do so ignoring whatever else is in front — `activate_ignoring_other_apps`
defaults to `true`. So the event loop is built with that turned off as well.

Measured rather than assumed: sampling the frontmost application every 400 ms
through a 3.5 s run, three runs each, the default holds the kalast process in
front for the first ~1.2 s and `open_in_background` never takes focus at all.
The run is otherwise identical — same iteration count, and the exported frame
is **pixel-identical** to a focused one, which is worth checking here because
an *occluded* window used to stop the simulation dead.

Two smaller decisions. It is **not in the config panel**: it is read once while
the window is created, so by the time a panel exists to tick it in, the window
it would have governed is open. And it is not called `background` — `Config`
already has one, the clear colour, and `AppConfig` already has `focus`, which
is the panels inside the window. The name collision was caught by
`test_config_panel.py` failing on the wrong struct's field, which is a better
outcome than shipping two `background`s across the two config objects the
9 September handoff already lists as a standing source of mistakes.

Unsupported on X11 and Wayland, where winit cannot ask for it.

### The Blender axes: an infinite grid and a navigation gizmo

Asked for directly, from the reference: "i like the pristine grid example from
webgpu samples website, is it what blender is using" and then "can you move the
gizmo 3d axe from the origin to a corner ... so you can even rotate view using
it on specific axis and also toggle view plane".

**The ground plane of `axes = "blender"` is shaded per pixel now**, not drawn
as line segments. It has no edge, crossfades between three levels a factor of
`grid_major` apart as you zoom -- one grid serves a unit cube and a body 1e4 km
away -- and antialiases itself. Four things had to be right and only the first
was obvious: the line width needs clamping at *both* ends or a grazing view
fills in solid; `fwidth` has to be taken on the world position and divided per
level, not on the divided coordinate; the depth has to be **clamped rather than
clipped**, because near and far are fitted to the bodies and at 40 units out
the frustum is a slab the grid was being cut to; and the fade has to be on
obliquity, since an infinite plane is bounded on screen by the horizon and a
distance fade never reaches its ramp.

Hiding all of them: `RenderPipeline::new` hardcoded `BlendState::REPLACE`, so
the alpha that *is* the antialiasing was thrown away. There is a `blended`
constructor now.

**The axis gizmo moved from the world origin to a corner and became a
control.** Six balls -- `+X +Y +Z` filled and lettered, `-X -Y -Z` as rings --
laid out from the camera basis and drawn back to front. Click one to look down
that axis and switch to orthographic; click the axis already being looked along
to toggle back to perspective, which is the only way back since no key is bound
to the projection. Left-drag anywhere on the widget orbits, which it does
nowhere else. `-Z` looks up at the scene from underneath, so `view_along` grew
a `positive` argument in both languages.

The arrows it replaces were only readable when the origin was in shot and not
behind the body, changed size with the zoom, and could not be clicked.

Verified by driving a running window from PowerShell: posted window messages
for the clicks, and `SendInput` with the target granting
`AllowSetForegroundWindow` for the drag, since raw mouse motion only reaches a
focused window. The handoff records the technique -- it is the way to test a
pointer gesture here.

`notes/2026-09-09_HANDOFF_axes_grid_gizmo.md` has the failures in full, the
traps, and what is left: the editor's letterboxed viewport is untested for
clicks, and there is no way to keep the widget out of an exported frame while
keeping the grid.

---

## 10 September — the grid's far plane, and a smaller gizmo (Windows)

Short session on top of the previous day's axes work.
`notes/2026-09-10_grid_far_plane_and_gizmo_size.md`.

**The infinite grid was not infinite, and it was the far plane after all.**
Spotted by eye. `grid.wgsl` clamped its depth to `0.0` past the far plane,
meaning to pin it to the farthest value so bodies still occlude it — but
depth here is reversed, so far *is* `0.0`, the buffer is **cleared** to `0.0`,
and the test is `Greater`. `0.0 > 0.0` is false, so every fragment past the
far plane was discarded by the depth test against empty background. The clamp
now floors at `1e-7`, an epsilon inside the far value; real geometry sits
order `near/far` ≈ 1e-3 above that, so bodies still win. The grid runs to the
frame edge now.

The obliquity fade was the wrong suspect and was ruled out the only way that
works here: `grid_fade_near = 0.9` gave a **pixel-identical** frame. Both
mechanisms bound the grid from the same direction, so only re-rendering
separates them.

**The gizmo shrank and moved.** `gizmo_size` 54 → 40, ball ratio 0.26 → 0.22,
`gizmo_label_size` 13 → 9, and the default anchor `top-left` → `top-right`.
Hit-testing is the part that could regress at 11.8 px of target, so it was
driven rather than assumed: a posted click on the `+Z` ball takes the camera
to `(0, 0, -1)`.

**`maturin develop` stopped failing with `os error 32`.** The long-standing
Windows lock on `kalast/_rs.pyd` — held by VS Code's language server, not by a
kalast window, and therefore present most of the time. Windows refuses to
*write* a mapped image but still allows *renaming* it, so `tools/develop.py`
moves the old module aside to free the name and lets maturin write a fresh
one. macOS never had this because unlinking a mapped file is allowed there.

## 10 September, later — a code quality audit, and pinning the shadow path

`notes/2026-09-10_code_quality_audit.md` and
`notes/2026-09-10_pinning_the_shadow_path.md`.

Asked whether 280 commits and +46k lines in 17 days had left poor code behind.
Measured rather than judged. The verdict: **the practices are good and the
physics is under-defended** -- 61 Rust tests of which 51 are app plumbing and
**2** are physics, zero tests in `mesh.rs` (1,425 lines of geometry),
`radiance.rs`, `roughness.rs` or `facet_shadow.rs`, and all four Python test
files testing generated code rather than any formula.

Also found: the whole physics core runs **f32** (`use_f64` exists, is not
default, and maturin does not enable it), so the CPU/GPU TPM agreement of
1.5e-05 K may be measuring the precision floor; Planck's law implemented twice
with the numpy copy redefining its own `_H`/`_C`/`_KB`; 12 dead shaders; and
134 `.unwrap()`s, 28 at the Python boundary.

**The audit's headline finding was wrong and is corrected in place.** It
called the compute/render shadow divergence a physics bug. Reading the Rust
caller first showed two of the three claimed divergences do not exist, and the
third -- the compute path not filtering with PCF -- is correct: the Sun is a
point source, occlusion is binary, and the bias was fitted at single-tap
against ray-traced ground truth. The real defect was two docstrings claiming
the paths "cannot disagree", plus no test anywhere.

Both fixed. `tests/test_facet_shadow.py` asserts the `shadow_pcf` invariance
and re-runs the ray-traced sweep as a regression bound. **Its first version
passed against a shader with the bias multiplied by 100** -- one favourable
Sun angle moved false-lit 18 -> 24 out of 1540. Fifteen angles and an
assertion on the *worst* one separates healthy from broken by 10x where the
aggregate manages 2.3x. Budgets are set from that measured pair, and the test
was confirmed to fail on the broken build before being kept.

## 10 September, later still — the conduction solvers, pinned

`tests/test_conduction.py`. Item 6 of the audit's test backlog, and the
highest-value one: `explicit.py`, `implicit.py` and `nonuniform.py` are 838
lines of numerics that nothing in the suite touched.

**The validation already existed and was not a test.**
`examples/analytical/sinusoidal.py` has checked the solvers against the
analytical damped thermal wave from the start -- eight error figures and an
order-of-accuracy table -- and *prints* them. A regression was only ever
caught if somebody ran it and read the output. This is that example's numbers
turned into assertions; the example keeps the plots and the commentary.

Three layers, weakest to strongest: per-configuration error budgets against
the closed form; amplitude decay and phase lag pulled out of the numerical
solution by a least-squares fit, which consults no analytic formula and so
survives an error in one; and observed order of accuracy against a
time-converged reference on the same grid -- 1 for backward Euler, 2 for
Crank-Nicolson and BDF2. That last is the one an error budget cannot replace:
the 16-node grid's spatial error is 0.7 K, so a second-order scheme quietly
dropping to first order still passes every budget.

Confirmed to have teeth by changing the second difference in
`src/tpm/core.rs` from 2.0 to 2.02 -- a 1 % error. Decay error goes
0.0001 -> 0.1604, phase lag 0.0040 -> 1.4351 rad, and the analytic error to
300 K. Restored and re-verified afterwards.

Two things found while writing it. **A raw DFT sum was the wrong estimator**:
the sample window spans one period only to within a timestep, and the leftover
fraction leaks the constant mean into the oscillating component -- 0.21 rad of
phase error on a solver whose amplitude was already right to 0.5 %. Least
squares against `[1, cos, sin]` fixed it. And **`skin_depth_1` and
`skin_depth_2pi` documented their period argument as "density"** -- formulas
correct, comments wrong, which is how a wrong argument returns a plausible
number. Both corrected and now pinned.

Runs in 2.2 s, pure numpy, no GPU and no window.

## 10 September, last — view factors and self-heating, pinned

`tests/test_view_factors.py` and `tests/test_self_heating.py`. Same
conversion as the conduction test: `examples/analytical/view_factors.py` and
`cavity_heating.py` computed all of it already and printed it.

**View factors** against two closed forms — parallel coaxial unit squares, and
perpendicular squares sharing an edge, which is the hard case because a shared
edge puts sub-pairs at arbitrarily small separation. Plus convergence under
subdivision, and reciprocity, which is an identity so any deviation is
implementation error rather than truncation.

The check worth keeping is the last: `view_factor_facets` guards the
point-to-point form with a zero return below `sqrt(area)`, and adjacent facets
sit exactly at that threshold — so on the neighbours that dominate
self-heating inside a concavity it **deletes** them, 37.8 % low on the
perpendicular pair. That number is now in the suite rather than in a note, so
the cheaper form cannot come back as an optimisation.

**Self-heating** on a sealed isothermal box, which is the configuration whose
answer needs no reference: row sums must be 1, and a black cavity must be in
equilibrium. At `eps < 1` the absorbed/emitted ratio is `eps` exactly.

Both tests verified by breaking the code. Dropping the emissivity from
`heating.emitted()` **passes at eps=1 and fails only at eps=0.9** — which is
why both are tested, and a reminder that a conservation test at the
convenient parameter value proves less than it looks. A 3 % error in the
view-factor kernel fails three of seven checks but *not* reciprocity, since a
uniform scale preserves it; the closed forms and the identity catch different
things and both are needed.

Also fixed: `cavity_heating.py` could not run on Windows at all, writing its
box mesh to a hardcoded `/tmp`.

Coverage now: 61 Rust tests and seven Python test files, four of them physics.
The audit's backlog is down to radiance band integration, roughness and
transient conduction — and `examples/analytical/` already holds the method for
all three.

## 10 September, closing — f32 confirmed, and one Planck

`notes/2026-09-10_f32_decision_and_one_planck.md`. Findings 2 and 3 of the
audit, both closed.

**f32 stays, as a decision rather than a default** — geometry throughput, with
3.1M-facet arrays halved and uploaded with no conversion since WGSL has no f64
regardless. Its costs are written down: physics constants exposed to Python
round to f32, `exp` overflows at 88 rather than 709 (measured to be outside
the 6-16 um, 30-500 K working range — no sample zeroed), and the CPU/GPU TPM
agreement of 1.5e-05 K is a bound at the resolution floor rather than a
measurement. Drift over a seasonal run's ~1e6 conduction steps is the one case
still unbounded.

**Planck is one formula now.** `radiance.py` had a second numpy copy with its
own `h`, `c` and `k_B`, while `kalast/util.py` already re-exported those from
Rust. `radiance.planck` broadcasts and calls the new `emit.planck_array`;
broadcasting stays in numpy, the Rust side loops over one expression, and the
documented contract is unchanged. The f64→f32 change was measured *before*
switching: worst 1.2e-5 pointwise, 3.8e-6 on the band table, against the
5.1e-5 interpolation error the table already accepts.

**And the test found a real defect in the surviving implementation.**
`tests/test_planck.py` checks Wien and Stefan-Boltzmann — consequences
involving constants the formula does not mention — plus both asymptotic
limits. Rayleigh-Jeans failed twice, for different reasons. The first was my
tolerance: `B/B_RJ = 1 - x/2 + O(x^2)`, so a 1.2e-2 deviation at 2 mm is
physics, and only its *rate* is testable. The second was real:
`exp() - 1.0` in f32 destroys nearly every digit when the exponent is small,
missing the limit by 2 % at 50 mm. `exp_m1` fixes it.

The deleted numpy copy had used `expm1` all along — so the merge would have
silently traded away a correctness property of the code being removed, in a
regime nothing here exercises, had the limit not been tested. That is the
case for testing a merge rather than eyeballing that two formulas look alike.
They did look alike. One was better.

## 10 September — a new project: ground-based lightcurves, for Eli

**New, and not previously in these notes** — searched for it before writing
this, since it had possibly been raised before: no mention of lightcurves,
Eli, or ground-based photometry anywhere in `notes/`. Recording it here so the
next session does not have to ask again.

The goal is simulating asteroid lightcurves as seen from ground-based
telescopes. Attached to it, a question about Brož's polygonal partial
shadowing/visibility algorithm — worth adopting?

Assessed in `notes/2026-09-10_polygonal_shadowing_assessment.md`. Short
version: **yes, as an additional path for photometry; no, not as a replacement
for the shadow map.** Each facet is projected twice — once along the Sun
vector, once along the observer vector — and clipped analytically against the
others by 2D polygon intersection, giving an *exact* real-valued lit-and-
visible area per facet. Brož reports < 0.1 mmag light curves at 42 nodes per
sphere.

kalast's `facet_shadow` samples four points per facet, so a facet is 0, 25,
50, 75 or 100 % shadowed and nothing between. That is right for the
thermophysical model, where the quantisation averages out over a rotation, and
wrong for a light curve, where the summed lit area *is* the observable. kalast
also computes no partial visibility at the limb at all, which is the entire
signal during a mutual event's ingress and egress.

The polygonal method is CPU pairwise clipping — good to ~1e4 facets, useless
at the 3.1M the shadow map handles in one pass. Different regimes, not
competitors.

The slides that prompted it (`11_Broz.pdf`, Les Houches 2024) turned out to
be on the ROB cloud -- the same host `README.rst` uses for `res/` -- and are
read now. They agree with the paper and add three things: the clippings are a
sequence of **three** (Sun, observer, pixel), the scattering law is **Hapke**,
and **Xitau's source is available** (F90, `xitau_20240124_POLYS`), so the
reference implementation can be read rather than reconstructed. The lineage is
Phoebe2, the eclipsing-binary code -- so the polygon-clipping half is mature,
and self-shadowing is the part Brož added.

One caveat survives: **neither the paper nor the 96-page deck gives a single
timing.** The only CPU figure in the deck is "1 week on 100 CPUs", and that is
mapping local minima in the dynamical fit, not the cost of the algorithm. So
every performance statement is inference from the method.

**The likely bigger gap is not shadowing.** Optical lightcurves need a
bidirectional scattering law — Lambert, Lommel-Seeliger, Hapke — and `grep`
finds none of that in `src/`; `roughness.rs` is the Kuehrt crater correction,
a different thing, and `tiri_deimos_photometry.py` is thermal. Exact lit areas
feeding a missing reflectance model buy nothing, so the order is probably
scattering law first.

**Measured, and the case is made.**
`examples/analytical/shadow_quantisation.py`: a cratered icosphere over 24
rotation phases, disc-integrated three ways off one ray tracer so only the
sampling differs. The quarter-facet quantisation costs **0.68 mmag rms at
best** — 5120 facets with only 5.7 % of the illuminated area shadowed —
rising to 3.9 mmag for an ordinary 1280-facet moderately cratered shape, and
40 mmag for a coarse rough one. Brož's target is 0.1 mmag.

Refining the mesh does not rescue it: the error falls as `N^-0.5` to
`N^-0.85`, so reaching 0.1 mmag from the *best* case needs 50,000-240,000
facets — one to two orders beyond what shape inversion produces, to buy what
polygon clipping gets exactly at 320.

Two things worth keeping from doing it. The reference had to be checked for
convergence: a first pass at 91 samples per facet reported ~10 % low, because
`q4 rms` keeps climbing until the reference stops moving. And **a
correction** — this entry previously said partial visibility at the limb was a
comparable second gap. On this test it is not; binary visibility costs little
beside the shadow quantisation for a single body. That claim was about mutual
events, which remain untested.

## 10 September — the photometry, both halves

Asked for after the Brož assessment: implement it. Two modules, and kalast can
now compute a visible-band light curve, which it could not before.

**`kalast.scattering`** — the reflected-sunlight half, which did not exist at
all. Lambert, Lommel-Seeliger, the `c·LS + (1−c)·L` mix, and Hapke IMSA with a
two-lobe phase function, opposition surge and Chandrasekhar `H`. Hapke's
macroscopic roughness is refused rather than ignored: `theta_bar = 0` is
exact, so this is a complete model of a smooth surface rather than an
approximate one of a rough one.

Two bugs, both caught by tests and both worth remembering. The **`mu0`
convention was inconsistent** — Hapke's own `r` folds `cos i` in and the other
laws do not, so swapping laws would have changed the answer by `cos i`, which
surfaces as a wrong pole solution rather than as an error. And the
**Henyey-Greenstein lobes were backwards**, the exact error the doc comment
warned about: `alpha = 0` is backscatter while the textbook form is written in
the scattering angle. Normalisation cannot see that — swapping the lobes
leaves the sphere average at exactly 1 — so direction is tested separately.

**`src/shadowing.rs`** — exact partial shadowing by polygon clipping, Brož's
method. No Clipper2: our facets are *triangles*, so `A \ B` decomposes exactly
into at most 3 convex pieces by half-plane clipping, which needs no dependency
and no C++ toolchain in the `maturin develop` path.

The bug that mattered: **deciding which facet is in front at the two triangles'
own centroids is wrong**, and it is the obvious thing to write. It disagreed
with a converged ray trace by up to 0.435 on facets near grazing incidence,
where a facet is nearly edge-on and its centroid depth says nothing about the
depth where it meets an occluder. Deciding at the *overlap* centroid, via
Brož's own back-projection, took it to 0.09. **The area-weighted aggregate
agreed to 2e-4 the whole time** — a disc-integrated check would have passed;
only the per-facet comparison found it.

Validated by convergence rather than by tolerance: refining the ray trace
moves it steadily onto the clipper (0.2392 → 0.0452 max gap over 45 → 1225
samples). Worth **5-10x** on a light curve — 0.68 → 0.07 mmag at 5120 facets,
under Brož's 0.1 — and the residual is the reference's error, not the
clipper's, since refining the reference drives it to zero while the q4 column
rises to its true 3.99.

Timings, which neither the paper nor the 96-page deck gives: 7, 34 and 156 ms
at 320, 1280 and 5120 facets, roughly linear thanks to a projected
bounding-box grid. **It does not replace the GPU shadow map** — that stays for
the thermophysical model and for 3.1M facets.

Suite: 77 Rust tests, 10 Python files.

## 11 September — mutual events, and a claim finally checked

`examples/analytical/mutual_event.py`,
`notes/2026-09-11_mutual_events_measured.md`. The last open item from the
Brož assessment: it claimed partial visibility was a gap comparable to the
shadow quantisation, the single-body measurement contradicted that, and the
correction noted the claim was really about **mutual events** and untested.

Two spheres edge-on, secondary at 0.3 primary radii, Sun 20 degrees off the
observer so transit and eclipse separate. Event depth 299 mmag.

| | rms in event |
|---|---|
| shadowing quantised to quarters | 3.01 mmag |
| visibility binarised per facet | 2.10 mmag |
| both, which is what kalast does | 2.40 mmag |
| neither, polygon clipping | 0.59 mmag |

**The original claim holds in this regime and only this one.** Binarised
visibility is comparable to the shadow quantisation during a mutual event,
where on a single body it was negligible beside it — so the instinct was right
about mutual events and wrong about the case it had been measured against,
which is what the correction said.

**The two approximations partially cancel**: together they cost less than the
shadowing term alone. Not what I would have predicted, and it means fixing one
in isolation buys less than measuring it in isolation suggests.

**And the error is confined to the events** — baseline 0.00 mmag for every
method, since two smooth convex spheres have no partial facet anywhere until
one crosses the other. The same code has a completely different error
structure depending on the scene, which is the argument for measuring rather
than reasoning about it.

49 ms a phase at 1600 facets. The ray reference needed the bucketed tracer to
finish at all — brute force did not complete in ten minutes.

---

## 11 September — handoff

`notes/2026-09-11_HANDOFF_audit_and_photometry.md` covers 10-11 September:
21 commits, the audit and its own correction, the test backlog from 4 Python
files to 11, and both halves of the photometry.

The open items, in order: **`theta_bar`** (Hapke's macroscopic roughness,
refused rather than implemented — the last known gap in the photometry); the
audit's remaining backlog of radiance band integration, roughness and
transient conduction, all three of which have their method already sitting in
`examples/analytical/`; the smaller audit items (dead shaders, unwraps at the
Python boundary, `SOLAR_CONSTANT` provenance); and `test_stubs`'s
hand-maintained case list.

The lightcurve project for Eli has its pieces now and no driver.

## 11 September, later — the light curve driver

`src/lightcurve.rs`, `notes/2026-09-11_lightcurve_driver.md`. The handoff's
closing line was "the lightcurve project for Eli has its pieces now and no
driver." It has one.

`scattering` gave a facet's reflectance, `shadowing` its lit and visible area,
and nothing summed them — the disc integral existed only inside two analytical
example scripts, hand rolled in numpy twice. Now `flux()` for one epoch of
placed geometry (which is how a **binary** goes through it) and `lightcurve()`
for a rotating body, with the Sun and observer carried into the body frame
rather than the mesh rotated. Spin state in convex inversion's convention, so a
published pole solution needs no translation. `examples/lightcurve/main.py`
runs on `res/` alone.

**The convex shortcut is exact, not an approximation**, and it is worth
11.6 ms → 0.021 ms an epoch at 1280 facets: a convex shape has no facet
occluding another, so both clipping passes return 1. A 90-point curve costs
1.9 ms instead of 1.0 s, which is what a fitting loop cares about.

Tested against closed forms, one per thing that could be wrong: a
Lommel-Seeliger body at zero phase **is** its projected area exactly, so a 2:1
ellipsoid's amplitude is `2.5 log10(a/b)` — 752.575 against 752.575 mmag; a
Lambert sphere follows the analytic phase function; occlusion is a no-op on a
convex shape and each pass separately is not on a concave one; and Helmholtz
reciprocity.

Four things came out of it.

**The first version of the concave test had a dead Sun pass and passed.** It
asserted only that occlusion *changed* the flux. It did, by 6 mmag — all of it
from the observer pass, because at that Sun direction the shape self-shadowed
nothing at all, to six figures. Each pass is now tested alone. A test that
something "has an effect" does not say which something.

**The ellipsoid gives an exact identity and a discretisation error at once,
and they behave oppositely.** The amplitude is right at 80 facets and *worst*
at 5120, where it is f32 rounding; the curve shape converges as `N^-0.94` and
is exact nowhere. A fixed tolerance on the second measures the mesh — the same
lesson as the conduction reference and the polygon clipper, from a third
direction.

**Five deliberate breaks, two of which exactly one test caught.** A mirrored
rotation is invisible to every curve check, because the ellipsoid curve is
symmetric in time — only the Rust `rotation_is_prograde` test sees it, and
without it a real target's light curve would come out time-reversed with
everything green. Running both clippings along the Sun is caught only by
reciprocity, since "occlusion has an effect" still holds.

**Two pre-existing defects, both found by writing the example rather than by
looking for them.** Every generated stub's numpy annotations were
`numpy.object` — an attribute numpy removed in 1.24 — because `resolve()`
rewrote dotted names identifier by identifier: **53 of them across 9 committed
stub files**, each one an error of exactly the kind that function exists to
prevent. And `test_stubs`' hand-maintained case list (open item 4 of the
handoff) is now loud rather than silent:
`test_every_generated_stub_class_is_covered` fails when a stub class is in
neither the case list nor an explicit `UNCOVERED` set. It immediately found
two classes a manual count had missed, and documents 16 that nothing checks.

Suite: 82 Rust tests, 12 Python files. `theta_bar` is now the only known gap
in the photometry — and it is one that can finally be *measured*, since there
is a curve to measure it on.

## 11 September, last — Hapke's roughness, and a wrong estimate corrected

`notes/2026-09-11_hapke_roughness.md`. `theta_bar` was the last known gap in
the photometry and is now implemented: Hapke (1984), effective cosines plus a
shadowing function, with the azimuth recovered from `(mu0, mu, alpha)`.

**The estimate it replaces was wrong by an order of magnitude, and that is the
part worth keeping.** Before implementing it I placed it by analogy against the
other Hapke parameters, which *could* be measured — tripling `w` moves a
normalised rotation curve 3-5 mmag, deleting the opposition surge 1.4 — and
concluded a few mmag, under the photometric noise. Measured: **23 mmag rms at
20 deg phase, and a 15 % change in the curve's amplitude**, which is the
quantity an axis ratio is fitted to. The analogy failed because `w` and `b0`
move the curve's *level* while roughness acts near the limb and terminator,
whose share of the disc changes as an elongated body turns. Every other term
here was settled by measuring it; this one had been settled by argument.

On a disc-integrated sphere it is a phase-curve effect and much larger: 0.6
mmag at opposition, 224 at 60 deg, 600 at 100 deg for `theta_bar = 30 deg`.

**Two blind tests, each blind differently, and only breaking the code showed
it.** Reciprocity is the natural test — the `i <= e` and `i > e` branches exist
to preserve it — and it catches a wrong term inside a branch at once. But
swapping the branches *wholesale* leaves it completely green, because they are
each other's mirror image: exchanging them preserves the symmetry they were
built to provide. `S = 1` at zero azimuth catches that, holding on one branch
only.

And that test was blind in turn: its first geometries were all near-normal
incidence, where `E2` underflows, the correction drops out of *both* branches
and `S` is 1 either way. It discriminates only where both angles are far from
normal. A new way for a test to be unable to fail — not a loose tolerance, not
a conservation law at a convenient value, but a test sitting where the term it
probes has underflowed.

87 Rust tests, 12 Python files. The photometry's forward model is complete;
what it lacks now is a fit — observed curves, a chi-squared and a minimiser.

---

## 11 September — handoff, macOS

`notes/2026-09-11_HANDOFF_lightcurve_driver_and_roughness.md` covers this
machine's half of the day: the disc-integral driver, Hapke's `theta_bar`, and
the two pre-existing stub defects found while writing the example. 4 commits,
29 files, +2,658 lines. 87 Rust tests, 12 Python files, `cargo test --release`
green end to end.

**The photometry's forward model is complete.** The largest gap is now that
nothing reads *observed* photometry: a fit needs real curves, a chi-squared
and a minimiser. After that, a two-body convenience so a mutual event does not
need its geometry placed by hand.

The thermal line is untouched and still queued behind it: the GPU TPM port,
then the GIS3D TIRI re-run with heating on.

## 15 September — a locale bug in `decimate.py`, and why `qualitythr` is 0.6

Found while cleaning the examples, from the question "the default is 0.3 and
we set 0.6, why not 1.0?"

**`examples/mesh/decimate.py` was silently producing garbage on the macOS
machine.** `LANG=fr_BE.UTF-8` means a comma decimal separator, and MeshLab
parses OBJ floats through the C locale, so every coordinate truncated at the
dot: the 3.1M Dimorphos loaded as a bbox of ±9 with volume **exactly 0**, and
decimation wrote that out with no error at all. The first sweep reported a
99.99 % area loss, which is what gave it away.

The obvious fix does not work. `locale.setlocale(LC_NUMERIC, "C")` before the
import is undone by pymeshlab initialising Qt, which calls
`setlocale(LC_ALL, "")`; setting it *after* the MeshSet exists works but a new
MeshSet undoes it again, and `decimate()` builds one per call. Only
`os.environ["LC_ALL"] = "C"` before `import pymeshlab` survives, and that is
now in the file with the reasoning beside it, because its *position* is
load-bearing and a tidy-up would break it silently.

**No data was harmed.** The shipped 10k Dimorphos has the right bbox and an
area of 0.074736 against the source's 0.0746914 — +0.06 %, exactly the figure
`2026-08-31_view_factors/` reports, so the 1 September re-cut ran under a C
locale. Nothing else in the repo uses pymeshlab.

**And the actual question, measured.** Same recipe and target, only
`qualitythr` varied:

| qualitythr | faces | area err | min triangle quality | quality < 0.3 |
|---|---|---|---|---|
| 0.0 | 2,690,976 | -0.013 % | 0.0512 | 7409 |
| 0.3 (default) | 10,000 | +0.067 % | 0.0609 | 110 |
| **0.6** | 10,000 | +0.060 % | **0.1216** | 100 |
| 0.8 | 10,000 | +0.056 % | 0.0838 | 103 |
| 1.0 | 10,000 | +0.056 % | 0.0838 | 103 |

**0.8 and 1.0 are a byte-identical mesh** — max vertex delta exactly 0 — so the
penalty saturates between 0.6 and 0.8 and 1.0 is not a stronger setting, only a
rounder number for the same one. 0.6 has the best *worst* triangle of the five,
so raising it makes the sliver that matters slightly worse. 0.6 turns out to be
the peak rather than the compromise it was presented as, which nothing had
shown before.

My first explanation of the parameter was also wrong: I said raising it trades
geometric fidelity for triangle shape, and it does not — area error *improves*
slightly across the range. The note's table stopped at 0.6, so neither claim
had ever been checked.

## 15 September, later — two example "tests" become real ones, and find two bugs

`examples/mesh/test.py` and `examples/mesh/test_intercept.py` were never
examples: 126 assertions between them, no window, no output, nothing to read.
Living in `examples/` meant nothing ran them, so the only coverage of the
flatten/smoothen round-trip and of the four ray helpers
(`is_point_in_or_on_triangle`, `intersect_plane`, `is_facing_plane`,
`intersect_triangle`) was invisible to the suite. They are now
`tests/test_mesh.py` and `tests/test_mesh_intersect.py`, grouped into named
functions so one failure no longer hides every check after it.

Converting them turned up two defects, both of which had survived precisely
because the assertions could not fail.

**`flatten()` leaves `indices` stale, and the old test asserted the wrong
values as expected.** It rebuilds `vertices` to three rows per facet and never
renumbers `indices`, so every Python accessor reading through them —
`get_facet_indices`, `get_facet_positions`, `get_facet_normals`,
`get_facet_colors`, `get_facet_vertices` — returns the wrong three rows for
every facet but facet 0, whose stale indices are `[0, 1, 2]` regardless.
`src/mesh.rs`'s own `get_facet_vertices` has an `is_flat()` branch that chunks
by three and is correct; the Python path does not use it and indexes through
`facets_matrix_array` instead. Two implementations of one question, one right.
`notes/API.md` claims "after `flatten` these are `0..3f`, one per row", which
is what it should do and does not. **Nothing outside the test calls those
accessors after a flatten**, so no result depends on it. Pinned as a defect in
`test_facet_accessors_read_stale_indices_after_flatten`, so a fix fails loudly
rather than passing unnoticed; the real invariants are asserted on
`positions[3f:3f+3]` instead, over all twelve facets rather than three.

**Fourteen one-sided comparisons, and four wrong expected values behind
them.** Every numeric check in the intercept file was
`numpy.all(got - want < tol)` with no `abs()`, so a result of `-99` against an
expected `1` gives `-100 < tol` and passes. Made two-sided, four expectations
failed: the 45-degree ray from the origin was written as hitting `-0.30834377`
where it hits `-0.30872506`, off by **3.8e-4**, and the one from `z = 0.4` by
6.6e-5; two more were literals rounded to four decimals. Each replacement was
verified against the crossing computed independently in numpy from the facet's
own corners — the engine agrees with that to 1.7e-7 everywhere, so the code was
right all along and only the expectations were wrong. The original's closing
remark that a 1.2e-7 difference was "f32 float precision" was reasoning about
numbers nothing had checked.

Also: the runner now catches `Exception`, not just `AssertionError`. A
list/array mix raising `ValueError` aborted the whole file mid-run, which hides
every test after it — the file-scale version of the same problem.

`examples/mesh/` is down to `decimate.py`. 14 Python test files.

## 15 September, last — `flatten` renumbers its indices, and `recompute_facets` stops returning NaN

Chased from the defect the mesh-test conversion pinned an hour earlier, and it
was worse than the accessors it was found through.

`flatten()` rebuilds `vertices` to three rows per facet and left `indices`
holding the shared pre-flatten values, so facet 1 still pointed at rows 1, 3
and 4 where its corners had moved to 3, 4 and 5. **The engine is mostly
`is_flat()`-aware and that is why nobody noticed**: `gpu.rs` draws a flat mesh
with `pass.draw(0..n_vertices)` and ignores the index buffer entirely,
`intersect_mesh` chunks the vertices by three, and so does `Mesh::
get_facet_vertices`. Rendering, picking and ray casting were all correct.

**`compute_facets` is not `is_flat()`-aware**, and that is the one that
mattered. `recompute_facets()` on a flattened cube gave **NaN normals** off a
degenerate triangle -- 10 of 12 facets wrong, NaN total area -- from a method
`notes/API.md` documents as the thing to call after moving vertices, on meshes
every render example loads with `flatten=True`. Nothing in the repo did call
it on a flattened mesh, so no result was ever poisoned, but nothing stopped it
either.

Fixed at the source rather than by teaching each reader to branch: `flatten`
now takes the shared indices into `_indices_before_flatten` and sets
`indices = 0..3f`, and `smoothen` puts them back. Which is what `API.md` has
claimed all along -- "after `flatten` these are `0..3f`, one per row" -- so the
documentation was right and the code was not.

`flip_facets` needed one change to keep the invariant: it swapped the vertex
pair *and* the index pair, which with meaningful indices is a double swap that
cancels. It now does one or the other.

Everything downstream improves or is untouched: the GPU ignores them, `n_facets`
is unchanged (the length never varied), and the `is_flat()` branches in
`intersect_mesh` and `get_facet_vertices` now agree with the un-branched path
instead of diverging from it. Verified by the whole suite, `test_facet_shadow`
included -- it loads with `flatten=True` and drives the GPU shadow map, so the
render path is checked rather than assumed.

90 Rust tests (3 new, 2 of which fail without the renumbering) and 14 Python
files. The pinned defect test is now a correctness test, which is what pinning
it was for.

## 15 September — `vsync` defaults to off

`app.simulation.config.vsync` was `true`; it is `false` now. With it on, a GPU
faster than the display reports the refresh rate and nothing about the scene —
on a 239 Hz panel the loop measured exactly 239.46 it/s whatever the
complexity, and on a 120 Hz one it produced a "3.1M facets costs 2x"
conclusion that was entirely the panel. **Fifteen lines across the examples
already set `False` by hand** — the sign of a wrong default rather than of
fifteen careful authors — and CLAUDE.md carried it as a standing instruction to
remember. (First written here as "nine scripts", off a `grep` truncated by
`head -20`; the real count came out when they were removed.)

The price, stated in the field's own doc so nobody has to rediscover it: an
uncapped viewer redraws as fast as the GPU allows, so a still image spins the
fan and can tear. `True` is now the thing you set when looking at a scene
rather than timing one.

`config_panel.rs` and the stubs regenerated, `CONFIG.md` and the benchmarking
section of `CLAUDE.md` updated — the latter now says "must stay `False`"
rather than "set it to `False`". Scripts that still set it explicitly are
harmless and were left alone.

## 15 September — the redundant `vsync = False` lines are gone

Fifteen of them across the live examples: 13 Python and the two Rust twins in
`crater_self_shadow`, which had to go together or the pair stops matching
line for line. Nothing else changed — 15 deletions, 0 insertions.

**The four in `tests/` stay.** A test that needs vsync off should say so rather
than inherit it, or the day the default moves again it passes for a reason it
does not state. Examples are the opposite case: they are read as a model of
what a script needs, and a line that does nothing teaches that it is needed.

`examples/old/` had none to begin with.

## 15 September — the viewport, tuned against Blender

`notes/2026-09-15_viewport_tuning_and_two_wrong_spaces.md`. A long round of
visual tuning, settled in the end by rendering frames and measuring pixels
rather than by adjusting what was complained about. **Two of the four real
bugs were not what the complaint described.**

**Dimming the grid did nothing for three rounds because the arithmetic was in
the wrong colour space.** The surface is sRGB, so the shader writes linear and
the display encodes: linear 0.048 is 0.245 on screen, and a 2x linear cut is a
15 % perceived change. Caught by differencing `axes = "blender"` against
`axes = "off"` — grey grid pixels peaked at 0.412 where the model capped at
0.140, and sRGB(0.140) = 0.416. Values are now chosen in perceived terms and
converted back; `CONFIG.md` carries the conversion. The **ratio** turned out to
matter more than the levels: 1.5x between thin and major is not a difference
the eye separates, so every tenth line looked like every other.

**The wireframe fade faded on angle because a facet was measured by its
smallest height.** `1/fwidth(bary.i)` is a triangle's height from vertex `i`;
taking `max` of the derivatives takes the smallest height, which is what
foreshortening collapses. Largest instead, and coverage across a sphere's disc
went from halving past r/R 0.6 to flat. The window was also far too wide —
3-12 px put a whole 5120-facet sphere inside the ramp at six units. Now 1-4 px,
and **off by default**, as a toggle.

Also: the grid's plane now follows the view (XY, YZ or XZ), so a side view has
a grid at all; scrolling works in orthographic, which it never did, since zoom
moved the eye along a direction a parallel projection ignores; orthographic is
**borrowed** for a plane view and restored on the way out; the grid stopped
being re-ruled by the orbiting body; `ambient_strength` is 0.

Three gizmo settings removed, one grid colour added. 92 Rust tests.

**And a process fix.** Five window-opening tests never set
`open_in_background`, so every suite run stole the keyboard from whoever was
working — recorded in the project's memory as a preference, and ignored all
session. Fixed in all five.

## 15 September — the config nested, the bindings generated, one panel

`notes/2026-09-15_nested_config.md`. Asked for as a panel reorganisation --
"same topics should be gathered" -- and done at the struct instead, so the
panel, the Python surface and the docs all follow from one shape.

`app.simulation.config` is a struct of fourteen groups now --
`config.grid.color`, `config.light.ambient`, `config.shadows.per_body` -- with
one rule: the group prefix is stripped and nothing else is renamed. `title`,
`fullscreen` and `vsync` moved to `app.config`, where a window property
belonged; the two configs themselves stay two, for the reason `9781ca9` gave.

**The bindings are generated now**, the last mirror of the config that was
still written by hand. The reason is a trap rather than tidiness: nesting with
`#[pyclass(get_all)]` hands Python a *copy* of a group, so
`config.grid.color = ...` would set nothing, silently. Each group has to be a
view through the shared `Rc`, which is a page of identical code fourteen times
-- so `tools/gen_bindings.py` writes it, and `src/py/app/config.rs` went from
1,426 lines to 375 of actual logic. Old flat names keep working for a release
through a generated shim with a `DeprecationWarning`.

**The right-hand panel is one panel**: topic headers, each holding the entity
beside its own settings -- the Sun's position and the Sun's colour together,
the HUD list and its font together, one Selection and one Export where there
were two of each. The panel generator emits one function per group and the
`:group:` marker is gone, because the struct's nesting is the grouping.

Three generators, three guard tests, all current. 92 Rust tests, 14 Python
files.

## 2026-09-17 — trackpad gestures, Blender's map

Two-finger swipe orbits, `shift` pans, `ctrl` zooms, pinch zooms 1:1; a wheel
still zooms. Wheel and trackpad are told apart the way Blender does it -- by
notches against pixels -- in `Controller::scroll`, three unit tests on the
routing. The pinch had never reached the camera in the editor -- egui consumed
it, the wheel being exempt and the pinch not -- and where it did, it was fed
in as notches, so a whole pinch moved the eye 11%. `controls.trackpad_orbit` is the way back to zoom-on-swipe,
which is also the switch for a Magic Mouse. And the turntable's orbital term
now reverses when the view is upside down, as Blender's does, so a drag to the
right turns the scene right either way up. And the camera is levelled whenever
the user takes hold of it -- orbit, WASD look, `T` -- because the WASD look
yawed about the camera's own up and anchor switches kept a stale `up`, and
both left the turntable tilted. Note: `2026-09-17_trackpad_gestures.md`.

## 2026-09-17 — PCF moved shadows; now it filters

Reported on the Didymos example at iteration 12907: Dimorphos's shadow, landing
at Didymos's terminator, detached from the night side at `shadows.pcf = 7` and
all but vanished at 16. A filter blurs an edge; it does not move it. The normal
offset was growing with the kernel radius, and a lift of `h` off a body of
radius `R` moves a grazing shadow edge by `sqrt(2 R h)` -- 45 m at pcf 0 and
512, 185 m at pcf 16. Measured by the darkness centroid: 103 px of shift at
512/16, 29 px at 8192/16. The offset is one texel again; the far taps are
handled by the receiver-plane term, whose ceiling is now a slope (tan 85°)
instead of one texel of depth. Centroid shift 6 px and 3 px, darkness integral
conserved to 0.3 %, false darkening at pcf 16 down fivefold. `pcf = 0` is
bit-identical. Note: `2026-09-17_pcf_erosion.md`; test: `tests/test_pcf_filters.py`.

## 2026-09-17 — a rate cap, to watch a mutual event

`sim.state.rate_limited` / `rate_limit` (a checkbox and a slider in the Run
header): a cap on iterations per second. The frame keeps its full rate, so
the camera stays live; the counter and both callbacks wait, the same contract
as pause, so no script steps an iteration twice. Asked for because the
didymos example at 600 it/s turns a mutual event into a blink.
`State::begin_frame` decides once per frame, `State::advance` moves the
counter. The next iteration is scheduled one period on from when it was
*due*, not from when it happened: measured from the last advance, a cap set
at the run's own ~350 it/s held every frame that came a hair early and the
run visibly slowed -- a beat between two nearly equal periods. Frames slower
than the cap bank nothing, so nothing bursts later.

## 2026-09-17 — Restart reaches a driven script

Restart did nothing for a script that drives its own loop: the request was
recorded inside the frame and taken between two editor frames, but a
`while app.running:` script never handed a frame back, since `running` and
`step()` only went false when the window closed. The hosted Rust path already
had the rule -- `superseded` -- and the Python path has it now: another
pending run ends the loop, the script returns, and the editor runs it again
from the top. Tested by a script that presses Restart from inside its own
loop. And Restart stays enabled after an edit: editing clears `script_ran`
so that Play means "run the new text", which greyed Restart out at the
moment it was wanted. It runs the text in the panel, saved or not.

## 2026-09-17 — meshes load flat by default

`load_mesh(..., flatten=True)` appeared 27 times in the live examples and
tests and `flatten=False` nowhere: a default nobody wanted, spelled out every
time. Flat is the default now, and the argument is the exception --
`smooth=True` keeps the file's shared vertices. The Rust `load_mesh` takes
`smooth: bool` the same way. `flatten=` is accepted for a release, inverted,
with a `DeprecationWarning`; `tests/test_load_mesh.py` checks all four
spellings.

## 2026-09-17 — closing over an edited script asks first

The editor's window closed over an unsaved edit without a word. Now
`CloseRequested` over a dirty buffer raises a modal -- Save and quit, Quit
without saving, Cancel -- and the answer comes back through the same request
plumbing as the buttons: a save that fails keeps the window. `app.close()`
from a script is not intercepted; that is the program ending.

## 2026-09-17 — `N` folds the panels

A halfway house between the full layout and focus mode: the three resizable
panels fold to their window edges at once, each keeping egui's thin handle
so one can be dragged or double-clicked back out on its own. The per-panel
fold already existed -- `show_collapsible`, drag past the minimum -- this is
the toggle over all three. `N`, Blender's sidebar key; not `Tab`, which egui
takes to focus the first text field. And as config: `app.config.panels_folded`,
set before `start()` to open the editor folded, written back from the
panels so a drag that brings one out clears it.

## 2026-09-17 — the Sun at its true distance

`sun.pos = p_sun` straight from SPICE rendered Didymos black, and the Hera
examples carried `p_sun / AU_KM * 500.0` to live with it. The light's view
had its eye at the Sun, so the matrix held a 1.5e8 km translation, which the
GPU applies in f32 -- about 9 km of precision there -- and a 2 km scene
collapsed into one quantum. The eye now sits two scene-reaches outside the
scene on the line from the Sun, which is all an orthographic light needs.
Tested twice over: the light matrix's translation is bounded by the scene
(Rust), and two spheres render the same with the Sun at 50 units and at
1 AU (`tests/test_far_sun.py`). The workaround can go.

## 2026-09-17 — a command-line script runs before the window

The editor opened at the default size, black, then ran the script and
resized to what it asked for. The window is created by the first frame from
`app.config` as it stands, and the script ran after that frame. It is queued
as the pending script at `editor_start` now, and `editor_tick` hands it over
before drawing anything -- `step()` reports the run over while a script is
pending -- so width, title and `open_in_background` set by the script are
what the window comes up with. Rust test:
`a_command_line_script_runs_before_the_window_exists`.

## 17 September — pulled the macOS work, and it was red here

Six days of macOS work pulled onto Windows: 34 commits, 103 files, the light
curve driver, Hapke's `theta_bar`, the nested config, the viewport tuning, the
trackpad gestures and the PCF fix. All notes read.

**`theta_bar` closed my own open item, and corrected my estimate of it by an
order of magnitude** — I had placed it at a few mmag by analogy with `w` and
`b0`; measured it is 23 mmag rms and a **15 % change in light curve
amplitude**, the quantity an axis ratio is fitted to. Roughness acts near the
limb and terminator, whose share of the disc changes as an elongated body
turns, so it is structurally a different kind of parameter from the ones I
reasoned from. Recorded in `2026-09-11_hapke_roughness.md`.

**Two tests were red on this machine and green on the other**, both the same
cause: `roughness_terms` returned **NaN** across the whole back-scattering
plane. `f(psi) = exp(-2 tan(psi/2))` must go to zero as `psi -> pi`; in f32 the
representable `pi/2` sits past the true pole, so `tan` returns -2.3e7, `exp`
overflows to `+inf`, and the shadowing denominator evaluates `inf - inf`.
`cos psi` clamps to -1 for every facet at the limb and terminator — exactly
where roughness does its work — so this was NaN over the part of the body the
parameter exists to model. Fixed, and pinned by a finiteness test rather than
by luck: reciprocity caught it only because two NaNs compare unequal.
`2026-09-17_hapke_nan_at_psi_pi.md`.

Also corrected four descriptions of the render/compute shadow difference that
`65f5794` invalidated — the `(1 + N)` offset scaling is gone, so filtering is
the only difference now. Second time those two paths have drifted in prose
while agreeing in code.

Local data paths re-applied after the pull: the root swap plus
`hera_plan_local.tm` -> `hera_plan.tm` and the same for `hera_ops`. **Six
Didymos/Dimorphos mesh paths do not resolve here** and were left alone — the
examples now want a different source model (`9309mm` against the `01165mm` on
disk) at different decimation levels, so substituting would silently run a
different shape.

## 2026-09-18 — the shadow array sized to the scene

Out of memory on a 16 GB machine with 13 GB in use: measured, the two full
Didymos/Dimorphos models are 5.3 GB of RAM (76 bytes a vertex in the default
f32 build, three a facet, about as much again left behind by the OBJ parse,
plus the GPU copy and its staging) -- but the shadow array
was 2.1 GB before a mesh was loaded, eight layers at 8192 for every scene. It
is allocated at the body count now and grown as bodies arrive: 10k pair at
8192, footprint 2.39 → 0.87 GB. Fixed alongside, since the smaller array
would have made it a crash: `facet_shadow` read body *i* from layer *i* even
with the per-body fit off, where only layer 0 is drawn. Note:
`2026-09-18_memory_meshes_and_shadow_maps.md`.

## 2026-09-18 — two transients off the mesh load

Measured against the same 3M-facet pair: the OBJ was read whole into a
`String` and held for the length of the parse (parse peak 973 → 812 MB once
streamed), and the GPU copies were built whole and staged whole again by
`create_buffer_init` (first frame +2.36 → +1.85 GB for the pair, written in
slices of 2¹⁸ vertices now; the remainder is the buffers themselves). Peak
RSS for the full pair 5.35 → 4.74 GB. The 150 MB `flatten()` keeps for
`smoothen()` is documented and left for a decision.

## 2026-09-18 — flat is built as flat

The default load parsed to a shared mesh with tobj on one core (0.85 s of a
1.0 s load for a 3M-facet model), flattened it, and kept the shared copy for
a `smoothen` nobody called. `Mesh::load_flat` parses the plain `v`/`f` files
shape models are in parallel and builds the flat vertices and facets in
parallel from positions and triangles, bit-for-bit the old result, with
nothing kept: 1.01 → 0.20 s, ~1.5 → ~0.95 GB per mesh. Anything else in the
format falls back to tobj unchanged. The smooth path followed the same
day, in tobj's vertex order so the bits agree: 0.94 → 0.17 s. `is_flat()` had been inferred from the kept copy and is an explicit
flag now. Note: `2026-09-18_memory_meshes_and_shadow_maps.md`.

## 2026-09-18 — the flat build writes once; a cache tried and rejected

The flat build lost its pre-fill (`MaybeUninit`, 90 → 55 ms) and the upload
converts its slices on all cores. A sidecar `<file>.kmesh` cache took a 3M
load to 75–95 ms and was rejected the same hour: kalast does not leave files
beside a user's models. Cold load ~200 ms; the frame that uploads the mesh
260–320 ms of moving 830 MB of fat vertices. Under that means changing what a
mesh *is* -- see `2026-09-18_memory_meshes_and_shadow_maps.md`.

## 2026-09-18 — the vertex, emptied out

`tex`, `tangent`, `bitangent` and `extra` went first: 36 of a vertex's 76
bytes, declared by the shaders and read by none. Then normal, colour, mode
and value moved off the vertex into a storage buffer held **per facet** for a
flat mesh, since its three corners share all four by construction -- indexed
`vertex_index / 3` or `vertex_index`, the instance's flat flag choosing.

The 3M Didymos + Dimorphos pair: peak RSS 4.74 -> 2.03 GB, GPU 1.74 -> 0.52
GB, footprint 6.07 -> 3.09 GB, and the frame that uploads a mesh 261 -> 60 ms.
Verified against nine reference renders; the only thing that changed is a
flat facet with one corner coloured differently from its siblings, which now
takes the first corner's colour. `tests/test_mesh_attrs.py` guards it, and
fails when the flat/smooth selection is broken.

## 2026-09-18 — a mesh is its shared vertices, and flat is a flag

The third of the memory changes, and the one that removes a duplicate rather
than a waste: a flat mesh was a second copy of the geometry -- `flatten` built
one vertex per corner, renumbered the indices to `0..3f`, and kept the shared
vertices aside for `smoothen`. The corners are expanded into the GPU vertex
buffer now and nowhere else, so `flatten` and `smoothen` change `attrs` and
`normals` and nothing else, work on any mesh, and cost no memory.
`_vertices_before_flatten` and `_indices_before_flatten` -- "temporary until
better solution is found" -- are gone with the class of stale-index bugs they
caused.

Per 3M-facet mesh the CPU holds 186 MB where it held 480. The Didymos pair
across the day: peak RSS 4.74 -> 1.43 GB, GPU 1.74 -> 0.53 GB, footprint
6.07 -> 2.70 GB, the upload frame 261 -> 45 ms, the load 200 -> 142 ms.

For a caller: `positions` is the shared vertices, `colors` is per facet on a
flat mesh, `normals` is empty there. A shape-model fingerprint should hash
`positions[indices]`, which is invariant -- the four TPM scripts do now, and
their digests are unchanged, so saved spin-up states stay valid.

## 2026-09-18 — a tagged release builds, and publishes, itself

`.github/workflows/release.yml`, the repository's first workflow. A `v*` tag
produces **one executable** per platform -- linux-x86_64, macos-arm64,
macos-x86_64, windows-x86_64 -- archived with `res/`, `examples/`, `notes/`,
`shaders/`, `README.rst` and `pyproject.toml`, and publishes the crate to
crates.io and the package to PyPI.

**One executable means no Python in it.** Built with the `python` feature,
pyo3 links libpython by absolute path -- the binary here names
`/opt/homebrew/opt/python@3.14/.../Python` -- so it would start on the machine
that built it and nowhere else. Embedding an interpreter would not help
either: the examples import numpy, spiceypy and matplotlib, so it would have
to carry a site-packages too. So the executable is the engine and the editor,
which run anywhere with nothing installed, and `.py` scripts are what
`pip install kalast` is for -- published from the same tag.

`Cargo.toml` grew an `include` list. Without it `cargo package` packed the
whole tree, 477 files including `res/ico7.obj` at 12 MB, and crates.io refuses
anything over 10 MB; with it the package is 117 files and 2.5 MB -- the
shaders, the three `include_bytes!` resources and the two meshes the library's
tests load. The patterns needed leading slashes: a bare `README.rst` is a
gitignore-style glob and matched five of them inside `.venv/`.

**Two things the registries said that the repository did not.** `cargo
package` fails today: `kalast_macros = "0.1"` cannot resolve, because the only
`kalast_macros` on crates.io is 0.4.1. And `kalast` itself is published up to
0.4.1 while every manifest here says 0.1.0 -- so a `v0.1.0` tag would publish
*below* what installers already resolve to. `kalast` is free on PyPI. The
`version` job checks all of it before anything is built: the three manifests
against the tag, the `kalast_macros` requirement against the version being
published, both registries for a collision, and a warning when the tag is
lower than the registry's maximum.

## 18 September — the Windows wheel, and the shadow proxy priced

Pulled the v0.5.0 release work and picked up its handoff.
`notes/2026-09-18_HANDOFF_windows_wheel_and_shadow_proxy.md`.

**One of the three release failures is fixed.** `wheel windows-x86_64`
reproduced on this machine: `maturin build` needs a Python interpreter on
Windows to derive an import library, unless pyo3's `generate-import-lib` is
on. `maturin develop` always has one — the active venv — so the daily loop
never exercises it and the gap surfaced at a tag. Feature added, wheel builds.

crates.io is **not** a version mismatch — both manifests are 0.5.0, the
dependency is `version = "0.5"`, crates.io has 0.4.1 — so it is the missing
`CARGO_REGISTRY_TOKEN`, which is account setup. The Linux wheel is **not
diagnosed**: `gh` is unauthenticated here and editing CI on a hypothesis is
how one red job becomes three.

**The shadow proxy is worth 1.56x and is not free.** 1062 → 1656 it/s on the
Didymos pair rendered at 100k with a 10k `shadow_path`. But `API.md` claimed
it buys that "without touching per-facet science data", and it does: the
shadow map decides which fragments are lit and `facet_shadow` reads that same
map, so a body is depth-tested against a coarser version of itself. 2.90 % of
facets differ, 0.69 % flip by half or more, and the shadowed fraction goes
0.4651 → 0.4715 — a 1.4 % relative **bias**, which does not average out over a
rotation. In the image it is self-shadow acne on the limb, not a displaced
mutual shadow. Good for interactive work, bad for the TPM; `API.md` says so
now.

The fix that keeps both: a proxy for *other* bodies only, full mesh in each
body's own layer. The array is already per-body, so the layers exist. Next.

Nearly shipped as an unqualified win — the rate alone said 1.56x and "faster"
was the whole of what was asked. One exported frame each way changed the
recommendation.

## 18 September, later — the release run diagnosed, and two hypotheses wrong

`gh` authenticated here, logs read, all three red jobs fixed.
`notes/2026-09-18_release_run_diagnosed.md`.

**A fourth problem was hiding the other three.** `gh run view --log` returned
nothing and the Mac note guessed "still uploading". The run had never
*completed*: both `macos-x86_64` jobs sat queued from 17:28 against
`macos-13`, retired in December 2025. A job asking for a dead runner label
does not fail, it waits — so the run hung and GitHub served no logs at all.
`macos-15-intel` now, the last x86_64 macOS image. And the per-job REST
endpoint serves logs mid-run where `gh run view --log` refuses:
`gh api repos/O/R/actions/jobs/ID/logs --allow-escape-sequences`.

**crates.io was never the token.** Both handoffs said so; the secret was
created 17:22:55 and the run started 17:28:21. The real cause is
`type-features` instead of `features` on syn in `macros/Cargo.toml`, so `full`
was off and `syn::Item`, `syn::File` and `parse_file` were all configured out.
It compiled anyway in the workspace because another member enables them and
cargo unifies features — **a dependency declaration only has to be right when
the crate is built alone**, and the first thing that does that is `cargo
publish` verifying its tarball. A manifest typo invisible until the first
release.

**The Linux wheel was neither hypothesis either.** `--manylinux auto` went
through `args`, so the action never started a container and
`before-script-linux` ran on the host unprivileged, where apt cannot lock. The
script is dropped rather than fixed: x11-dl, wayland-sys and ash all dlopen,
so no `-devel` package is needed, and installing them would next have hit the
dead yum mirrors that were the other standing guess.

Three hypotheses across two handoffs, one right. All three were answerable by
reading a log, and the log was unreachable only because of a queued job nobody
looked at — because a queued job does not look like a failure.

Nothing published, so v0.5.0 can be reused. The tag points at `b4429b0` and
the fixes are after it, so it must move before a re-run.

## 18 September — v0.5.0 published

Run `35378532502` green end to end, 13 of 13 jobs. Tag moved to `2b4f5d5`,
the stuck run cancelled.

- **crates.io**: `kalast` 0.5.0 and `kalast_macros` 0.5.0
- **PyPI**: `kalast` 0.5.0, four wheels and an sdist
- **GitHub release**: executables for linux-x86_64, macos-arm64,
  macos-x86_64 and windows-x86_64

The Linux wheel's tag is `manylinux_2_17_x86_64.manylinux2014_x86_64` — glibc
2.17, installable back to RHEL 7. The broken configuration was building on the
host, which would have tagged against ubuntu-24.04's glibc 2.39 and excluded
most of the distributions people run. That the container fix was needed at all
is only visible in the tag.

`publish to PyPI` passed first time, so the trusted publisher was already
registered, answering the last open question from the Mac handoff.

**0.5.0 is burned on both registries now**; any further fix is 0.5.1. First
release of kalast above 0.4.1, and the first through this workflow.

## 2026-09-21 — the v0.5.0 bundle, actually run

Nothing in the pipeline runs the executable it ships, so the first person to
try it was the user, on the released macos-arm64 archive. The editor opens.
The two examples did not, each for its own reason, and both messages named
the wrong thing:

- **`./kalast examples/.../step.rs`** ended in *"failed to read
  `<bundle>/Cargo.toml`"*. Hosting a `.rs` generates a wrapper crate that
  depends on kalast **by path**, at the working directory, so it needs a clone
  of the repository and a cargo toolchain -- neither of which is in a bundle,
  and `Cargo.toml` is a file the user had no reason to expect. `write_wrapper`
  checks for the source tree first now and says that, pointing at `.py` as the
  thing that does work. `check_source_tree` is unit-tested against the repo
  and against an empty directory.
- **`./kalast examples/.../step.py`** printed Python's *"No module named
  kalast"*. The no-interpreter build hands a `.py` to `python -m kalast`, and
  it spawned and walked away, so the interpreter answered into a terminal
  after our own window had closed -- and answered the wrong question, since
  the interpreter existed and the package did not. It runs `import kalast`
  first now, and if that fails says `pip install kalast` and names the
  interpreter it tried.

**`shaders/` is out of the bundle.** Every shader is compiled into the binary
by `include_wgsl!`, so the copy in the archive could be edited all day and
change nothing, which is worse than not shipping it. It was in the original
request and I shipped it without saying that; the workflow now says why it
does not.

None of this is in v0.5.0 -- the fixes are for the next tag. **The gap that
produced all three is that the release workflow builds an executable nobody
ever runs**, on any platform. A smoke step is hard (the runners have no
display) but downloading the artefact and giving it a `.obj` is not.

## 2026-09-21 — the bundle stopped asking for a virtualenv

The answer to "so now rust and python example will work?" was no for Python,
not really: it needed `pip install kalast` first, and being told so clearly is
not the same as working. **Kalast is the executable**; `pip install` is a
second way in, like `cargo add`, not a step a release should require.

So the bundle carries its own interpreter -- python-build-standalone, pinned
by date, with this tag's own wheel and `tools/bundle-requirements.txt`
installed into it -- and `run_script` looks beside `current_exe` before it
looks at `PATH`. Nothing is installed on the user's machine and nothing is
written outside the folder they unpacked.

Measured, not guessed: `pip install kalast` is **766 MB**, the closure
`import kalast` actually reaches is **374 MB**, and the bundle is **380 MB**
unpacked, **130 MB** compressed. pyarrow and scipy are 174 MB of that and are
reached only because `kalast/__init__.py` eagerly imports `kalast.plot`;
deferring that would roughly halve the download and is the next thing to do.

Assembled and run here before committing: the executable picked the
interpreter beside it and rendered 60 frames. **And the workflow now imports
kalast from the bundle it just built** -- the step whose absence let all three
v0.5.0 faults ship. Details, including a segfault that turns out to be a
kalast clone shadowing the installed package, in
`2026-09-21_a_bundle_that_runs_python.md`.

## 2026-09-21 — and a `.rs` example runs from a release too

"Rust examples need to run too, that's the whole point of the bundle." Two
independent reasons they could not, both fixed: the generated wrapper
depended on kalast **by path**, so it needed a clone, and it now depends on
`= the exact version running` from crates.io; and a machine may have no
cargo, so the editor installs a minimal toolchain into `toolchain/` beside
the executable the first time a `.rs` is actually compiled.

A live bug fell out of the first one. `python` is a *default* feature and the
wrapper never turned defaults off, so a `--no-default-features` host -- which
is what a bundle is -- would have built a guest *with* pyo3 and refused it at
load. Invisible in the repository, where the host has `python` too.

Both measured: a bundle compiled `crater_self_shadow/step.rs` against
crates.io in 1m15s with guest and host fingerprints identical, and with
`env -i` and an empty `HOME` the bootstrap installed cargo 1.98.1 into
`toolchain/` (458 MB) and wrote nothing to `$HOME`. `release` now waits for
`crates`, since a bundle published before the crate it compiles against is a
bundle whose Rust half cannot work. Details in
`2026-09-21_a_bundle_that_compiles_rust.md`.

## 2026-09-21 — and the Rust examples ship already built

The last piece: someone who opens the example in the archive should not wait
five minutes or fetch a toolchain. `kalast --precompile a.rs b.rs` builds
and exits with no window, and the release workflow runs it **with the
executable it is about to ship**, so the libraries come out of the same
`write_wrapper`, feature set and `build_dir()` the editor looks in. A recipe
written in YAML would drift, and the way that shows up is the editor
silently recompiling everything the bundle shipped.

It checks `is_current` first, so it is idempotent -- which makes it the
verification too: run inside the assembled bundle it must report **0 built**,
and anything else fails the job. Locally: 2 up to date, 0 built, both
libraries accepted by the host (`af5af4cdd9cd31f3`), and
`./kalast examples/crater_self_shadow/step.rs` loaded the shipped library and
rendered to iteration 10000 with no cargo, no toolchain and no network.

32 MB added to the archive. "All the Rust examples" is two of seven --
`--precompile` over all seven gives *2 of 7 built*, the other five being the
`examples/old/` versions that do not compile against the current API.

## 2026-09-21 — v0.5.1, rehearsed before it was tagged

`workflow_dispatch` run 35597536132 on `9da3555`, green end to end: four
wheels, four executables, sdist, with the publish jobs correctly skipped
because they are tag-only. **That is what `workflow_dispatch` is in this
workflow for**, and it is the first time it has been used -- v0.5.0 was
tagged blind and half the run was red.

Everything new was exercised. From `executable macos-arm64`:

```
precompiled: 0 up to date, 2 built, 0 failed
Python 3.14.7
kalast from .../dist/kalast-dev-9da3555-macos-arm64/python/lib/python3.14/site-packages/kalast/__init__.py
bundle unpacked: 428 MB
precompiled: 2 up to date, 0 built, 0 failed
```

The last line is the one that matters: run inside the assembled bundle,
`is_current` accepts the shipped libraries. Windows agreed, which was the
real question -- `find -print0 | xargs -0`, `find -prune -exec rm -rf` and
the `*.dll` glob all went through Git Bash, and the DLLs came out with no
`lib` prefix on both sides because `dylib_path_for` and the copy derive it
the same way.

| | compressed | unpacked |
|---|---|---|
| macos-arm64 | 149 MB | 428 MB |
| macos-x86_64 | 157 MB | 446 MB |
| windows-x86_64 | 157 MB | 431 MB |
| **linux-x86_64** | **216 MB** | **630 MB** |

Linux is the outlier and nothing about it is kalast: a bigger standalone
CPython and fatter manylinux wheels for scipy and pyarrow. Which sharpens
the case for the deferred `kalast.plot` import -- scipy and pyarrow are
reached only through it, and they are the largest things in every one of
these.

**Two small things deliberately not folded in before the tag**, so that what
was tagged is exactly what was rehearsed: a `touch` on the copied libraries
(they are current today only because `cp -r examples` runs before the
library copy, and the in-bundle check is what would catch a reordering), and
making the import check assert the wheel's version instead of printing an
empty `kalast.__version__` that does not exist.

### v0.5.1 shipped, and the artefact was run

Run 35600392575: **every job green** -- the version gate, four wheels,
sdist, four executables, crates.io, PyPI, the GitHub release. The first
fully green release here; v0.5.0 took three runs across two machines and
still shipped a bundle that did not work.

Then the thing whose absence caused all of this: the published
`kalast-v0.5.1-macos-arm64.tar.gz` was downloaded and run.

```
kalast 0.5.1 from .../kalast-v0.5.1-macos-arm64/python/lib/python3.14/site-packages/kalast/__init__.py
precompiled: 2 up to date, 0 built, 0 failed
loaded target/kalast-hosted/plain/release/libcrater_self_shadow_step.dylib
PY OK: 60 frames, lit 98.1 %
```

A `.py` ran on the interpreter in the archive and a `.rs` loaded from the
library in the archive, on a machine where neither `pip install kalast` nor
cargo was involved. **CI checking the bundle it just built is not the same
as running the artefact a person downloads**, and after v0.5.0 that is not a
distinction worth being relaxed about.

Sizes as published: 149 MB macos-arm64, 157 macos-x86_64, 159
windows-x86_64, 217 linux-x86_64.

## 2026-09-21 — the editor stopped closing itself to run a script

Reported from the installed v0.5.1 bundle: `kalast step.py` opened the UI,
closed it, and reopened by spawning `python -m kalast`. The executable was
built `--no-default-features`, so it had no interpreter and could only hand
the script to one -- a decision whose reason (pyo3 links libpython by
absolute path) expired the moment the bundle started carrying its own
interpreter, earlier the same day. I improved the spawn's error messages
instead of asking whether it should still exist.

The bundled executable is now built **with** the `python` feature against
the interpreter in the bundle: `PYO3_PYTHON`, `-L native=<python>/lib`
(python-build-standalone reports `LIBDIR` as `/install/lib`, its build
container), an rpath relative to the executable, and `PYTHONHOME` set before
`Py_Initialize`. A `.py` now runs in the window already open.

Two consequences worth knowing. `abi_fingerprint` hashes the `python`
feature, so a rebuilt `.rs` links libpython too and
`build_hosted_blocking` configures that itself. And `PYTHONHOME` must be
stripped from the cargo child, or pyo3's build script runs a different
interpreter and dies -- which it did, and the rehearsal reported success
anyway because `is_current` is a timestamp check and the bundle had been
handed dylibs from 9 September. Details in
`2026-09-21_the_editor_stopped_respawning_itself.md`.

## 2026-09-21 — v0.5.2, and a CI bill that was self-inflicted

Run 35610973139 green in every job. Verified on the **published** archive,
which is the point: launching `./kalast examples/crater_self_shadow/step.py`
leaves the process it started running, prints no `-m kalast` line, and opens
one window. On v0.5.1 that process exited and a second one replaced it.

Two rehearsals were needed. The first (35604620915) failed on Windows with
`exit code 127` and no output at all -- Git Bash's way of saying a
**dependency** could not be loaded, not the file itself. `abi3-py314` makes
pyo3 link `python3.dll`, the stable-ABI forwarder, and only
`python314.dll` had been copied beside the executable; `vcruntime140.dll`
and `vcruntime140_1.dll` were needed too. Every DLL at `python/`'s root goes
beside it now, and the step lists the directory on failure so the next one
is a single log read.

**And the release took 28 minutes when v0.5.1 took 9.** Not the price of
publishing -- the publish jobs are about two minutes between them. The
executable is linked with `-L native=<path>/lib`, that path was
`dist/kalast-dev-<sha>-<target>/python/lib`, and cargo fingerprints every
crate on `RUSTFLAGS`: a new commit meant a new path meant all ~200
dependencies rebuilding, every run. `executable macos-x86_64` went from 4
minutes to 24. The interpreter is now unpacked to `runtime/python`, a path
with no version or commit in it, and moved into the bundle once the build is
done; `target/kalast-hosted` is cached as well, since the hosted wrapper
compiles kalast a second time into a target directory of its own.

Two things still worth doing. **Bump before rehearsing**, so the tagged
commit is the one that was rehearsed -- today's tag pointed at a commit
whose pipeline had never run, and while the delta was three version strings
that is the same shape of gap that let v0.5.0 out. And artefact reuse across
runs, which is only worth its complexity once the above is measured, because
it buys those last two minutes and nothing more.

## 2026-09-21 — a space in the bundle's path broke rebuilding a `.rs`

Reported from `~/Downloads/kalast-v0.5.2-macos-arm64 2`, the name macOS
gives a second download of the same archive. `RUSTFLAGS` is **split on
whitespace**, so `-L native=…/kalast-v0.5.2-macos-arm64 2/python/lib`
reached rustc as two arguments:

```
error: multiple input filenames provided (first two filenames are `-` and `2/python/lib`)
```

`CARGO_ENCODED_RUSTFLAGS` exists for this -- separated by `\x1f`, so a path
with spaces stays one argument -- and `bundled_python_rustflags` now returns
one element per rustc argument, unit-tested against a path containing a
space. Verified in a folder named exactly like the report: *0 up to date, 1
built, 0 failed*.

Only the *rebuild* path was affected; the prebuilt library loads fine. And
the workflow builds with plain `RUSTFLAGS` from `$PWD/runtime/python`, which
is why two rehearsals and a release never saw it: a GitHub workspace is
`/home/runner/work/…` or `D:\a\…` and never contains a space. **The runtime
path is the user's; the CI path is not.** Worth remembering the next time
something passes CI and fails on a download.

## 2026-09-21 — `import kalast` stopped loading the plotting stack

`kalast/plot` and `kalast/tpm` imported all their submodules eagerly, so a
script that only rendered a mesh loaded matplotlib, scipy and pyarrow --
229 of the bundle's 283 MB of site-packages. Both defer through PEP 562
now; `import kalast` reaches numpy and spiceypy and nothing else, every old
spelling still resolves, and `tests/test_lazy_imports.py` pins both halves
in fresh interpreters.

Deferring alone saves nothing -- it makes the packages optional, and the
bundle shrinks only if they stop being shipped. **pyarrow goes: 120 MB, and
only `kalast.plot.tool` wants it, which no shipped example calls.** scipy
and matplotlib stay, because three examples in the bundle plot and two of
them solve. Also corrected: `tpm/implicit` is not dead code, whatever its
docstring's historical note says -- `analytical/sinusoidal.py` calls it.

Stripping takes another 14 MB, verified to keep `kalast_abi` and
`kalast_example` exported and the libraries loadable, which the workflow
now checks on every build. Together about 134 MB off every archive.

Not done, and recorded in the note: one shared `libkalast.dylib` would save
54 MB more and is the wrong trade, because Rust's dylib ABI is unstable and
a user rebuilding a `.rs` uses their own rustc -- which is the whole reason
the host/guest boundary is a C ABI with a fingerprint.

## 2026-09-21 — one compile of the engine per bundle, and the wheel stopped linking libpython

A release bundle compiled kalast and its ~200 dependencies three times. The
wheel is a separate link and a separate CI job; the executable is the
product; the hosted-example build was waste. It now shares the executable's
target directory and finds everything built: **18 s and 3 crates, against
1m 21s and 30**. Four things had to be identical -- target dir
(`KALAST_HOSTED_TARGET_DIR`), the `-L` path (a stable stage name, renamed at
the end, which also fixes the cross-run cache), the rpath (both binaries get
both), and the **lockfile**, which the first attempt missed: the wrapper
resolved its own and drifted to `egui 0.36.2` against the root's `0.36.1`,
recompiling everything downstream. `write_wrapper` copies the root's now.

The collapse forced a feature split -- `python` / `embed` / `ext` -- because
the guest needs the host's exact link policy, and that closed a latent bug:
without `extension-module` the wheel linked libpython by absolute path, and
python-build-standalone's `python3` has its interpreter linked in
statically, so a locally built wheel segfaulted on import inside the
bundle. PyPI's worked only because the runner's Python happens to be one
pyo3 does not link against. `pyproject.toml` asks for `ext` now.

All measured locally on macOS arm64, no CI: bundle 291 MB unpacked, 113 MB
compressed, every check passing. Details in
`2026-09-21_one_compile_not_three.md`.

## 2026-09-21 — the bundle's interpreter, pruned to what the UI app uses

`python/` was 214 MB, and 49 of it nothing ran: a second CPython linked
statically into `bin/python3`, kalast's own extension module (the embedded
interpreter gets the executable's bindings through `inittab`), pip, tcl/tk,
headers. Gone; `python/` is 165 MB and the bundle **97 MB compressed**, from
149 at v0.5.2. `python/bin/python3 -m kalast` goes with it -- it opened the
same window. Plotting stays, for the three shipped examples that plot.

The coupling was that rebuilding a `.rs` handed pyo3 that binary to
introspect, and the bundle marker was that binary too. The marker is the
standard library now, and pyo3 reads `python/pyo3-config.txt` -- written by
the workflow while it still had an interpreter to ask -- with the two
build-machine paths rewritten to the user's. Proven locally against the
source tree; against crates.io it waits for 0.5.3, since the published
0.5.2 has no `embed` feature to ask for. Details appended to
`2026-09-21_one_compile_not_three.md`.

## 2026-09-21 — `./kalast some.obj` no longer opens on a black window

Reported: opening a mesh from the command line rendered nothing. `editor_start`
loaded it and did nothing else, so camera and Sun both sat at the origin --
inside the body. `Simulation::frame_all()` now stands the camera where
Blender's default view stands, backed off until the bounding sphere of every
body (through its `mat`, so where it *is*) fits the field of view, and puts
the Sun where Blender's default light stands, high and to the camera's
right; both look at the centre. An `.obj` on the command line gets that plus
the Blender axes and the wireframe, which give a bare shape its scale.
Measured through the readback: 48 % of `ico1.obj` lit, a sphere from one
side, where it had been 0. Exposed as `app.simulation.frame_all()`; stubs
and `API.md` updated; the command-line path itself is pinned by a headless
editor test.

## 2026-09-22 — v0.5.3 shipped, and a tag now publishes its rehearsal

**v0.5.3.** Bumped first, rehearsed (run 35630477424, green in every job on
the first try), tagged the same commit, released (run 35720751821, 10
minutes -- half the rehearsal's 20, because with stable paths the tag build
hit the cache the rehearsal had warmed). Archives 90 / 96 / 119 / 134 MB,
from 149 / 157 / 159 / 217 at v0.5.2. Then the downloaded macos-arm64
archive was run: embedded interpreter, prebuilt `.rs` loading, a `.py` in
the app's own process with no respawn -- and the one check no rehearsal
could reach, **a `.rs` edited in the bundle and rebuilt against crates.io**
through the shipped `pyo3-config.txt` with no interpreter binary present:
263 crates in 91 s, and the rebuilt library loaded. That was the command
that failed the day before for want of an `embed` feature on the registry.

**Artefact reuse.** The tag run rebuilt everything the rehearsal had just
built, on the same commit. It now asks the API for a successful
`workflow_dispatch` run with the same `head_sha` and all nine artefacts
unexpired; if there is one, `wheels`, `sdist` and `executable` are skipped
and `release` and `pypi` download from that run. Two things made that
possible: bundles are named from the manifest version on every path, so a
rehearsal's archive is byte for byte the release's down to the directory
inside it; and `github.sha` on an annotated-tag push is the commit, not the
tag object -- checked against the v0.5.3 runs. The step's shell was
exercised as written against the live API for the v0.5.3 commit, a dispatch
and an unrehearsed commit, and the name guards on both publish jobs against
the real v0.5.3 asset names. A tag nobody rehearsed builds as before.

**Not yet proven end to end**: a tag run actually skipping the builds and
publishing another run's artefacts. That is 0.5.4's dispatch-then-tag, and
the thing to look for is `wheels`/`executable` skipped and `release` logging
`reusing run <id>`. The Windows guest still recompiles 278 crates -- its
`-L` path from `current_exe` has backslashes and does not match the host's
-- and with reuse in place that is the long pole of a rehearsal, not of a
release.

## 2026-09-22 — a Rust example compiles from the buffer, unsaved, like a Python one

Reported: editing a `.py` in the UI app and hitting Restart applied the
change; editing a `.rs` needed Save before Compile took it. The asymmetry
was exact: the Python path hands `run_toplevel` the buffer, while
`write_wrapper` did `read_to_string(file)` and `is_current` compared the
library's mtime with the *file's*, so an unsaved edit was neither built nor
seen as stale.

`write_wrapper`, `build_hosted` and `is_current` take the source text now,
and the editor passes `editor.script`. Staleness follows the text: a
library keeps a fingerprint of what it was built from beside it
(`<lib>.source-hash`, SipHash with fixed keys so a runner and a user's
machine agree), and a buffer that differs is stale -- so Play on an edited
`.rs` rebuilds then loads, as Restart re-runs an edited `.py`. A library
with no fingerprint falls back to the old mtime rule. `--precompile` passes
the file's text and gained `--force`, which the workflow uses in place of
the `touch` hack; the fingerprint ships beside each prebuilt library so the
in-bundle check reads it. Pinned by three tests: the wrapper is built from
the buffer and not the file, staleness follows the text, and the hash is
stable. And run for real: first `--precompile` builds and writes the
sidecar, the second reads it back as current, an edited file is stale,
`--force` builds regardless.

## 2026-09-22 — res/README.md for the Hera data, and a reverted overreach

Asked for a README in `res/` on getting HERA.zip
(spiftp.esac.esa.int, 1.1 GB), fixing the examples' paths, and the
`PATH_VALUES` edit every meta-kernel needs -- SPICE resolves the pristine
`'..'` against the working directory, not the file, so the examples load
`*_local.tm` twins with the absolute `kernels/` path. The README covers
those three things.

Two facts from the zip worth keeping: the full-resolution OBJ shape models
are *in* it, under `kernels/dsk/` beside their DSKs, under the dataset's own
names; and the decimated `_10k`/`_100k` meshes are not, they are made locally
with `examples/mesh/decimate.py`.

What was **not** asked for and has been reverted the same day: rewriting
twenty example scripts to read `KALAST_HERA`/`KALAST_MESH`/`KALAST_TIRI`
environment variables, a `tools/hera_data.py` download script, and a
per-machine `local_paths.toml` (which the user deleted). The examples are
byte for byte as they were before; there is no per-machine file and the
user does not want one.

## 2026-09-22 — v0.5.4, the first tag that published its rehearsal

Bump, changelog section approved by the user, push, rehearse (run
35743729082, 19 min, green in every job), tag the same commit. The tag run
(35746071162) found the rehearsal by commit -- `reusing run 35743729082,
rehearsed on this exact commit` -- skipped `wheels`, `sdist` and all four
`executable` jobs, downloaded the nine artefacts from it, and published:
**3 minutes**, against 10 for v0.5.3 and 28 for v0.5.2. Both name guards
passed. The release body is `CHANGELOG.md`'s `## v0.5.4` section verbatim,
which the gate had checked for and the user had reviewed before the tag.

**Except that the release had no assets.** Every job green, the release
page carrying the changelog body -- and zero archives on it. The release job
had downloaded all four into `dist/` and the name guard had passed on them;
then `actions/checkout`, which I had placed *after* the download so the
changelog could be read, cleaned the workspace ("Deleting the contents of
/home/runner/work/kalast/kalast"), and the upload step found nothing, said
`Pattern 'dist/*.tar.gz' does not match any files`, and went green. Found
only because the post-release check listed the assets rather than trusting
the job status. Repaired by hand within the hour: the four archives
downloaded from the rehearsal run -- the bytes the job would have published
-- and uploaded to the release; the arm64 one then unpacked and run
(`embedded interpreter OK: kalast 0.5.4`, `2 up to date, 0 built`). The job
now checks out first and fails on an unmatched upload glob.

So the release procedure in rule 33 has been run once end to end as written,
and its first outing found the one step whose failure mode was silence.

## 2026-09-22 — the shadow pass draws shared vertices and culls: 41.6 → 64.2 it/s

`examples/didymos/main.py` on the full pair (2 × 3.1 M facets) was GPU-bound
at 41.6 it/s with the shadow pass at 19.65 ms of a 24.5 ms span: two 8192²
layers, every body into every layer, and each flat mesh drawn non-indexed --
9.4 M corners through a vertex stage that reads a position and a matrix.
Two changes, no shadow proxies, no change to any shadow a closed mesh casts
(`notes/2026-09-22_indexed_shadow_pass_and_caster_culling.md`):

- `MeshBuffer::shared_positions` (19 MB per 3M model) and `render_depth`:
  the shadow pass draws a flat mesh indexed over its 1.6 M shared vertices
  through the index buffer it already had. Shadow 19.65 → 10.96 ms,
  **54.6 it/s**. The facet-id and hemicube passes keep the non-indexed draw
  they need for `vertex_index / 3`.
- The shadow pipeline culls back faces when `shading.render_back_face` is
  `false`, the flag that already meant "closed geometry" for the main pass;
  `true` leaves both passes unculled as before. Shadow 10.96 → 8.71 ms,
  **64.2 it/s** (64.2 / 67.4 / 59.6), frame 15.6 ms wall.

The five shadow tests pass, `cargo test --release` passes with and without
the `python` feature (one unrelated flaky test in `app::cargo::buffer_tests`,
two tests sharing `wrapper_dir()` in parallel; passes alone).

Then the cheap third step: a body whose world AABB cannot rasterise under a
layer's matrix is not drawn into it (`aabb_may_hit_frustum`, conservative,
output-identical, unit-tested). Shadow 8.71 → 7.57 ms and the frame did not
move (62.5 it/s, within noise of 64.2): the shadow pass now overlaps the
render pass completely, so **the render pass is the frame** -- 12.8-13.8 ms,
non-indexed for the `vertex_index / 3` attribute lookup, a shader design
question. Layer resolution below 8192 is unmeasured; proxies stay ruled out
for this round. 100 it/s is 10 ms; the frame is 15.6.

## 2026-09-22 — `open_in_background` was only "not key"

A benchmark run took the user's mouse in the middle of their work, flag set.
Two halves, both in winit's macOS path: an inactive window is still ordered
*in front* (`orderFront`), so it covered the terminal and took the next
click; and on macOS 14+ `activateIgnoringOtherApps:NO` goes through
cooperative activation, which lets a process launched from the *active*
application -- the terminal being read -- activate anyway. Fixed together:
`WindowLevel::AlwaysOnBottom` on every platform, and on macOS
`ActivationPolicy::Accessory` for a background run (no Dock tile, no Cmd-Tab
entry while it is up). Frontmost application sampled every 0.5 s through a
6 s background run: the one that was in front, 11 of 11. CONFIG.md's entry
says so.

## 2026-09-22 — the main pass draws a flat mesh over its shared vertices: 93.4 it/s

The render pass was the frame (12.8-13.8 ms) and it was geometry: twelve
times fewer pixels took 3.4 ms off it. It shaded 18.9 M vertices a frame
because a flat mesh was drawn as expanded corners so the shader could find
the facet by `vertex_index / 3`. wgpu 30 has `PRIMITIVE_INDEX` as a WebGPU
feature, and this adapter has it: the fragment stage now reads a flat
mesh's attributes by `attrs[primitive_index]`, the vertex stage does the
position only, and the draw goes indexed over the shared vertices the
shadow pass already had -- except while the wireframe is on, whose
barycentrics need the corners (`INSTANCE_FLAG_CORNERS`). Render pass
13.1 → 8.2 ms, **93.4 it/s** (93.4 / 93.4 / 92.6), from 41.6 in the
morning. Seven tests, both cargo feature sets, and four looked-at frames
(flat, wireframe, smooth, a per-facet colormap ramp).
`notes/2026-09-22_indexed_main_pass_primitive_index.md`.

A device without the feature falls back to the corners draw with the facet
passed as a flat varying, exercised by forcing it for one build: identical
frames, 52.5 it/s.

Then two single-run variants: no text pass at all leaves the frame at
10.69 ms (the text figure was waiting behind the blit, not work); and
`shadows.resolution = 4096` gives **106 it/s** (2048: 110), because the
main pass's fragment stage waits for the 268 MB layers to be stored. The
physics query at 4096 gives the same counts as at 8192. **Open:** whether
4096 becomes the default -- the user's call, it changes the texel on their
outputs (10 → 21 cm on Didymos).

## 2026-09-22 — `shadows.resolution` defaults to 4096

Decided by the user on the measurement above: the layers are stored every
frame and the main pass waits for them, and at 4096 the Didymos pair crosses
100 it/s -- 105.3 / 106.6 / 97.3 over three runs, **2.5× the morning's
41.6** -- with the per-facet shadow query giving the same counts as at 8192. Texel 21 cm on Didymos, 4 cm on Dimorphos, with a
layer per body; 67 MB a layer instead of 268. No example pinned its own
resolution, so all of them take it; `test_pcf_filters` and
`test_shadow_layers` pin 1024 and `test_mesh_attrs` 2048, as before. A
script that wants the finer texel sets 8192 back. CONFIG.md's entry says so.

## 2026-09-22 — the display paced the loop

"Moving kalast to the second screen halves the it/s." A visible window was
presented every frame, and the window server hands drawables back at the
pace of the display the window is on: measured by the user on a light
scene, 300 it/s on the 120 Hz panel and **120 exactly** on a 60 Hz monitor,
both loop shapes. The swapchain is now acquired at most once per refresh of
the window's current display (winit's monitor refresh rate, re-read on move
and resize), and the frames between run as an occluded window's do. After:
~3,000 it/s on either screen. Every probe run from here used a background
window, which is covered and never presents, so none of them could see it;
a first change to `start()` measured against those was reverted.
`notes/2026-09-22_present_paced_by_the_display.md`, which also corrects the
2026-09-08 note's "pinned Rust binary" reading.

Reverted the same evening: on the adaptive-refresh built-in panel a present
every 8 ms let it fall to an idle refresh and every acquisition then blocked
130-220 ms -- 6 frames a second on screen at 100 it/s. Presenting every frame
again; the display still paces the loop (this example's own work is ~1 ms a
frame, the other ~9 ms is the acquisition). The `debug.window` line now
prints presents/s, frames/s, refused acquisitions, the longest gap and the
longest acquire. Next: acquire on a helper thread so the loop never waits.

Three more attempts the same night, all measured in the foreground and none
kept: a gate at twice the refresh rate (9 presents/s -- any gap between
presents makes the next `nextDrawable` cost ~100 ms on this panel; only
asking continuously is cheap); acquiring on a thread and presenting on the
loop's (deadlocks in wgpu-core, which holds `surface.presentation` across
the blocking acquire and takes it again to present); and acquiring, copying
and presenting on a thread (hangs in `queue.present` on the device's
exclusive snatch lock). `main` presents every frame as before; the loop stays
paced by the display for light scenes (about two iterations per refresh),
and a covered window runs free. The proper fix is the simulation on its own
thread with the winit thread presenting -- a plan, not an evening.
`notes/2026-09-22_present_paced_by_the_display.md`.

## 2026-09-23 — the screen no longer paces the loop: 2,850 it/s at 120 fps on screen

The presenter thread is dead on macOS with wgpu-hal 30 (its `acquire_texture`
hops to the main thread for the occlusion check), but the spike's *main-thread*
gate -- present only once per refresh interval -- ran free, and the same gate
in kalast had stalled for a reason that was never the display: with
presenting no longer throttling the loop, the CPU ran ahead of the GPU to
Metal's 64 in-flight command buffers, and the presenting frame's drawable
copy queued behind them (submit-to-completion latency measured at 250-780 ms
on a 0.94 ms GPU frame). `Window::render` now waits for the frame before
last, keeping two in flight; the redraw handler acquires the swapchain once
per refresh interval of the window's current display. Visible window: light
scene 2,850 it/s with 118 presents/s (was 300 here, 120 on the 60 Hz
monitor); `_10k` Didymos ~100 it/s, SPICE-bound, 74 presents/s; covered
ceiling 3,060 unchanged. `step()` is at most two frames ahead of the GPU
now, which every benchmark note wanted. `notes/2026-09-23_presenter_thread.md`.

## 2026-09-23 — v0.5.5 shipped

Bump, section reviewed by the user, push, rehearsal (run 35864527228:
every job green, including the first Linux bundle with `rfd`/GTK and the
first Windows bundle whose exe is a windows-subsystem program), tag on the
same commit. The tag run (35866707936) reused the rehearsal, published to
crates.io and PyPI, and the release carries its four archives -- checked on
the release page, not trusted from the job: linux 138 MB, macos-arm64 90,
macos-x86_64 96, windows 120. The section is the day's list: the 2.5×
rendering, the 4096 default, the background window, the unpaced loop, the
file picker, the Windows console, the fps cap, the toolbar.

## 2026-09-23 — the UI app offers a newer release

Asked of GitHub on a thread when the UI app opens (`app.config.check_updates`,
default on; never when a script runs its own window), answered through a
channel the frame reads with `try_recv`: the log gets this version and its
date, the newer one and its date, and its notes; the toolbar gets an
**update** button. Installing runs on a thread too, by how this copy was
installed (`update::Kind`): a bundle downloads its archive into the bundle
folder, unpacks it with `tar` and swaps every entry in place, what it
replaced parked in `.previous` for the next start to delete (Windows cannot
delete a running exe, but can rename it); a pip install runs `pip install
--upgrade`; a checkout is told to pull. Then a **restart** button relaunches
the same command line; nothing restarts on its own. `kalast --update` from a
terminal; `KALAST_UPDATE_PRETEND` to try the path. Tried end to end on a
copy of the v0.5.5 bundle claiming to be 0.5.4. `src/app/update.rs`.

## 2026-09-23 — a hosted example's panic no longer takes the window (for v0.5.7)

Reported from Windows: opening a pre-compiled `.rs` example crashes the UI
app. Not reproducible here, so the two causes the code allows are closed
blind. A panic in the example's `main` unwound into the wrapper's
`extern "C" fn`, which aborts the process on every platform -- and on
Windows, with stderr going nowhere since v0.5.6, without a word. The
wrapper's `kalast_example` now runs `main` under `catch_unwind`, says the
panic into the host's log through a new `HostApi::log`, and returns `1`;
the host keeps the library loaded (its scene and callbacks point into it)
and logs that it stopped there. And `kalast.exe` links with a 16 MB stack
(`build.rs`, msvc only): a hosted `main` runs inside the host's frame and
drives another frame from inside it, two frames of egui and wgpu on one
stack, which on Windows' default 1 MB is the one crash that would be
Windows-only. Tried here with an example whose `main` panics: the window
stays, the log says why. After the v0.5.6 tag, so it ships in v0.5.7.

## 2026-09-23 — v0.5.6 shipped

Bump, rehearsal 35881224098 green in every job, tag on the same commit, the
tag run (35884182981) reusing it: crates.io, PyPI, and the release with its
four archives checked on the page (linux 141 MB, macos-arm64 93, macos-x86_64
99, windows 123). From this version on the UI app sees the next release
offered in its toolbar. New standing rule from the user the same day: when
a rehearsal is all green, push the tag -- the section is shown as the
rehearsal starts, no approval step after.

## 2026-09-23 — fullscreen on the second screen (for v0.5.7)

`F` on the external monitor went fullscreen and came straight back out. The
green-button heuristic reads `isZoomed` every frame, and in simple fullscreen
that compares the screen's frame with its visible frame: unequal on the
laptop (the notch's 32 points), equal on a monitor with no notch and no Dock,
so there it read as a press one frame after `F` and undid it. Not read while
fullscreen or on the way in or out now. Same commit: the focus-mode toolbar
takes its row's height instead of a fixed 30 points that looked like two rows.
Write-up: `2026-09-23_fullscreen_second_screen.md`. Open: the user confirms
`F` on the second screen; the window cannot be placed there from a script.

## 2026-09-23 — every panel folds, one at a time too (for v0.5.7)

Asked for after the toolbar-height fix: the toolbar folds with `N`, folds
with the mouse, and each panel has a key of its own. The docked toolbar is
`show_collapsible` and `resizable` now -- resizable only for the handle, since
a panel is the size of its content and one row of buttons does not stretch to
fill a drag -- so its lower edge drags up or double-clicks shut like the other
three, and egui's handle at the top edge brings it back. `toggle_panels`,
the config fold and the read-back cover all four, so `panels_folded` means all
four. The arrow keys fold the panel on their edge (`Editor::toggle_panel`);
they were free, and egui keeps them while a text field has the focus. And
from a script, one field per panel -- `toolbar_folded`, `log_folded`,
`script_folded`, `simulation_folded` -- live and written back like
`panels_folded`. Write-up: `2026-09-23_fold_any_panel.md`.

## 2026-09-23 — applying to the SignPath Foundation

The Windows bundle is unsigned, so SmartScreen warns at first launch and
every release starts with no reputation. Signing options were weighed: the
SignPath Foundation signs open-source releases for free through an HSM and
a GitHub Actions step, Azure Artifact Signing takes organisations in the EU
but individuals only in the US and Canada, and an EV certificate no longer
buys instant reputation. The user applied to SignPath with the releases page
as the download page. Their conditions want a *Code signing policy* on the
home page and the release pages -- attribution line, team roles, privacy
statement -- so the README has the section and every release body carries
the attribution line under its changelog section (the workflow appends it;
v0.5.6's body was edited by hand to match). The privacy statement discloses
the UI app's update check, which contacts GitHub's API on open; the verbatim
SignPath sentence alone would not have been true. Open: the signing step in
`release.yml` once the project is approved and the SignPath project set up.

## 2026-09-23 — v0.5.7 shipped

Bump 97c7f21, rehearsal 35896345968 green in every job, tag on the same
commit, the tag run (35898045779) reusing its artefacts: crates.io, PyPI, and
the release with its four archives checked on the page (linux 141 MB,
macos-arm64 93, macos-x86_64 99, windows 123). The first release whose body
ends with the SignPath attribution line under the changelog section. In it:
every panel folds (toolbar with `N`, arrows, four config fields), fullscreen
on a second screen, the focus-mode toolbar's height, the `.obj` open button,
and the Windows hosted-example panic fix.

## 2026-09-24 — the SignPath Foundation declined

Their reply: the Foundation program wants public-trust signals first --
stars, forks, contributors, articles, institutional backing -- and kalast
does not show enough of them yet; reapply later, or pay for a subscription.
The user will not pay. So the code signing policy is gone from the README,
the attribution line is gone from the release job and was removed from the
v0.5.6 and v0.5.7 release bodies, and rule 33 is back to "verbatim and
nothing else". The README's Getting started says instead that the bundles
are unsigned, that Windows warns once and how to get past it, and that
`pip install kalast` never sees the warning. The bundles stay unsigned; a
reapplication is possible once the project is more visible.

## 2026-09-24 — where a point lands in the image

Asked for: the pixel position of each body's centre and each selected
facet's centre, `(0, 0)` to the camera's resolution, exported from an
example. `Eye::project` is the inverse of the picking ray -- top-left
origin, `x` right, `y` down, pixel `(i, j)` covering `i..i+1` -- and
`Simulation::project`/`project_body`/`project_facet` compose it with the
body's matrix, at `Simulation::image_size`, which the window now writes
every frame since only it knows the size drawn. Python: `sim.project(point)`,
`sim.project_body(b)`, `sim.project_facet(b, f)`, `sim.image_size`.
`examples/hera_didymos/afc.py` writes them to
`out/hera_didymos/afc/screen.csv`. Checked against the rasteriser's facet-id
map (no half-pixel bias, mean offset under 0.05 px) and, on a scratch copy of
the AFC example, against a pinhole model from the SPICE vectors (1e-4 px).
Write-up: `2026-09-24_image_positions.md`. Open: no visibility flag -- a
facet turned away still gets a position; and a pinned `image.width`/`height`
set before the window opens is ignored, the render taking the window's size
(found here, not fixed).

## 2026-09-24 — the landmark tracking example, and what it turned up

`examples/landmark_tracking/main.py`, the user's adaptation of a
collaborator's script: 3000 random facets of a Dimorphos model tracked
through a sequence of camera and Sun positions, every frame exported, each
facet's position, pixel (`project_facet`, read after `step()`) and view
cosine written to `track.csv`. Inputs and outputs are on the OMA cloud share
its README links. Its markers are unlit through `color_modes = 1`: a facet
on mode 0 follows the global mode, and in the lit mode `mesh.colors` is the
diffuse albedo -- `light.color × colour × (ambient + cos i × shadow)`, used
linearly, measured 74/255 for 0.15 at cos i 0.457 (the PNG is sRGB-encoded).
Open, found here, not fixed: the `P` and `K` keys change the pause state as
the event arrives, before that pump's frame, so a driven loop's paused
branch can draw a frame it never set up -- the user saw iteration 1 skipped
after the UI app's hold at 1, which this script now clears with
`pause_at = None`; from the code, the Pause button, clicked in the UI pass
before `update()`, stops the counter counting the frame it ran, so resuming
with Play repeats it; and `shading.srgb_mode`'s doc says 0 decodes colours
before shading, which the lit path does not do. A first-class albedo was
offered, separate from the display colour.

## 2026-09-24 — v0.5.8 shipped

Bump 434298e, rehearsal 36002305192 green in every job, tag on the same
commit, the tag run (36004270473) reusing its artefacts: crates.io (kalast
and kalast_macros), PyPI (four wheels and the sdist), and the release with
its four archives checked on the page (linux 141 MB, macos-arm64 93,
macos-x86_64 99, windows 123), its body the changelog section alone. In it:
`sim.project`, `project_body`, `project_facet` and `image_size`, the AFC
example's CSV of image positions, and the landmark tracking example.

## 2026-09-24 — a bundle that double-clicks, and a Linux bundle for 22.04

Two reports from colleagues. On a Mac, once the download flag was cleared
(the README now says `xattr -cr` on the folder: all 3,629 files of an
unpacked browser download are quarantined, 192 of them native), the crater
example stopped on its first mesh: the Finder starts a program in the home
folder, and `res/` is found from the working directory, as are the
precompiled `.rs` libraries and the file picker's `examples/`.
`kalast::app::bundle_working_dir` decides and the binary's first step acts:
a bundle started with nothing to open from anywhere else moves into its own
folder and says so on the terminal; a file named on the command line keeps
its directory. Tried on the built binary in a fake bundle with
`--precompile`, no window: from elsewhere it moves, named a file it stays,
inside it says nothing. On Ubuntu, `GLIBC_2.39 not found`: v0.5.8's
executable and its hosted `.so` ask for 2.39, for `pidfd_spawnp` and
`pidfd_getpid`, which std's process spawning picks up on 24.04; the bundled
libpython asks for 2.17. The Linux executable job runs on `ubuntu-22.04`
now, under its own cache key, so the bundle needs 2.35 -- exactly, since
`hypot`/`hypotf` bind at 2.35 -- and 2.35 is not slower than 2.39: every
faster maths entry point glibc added is in it (`expf`/`logf`/`powf` 2.27,
`exp`/`log`/`pow` 2.29, `hypot` 2.35). Older costs in steps: below 2.35
`hypot` (the interface only), below 2.29 the f64 trio (the roughness model),
below 2.27 the f32 trio (every CPU photometry and TPM path, `Float` being
f32). The v0.5.8 wheel, manylinux2014 from `manylinux: auto`, had all of them
on the old compatibility versions, so the user moved it to 2.35 as well:
built on the 22.04 host with `container: off` and `--manylinux 2_35`, there
being no standard 2_35 image. Older systems now get the sdist and build from
source. Open: the rehearsal that shows both build on 22.04 and what their
binaries ask for. A manylinux_2_28 container would reach RHEL 8, 9 and Ubuntu
20.04, for the f64 trio and `hypot`, if that is ever wanted.

## 2026-09-24 — v0.5.9 shipped

Bump b3ed5ab, rehearsal 36026031454 green in every job (18 minutes, the
Linux build cold on ubuntu-22.04), tag on the same commit, the tag run
(36028380323) reusing its artefacts: crates.io (kalast and kalast_macros),
PyPI (four wheels, the Linux one `manylinux_2_35`, and the sdist), and the
release with its four archives checked on the page (linux 141 MB,
macos-arm64 93, macos-x86_64 99, windows 123), its body the changelog
section alone. Checked on the rehearsal's own artefacts before tagging: the
bundle's executable, its hosted `.so` and the wheel's `_rs.abi3.so` ask for
glibc 2.35 at most, with `expf`/`logf`/`powf` at 2.27, `exp`/`log`/`pow` at
2.29 and `hypot` at 2.35. The two `pidfd` symbols are left weak and
unversioned by a 2.35 link, so the loader requires nothing newer, and on a
2.39 system they still bind. In it: the double-clicked bundle, the Linux
bundle and wheel on glibc 2.35, and the README's Mac and Linux notes.

## 2026-09-25 — `print` reaches the log from a script's first line

Asked for: `examples/cube/color_map.py`'s `print` showed in the terminal and
not in the log panel. The capture -- stdout and stderr pointed at a pipe,
drained into the log and teed to the terminal -- was made when the window
opened, and a script named on the command line runs before that: measured
from inside one, its stdout was still the terminal (`S_ISFIFO` false). The
capture starts in `editor_start` now, and a thread empties the pipe instead
of the frame, since before the first frame nothing did and a pipe holds
64 KB: a script printing more blocked in `write` for good. That was already
true between frames. The bundle's embedded Python is line-buffered the way
`python -m kalast` makes it, since it now starts on a pipe, and the binary
hands the descriptors back at exit, `Drop` never running once a script holds
the app. Measured through both front doors: a top-level `print` goes through
the pipe, and 3,000 lines, 200 KB, reach the terminal with no hang, where
1,709 did before. A test runs the capture in a child process -- 4,000 lines,
no frame to drain them -- and, with the reader disabled, catches the hang.
Windows still has no capture.

Then, asked for after the update check's line landed between a script's
`[0. 1. 2. ...]` and its next print: two tabs. The pipe cannot tell a
script's `print` from the engine's `println!` -- both are bytes on
descriptor 1 -- but a level up they differ: `capture_output` replaces
`sys.stdout` and `sys.stderr` with `_ScriptStream`, which hands each write to
`script_write` in Rust, for the script tab and the terminal's copy, written
to the capture's saved stdout rather than descriptor 1 so it does not come
back through the pipe. The pipe feeds the kalast tab, and the update check
writes there too. `app.log` and the disconnected-callback notice stay in the
script tab. Checked through both front doors: the writer is in place at a
script's top level, and each line, a script's and the engine's, reaches the
terminal once; the child-process test now also writes the way a script does
and finds the line in the script tab, whole, and not in the kalast one. A
Rust example's `println!` still lands in the kalast tab. Found on the way,
not fixed: `app::cargo`'s `the_wrapper_calls_the_example_s_own_main` and
`the_wrapper_is_built_from_the_buffer_not_the_file` write the same
`target/kalast-hosted/.../src/lib.rs` in parallel and read each other's --
about one run in two when only those run, and the release job runs them.

## 2026-09-25 — the v0.5.10 rehearsal, and three tests that raced

The rehearsal (36144304551, on 09f4c08) failed on Linux in the engine tests,
on `stdio_tests` itself: the child wrote its script line to the terminal
while the reader was still teeing the 280 KB there, in chunks that end
mid-line, and one landed inside the script line (`left: 0`). Here the same
race showed as a `line ` cut in two, once in 17 runs. The child now writes
the script line once the reader is idle and the unterminated line last, so
nothing overlaps. Two more, found on the way:

- Since the capture moved into `editor_start`, the unit tests that call it
  redirected the test process's own stdout, written by the harness from its
  own thread. With output in a pipe, as in CI, the harness died on EPIPE, 2
  runs in 60; with the capture off there, 100 in 100 were clean. No capture
  in the unit-test build now (`cfg!(test)`), and one per process
  (`CAPTURING`): a second one saved the first one's pipe as the terminal.
- `app::cargo`'s two wrapper tests wrote the same `target/kalast-hosted`
  crate in parallel and read each other's, 4 runs in 6 when only they ran.
  They take turns through `WRAPPER_TESTS`: 0 in 20.

After: the full suite 50 in 50 in release with piped output, the capture
test 50 in 50 in debug and in release, the editor tests green. Not tagged:
v0.5.10 goes out once the changelog has more in it; the manifests and the
section already say 0.5.10.

## 2026-09-25 — a rolling beta

Asked for: bundles published between releases, each overwriting the last,
and the release then made of the last one. `gh workflow run release.yml -f
beta=true` rehearses as before and a `beta` job then puts the four bundles
on the pre-release v<version>-beta: created the first time, afterwards its
tag moved to the new commit through the refs API and the archives uploaded
with `--clobber`, their names being the version's. The body is the
version's changelog section as it stands, under a line naming the commit.
A pre-release, so the UI app's update check, which skips them, never offers
it, and nothing goes to PyPI or crates.io, where a version is final.
Tagging v<version> on the beta's commit reuses the beta's run -- `reuse`
matches any successful dispatch of that commit -- so the release is the
beta's bytes, and the release job then deletes the pre-release and its tag.
`v*-beta` tags are excluded from the tag trigger. The first beta, on
db05247, went out green.

Then asked for: the update check should suggest a newer beta by its date. A
beta and the release of its version both say v<version>, so the executable
now carries the commit it was built from and that commit's date
(`KALAST_COMMIT`, `KALAST_COMMIT_DATE`, the date asked of GitHub's API once,
in `reuse`, so all four platforms write it as GitHub does). A build that
knows them, and whose version has no release, is a beta: it is offered
`v<version>-beta` when that was built from another commit and published after
its own date. Once the version is released, from another commit, it is
offered the release; the last beta's bundle is the release's, same commit,
and is offered nothing. A newer version wins over both, and a build without
the two -- local, pip -- sees no betas. For this the beta is recreated each
time rather than edited, since a release keeps the date and commit it was
first published with, and the release job names its commit
(`target_commitish`) instead of the branch. Checked live with a binary built
as an older beta and as the beta itself.

## 2026-09-25 — times in the log

Asked for: a time on every line of both log tabs, and a first stamped line
in each. A line is stamped where it is caught, not by the frame that moves
it into the panel: the pipe's reader once per read, `script_write` when a
line is whole, `Log::push` for the rest -- so what a script prints before the
first frame keeps its own time. Local `hh:mm:ss.mmm` -- the milliseconds
asked for after -- from `localtime_r` on the seconds of one `SystemTime`
reading, or `GetLocalTime` (`src/app/clock.rs`): the standard library knows
no time zone, and no dependency was added for one. The panel draws the stamp dimmed
in the same galley as the line, so a copied line keeps it; the terminal's
copy is unchanged. `editor_start` opens the kalast tab on `kalast
v<version> started` and the script tab on `script log started`, stamped
like the rest -- the time only, as asked; a first version put the date on
those two -- before any script has run. A cleared tab starts empty.

## 2026-09-25 — an orthographic view that holds still

Reported: clicking a gizmo ball in `examples/didymos/main.py` gave a side
view that moved, where the perspective view had held still. The camera's
orthographic box was fitted to the scene every frame -- `side` from the
bounds' radius, `offset` onto the bounds' centre -- the Sun's fit, whose
shadow map has to cover the scene. With Dimorphos orbiting, the union box
moves and grows, and the view panned and zoomed with it: over an orbit of a
test pair in a 640×480 window, the primary's centre wandered 200 px and
`side` went 2.67 to 2.75. A camera now frames from where it stands: `side`
is the half-height its perspective view has at the anchor, `distance ×
tan(fovy/2)`, and the box sits on the view axis, so only near and far follow
the scene. The same run: `side` constant, the centre on one pixel.
Switching projection keeps the anchor plane in place, Blender's rule, and
the gizmo's snap still backs off four radii, which at the default 30° frames
the scene as before (1.07 R, against the fit's 1.05 R). The Sun's fit is
unchanged. `an_orthographic_camera_holds_still_while_the_bodies_move`,
`switching_projection_keeps_the_anchor_plane_in_place`.

## 2026-09-25 — the log: pauses, unread tabs, a panel that keeps its height

Asked for, three at once, and then the resume. The kalast tab logs `paused
after iteration N`, N being `drawn_iteration`, the toolbar's number, and
`resumed at iteration N`, N being `state.iteration`, the one run next: the
frame compares its `begin_frame` decision with the last frame's
(`App::was_paused`, `pause_line`, tested on its own), whoever made the
change -- `P`, the buttons, the pause mark, a script. "After" and "at" so
that 41 then 42 does not read as a skip; and not `drawn_iteration` for the
resume, which after a Restart is the old run's number until the first frame
of the new one. A step -- Step, `K`, the one iteration a Restart or an
opened script shows -- is a resume with the mark on the iteration it runs,
and logs only its pause. `was_paused` is `None` before the first frame,
since how a run starts is not a change.

A tab not shown gets a dot while lines have come in that it has not shown:
`Log` counts pushes since `mark_read`, which the panel calls on the tab it
draws, only when the log panel is drawn at all. Painted on the tab's corner
in the hyperlink colour rather than added to the label, so the row does not
shift. The two first lines are read already, so a dot is for news.

The docked log panel shrank on switching to a shorter tab, and a drag
taller than its text snapped back on release: egui stores a panel's size as
the rect its content used, and the log's `ScrollArea` shrank to its lines.
`auto_shrink([false, false])` makes it fill the panel. The floating panel in
focus mode was not affected, as it fills the rect kalast keeps for it.

## 2026-09-25 — `pause_after_iteration`, and the kalast tab first

Reported confusing: a script opened in the UI app showed `pause_at = 1` in
the state and the log said "paused after iteration 0". `pause_at` was the
counter to stop on, compared after the increment, so it read one ahead of
the iteration that had run. Renamed `pause_after_iteration` and compared
with the iteration just run: `0` holds after the first, as the log says.
Every writer moved by one -- Step, `K`, Restart's one shown iteration, a
loaded Rust example, the Run header's checkbox -- and `{nit}` reads it plus
one, the run's length, so a HUD shows what it did. In Python `pause_at` stays
for a release as a deprecated alias doing the conversion, like
`load_mesh(flatten=)`; `pause_at = 0`, which never fired, becomes `None`.
The Rust field is renamed outright. Updated `examples/landmark_tracking/`
`main.py`, the debug print in `examples/didymos/main.py`, and
`tests/test_editor_startup.py`.

And asked for: the kalast tab first and shown when the UI app opens,
`LogTab::Kalast` the default.

Found on the way: the deprecation warnings never showed. `PyErr::warn` was
given stacklevel 2, but a call into Rust has no Python frame of its own, so
2 is the caller's caller -- `<sys>`, line 0, for a script's top level -- and
the default filters show a `DeprecationWarning` only when it is attributed
to `__main__`. `load_mesh(flatten=)` and the old flat config names (the
shim `tools/gen_bindings.py` writes) had been silent since they were added;
all three use 1 now, which names the script's line, top level or inside a
function, and in the UI app too, whose scripts run as `__main__`.

## 2026-09-25 — Catppuccin Mocha for the panels

Asked for: Catppuccin Mocha as a theme, chosen from the Window config, and
touching the UI only. `app.config.theme`, a `UiTheme` -- `Default`,
`CatppuccinMocha` -- through the generators like `HudAnchor`: a combo box
under Window, `"default"` / `"catppuccin-mocha"` from Python.
`src/app/gui/theme.rs` writes Mocha over egui's dark visuals with the roles
Catppuccin's own egui port gives each colour (panels on base, fields on
crust, widgets on the surfaces, links in rosewater), written out here rather
than taken from `catppuccin-egui`, whose releases trail egui's. It goes in
both of egui's slots, dark and light, since egui follows the system between
them and a change of appearance would otherwise drop it; applied when the
field changes, not per frame. The scene is untouched by construction: it is
an untinted image in a frameless central panel, over a pass cleared to
black, and HUDs, labels and the gizmo are the renderer's.

## 2026-09-25 — a VS Code layout, remembered settings, a python console, and a pick through Dimorphos

**The pick.** A click on Dimorphos in `examples/didymos/main.py` selected
Didymos behind it. The full-resolution models are indexed, so the click takes
the CPU ray, and Möller–Trumbore refused every Dimorphos facet as parallel:
its `det` is twice the area times a cosine, 6e-8 for a 0.24 m facet in km,
and was compared with f32's epsilon, 1.2e-7. Didymos's 1.2 m facets passed.
Relative to the edges now; regression test with a 0.24 m facet at 1.8 km.
Reproduced first with the pair at the example's epoch, the eye behind
Dimorphos: the ray through its centre returned body 0, then body 1. Only
picking and the Python ray helpers use the test; the physics does not.

**Asked for, in several messages:** Mocha as the default theme and the other
called "dark" (egui's dark in both of egui's slots, so the system's
appearance does not swap it); the theme and fullscreen remembered, as app
settings and not the simulation's; the layout of the VS Code window in a
screenshot -- code in the middle, a tabbed side panel on the right, the log
below, rounded panels; a Python console in the bottom panel; and the
changelog headed `-beta` while betas go out.

- `src/app/settings.rs`: `theme` and `fullscreen` in `settings.toml` in the
  OS's configuration folder (or `KALAST_SETTINGS`), a two-key TOML written
  by hand. Loaded in `editor_start`, before a script, and saved only from the
  app -- the app tab's widgets, `F`, the green button -- so a script dressing
  a figure does not change the app for next time. `open_in_background` wins
  over a remembered fullscreen, so test windows stay out of the way. The
  earlier worry about a Space was wrong: kalast's fullscreen on macOS is the
  simple kind, and stays on its Space.
- The layout: the middle a card with `renderer` and `editor` tabs; the side
  panel a card with `app` (`group_app`, moved out of the simulation panel's
  Window header, whose image size stays there as "Image"), `simulation` and
  `files` -- the working directory, listed as folders open, each listing
  kept two seconds; the log a card below. The cards sit 6 px apart over the
  theme's `extreme_bg_color`, the toolbar straight on it. The scene image
  takes the card's rounding; the render itself is untouched. The left panel
  is gone: `←` does nothing, `app.config.script_folded` is a no-op kept for
  old scripts, and the four-slot arrays keep their shape with slot 2 empty
  (`DOCKED`). Play, Restart and a Rust load switch to the renderer; the scene
  is measured, and takes clicks, only while its tab is shown.
  Then: no idle separator line along the panels' edges
  (`show_separator_line(false)`) -- in the gap it read as a pale border
  between the cards -- only egui's highlight while an edge is hovered or
  dragged; and VS Code's grip, three dots in the middle of the side panel's
  and the log's drag gaps, in the weak text colour.
  Then, from the screenshot's details: the cards 4 points apart (2 each)
  with a 1 px outline barely lighter than the card; the renderer in no card
  at all, so `N` leaves the scene as the whole window; the renderer/editor
  choice moved to two buttons at the toolbar's start, so `↑` hides them --
  a `Cell` shared by the toolbar and the middle, written back after the
  frame; and egui's own edge line, 1 px against the card in the text colour,
  replaced by VS Code's sash -- 4 px in the theme's accent (mauve), down the
  middle of the gap, lit after a 300 ms rest or at once while dragged. egui
  reads that line's strokes from the style the panel is shown in, so the
  panels are shown with them at no width and their contents given the style
  back. Beside the scene, which has no margin, the gap's middle is half a
  margin into the panel, and the sash sits there. The three dots were then
  taken out again, asked for: they looked out of place; dragging never
  depended on them.
- `Cmd`-`Escape` quits, `Ctrl`-`Escape` off macOS: the way out of simple
  fullscreen, which takes the title bar and its close button with it. Taken
  before egui, which keeps every key while a text field has the focus, and
  sent down the close button's path (`request_close`), so an edited script
  still asks. `Controller` tracks `super` now.
- The console: the log's third tab. A line typed goes to a queue; the loop
  hands it out as `EditorTick::Console` between frames, and a script driving
  its own loop takes them in its `step()` wrapper (`serve_console`). Python's
  `code.InteractiveConsole` runs it among the running script's globals --
  recorded by `run_toplevel` and `make_runner` -- with `sys.stdout` pointed
  at the tab. Both front doors handle it: `python -m kalast` through
  `run_editor`, the bundle through `console_line`, its interpreter setup
  split out of `run_script` (`prepare_python`) so a line can come before any
  script. Checked through `console_push`: a value, a block, a traceback,
  `exit()` refused, a script's variables; the queue has a Rust test.
- The changelog's section is `## v0.5.10-beta`; the tag's gate and release
  notes now match `## v<tag>` exactly, so a tag is refused until the heading
  is renamed, and the beta's body takes either.

Later the same day, asked for: **save** moved into the toolbar beside the
renderer/editor buttons; the editor's open button, its path field with the
example hint, and the file dialog behind them (`rfd`, `examples_dir`) taken
out -- the files tab is how a script is opened now -- with a breadcrumb at
the editor's top naming the file; and a click in the tree no longer switches
the middle, which stays on the renderer or the editor. The "Log" label left
the bottom panel, which holds the logs and the console. And the python tab
grew the panel a point a frame to its maximum: its prompt's row started at
egui's 18-point interaction height while the field in it, a monospace line
and its 2-point margins, is 20 and a bit, so the centred field stuck out
past the panel and egui kept the taller size. The row is allocated at the
field's own height now, the tab laid out bottom up, prompt first;
`the_console_does_not_grow_its_panel` runs six frames of the tab in a card
and holds the height. Reproduced before the fix, 1.125 points a frame, and
narrowed by variants: a scroll area with a plain row held, the row with a
text field did not.

Then, asked for and reported, in a run of messages:

- **Object persistency between scripts.** Reproduced: a cube, `reset()`, a
  sphere -- the GPU drew facet ids up to 7 of the sphere's 1280. The window
  rebuilt body buffers only on a change of count or `meshes_dirty`, and
  `reset()` set neither, so a script loading as many bodies as the last was
  drawn with its meshes. `reset()` marks the meshes dirty and clears the
  selection now (ids up to 1200 after); `begin_script` drops the callbacks,
  which a Python script left armed for the next; and a script other than the
  last runs on `Simulation::renew` -- config refilled in its own cell, camera,
  Sun, clock, export as `Simulation::new` has them -- tracked by
  `Shared::last_script`. The same script again keeps its config, which the
  panel edits, as the Python `reset()` doc has always said. Tests:
  `a_reset_rebuilds_the_meshes_and_drops_the_selection`,
  `renew_is_a_new_apps_renderer`.
- **The console did nothing** while a script looped under `python -m
  kalast`: its proxy there, `_EditorApp`, did not serve the queue from
  `step()`, only `_ScriptApp` did. Both do; checked with a line queued before
  a driven script ran (`console_submit`, bound for tests), which found it in
  the script's namespace. And the prompt lost its box and its hint
  (`Frame::NONE`), a terminal's line.
- **`Cmd`-`Escape` did not quit.** Not reproducible from here, no key can be
  sent to the window; the check depended on winit's modifier events, so it
  also asks AppKit now (`NSEvent::modifierFlags`, `macos::command_down`).
- The editor fills the middle (no frame of its own, the field the size of
  the view), the open file is named in the toolbar beside save, and a file
  clicked over unsaved edits asks first -- save and open, open anyway,
  cancel -- the open waiting on the save and dropped if it fails.

- **Tab completion in the console**, asked for. The input line cannot see
  Python's namespace, so Tab queues the line (`console_ask_completion`)
  beside the queued lines, `EditorTick::Console` now just says "serve the
  console", and `kalast.editor.serve_console` runs lines and answers the Tab
  (`console_complete`, `console_offer`); the prompt applies the answer if
  the line has not changed since -- one match whole, several their common
  part and a listing -- cursor to the end, as `↑`/`↓` now do too. The field
  locks the focus and the Tab is consumed before it, so it neither moves
  the focus nor types a tab. `console_complete` is `rlcompleter` with two
  changes: `inspect.getattr_static` decides the `(`, so no getter runs --
  `m.` on a 3.1M-facet mesh would have copied its arrays -- and only a
  dotted name is looked into, so a Tab calls nothing. The console tests take
  turns (`CONSOLE_TESTS`), the state being one per process.
- `help(m)` "truncated": it was all there -- `m` was a `Body`, whose class
  had two getters and no docs. Documented now, pointing at `help(body.mesh)`.
- The editor lost the separator above its code.
- **The console froze on Didymos**, not on the cube: evaluating `app.simulation`
  or `m.mesh` printed their `repr`, which was the full `Debug` -- every
  position and facet, 16 MB of text at 100k facets, half a gigabyte at 3.1M --
  then split and laid out in the python tab. Both are one-line summaries now
  (`Simulation(1 bodies, 3145728 facets, iteration 0)`, `Mesh(... flat)`,
  0.1 ms on Didymos), and the tab keeps at most 4096 bytes of one line, with
  a count of the rest. Tab itself was measured innocent: the completer takes
  2 ms at most on numpy and spiceypy, the prompt ran twelve Tab frames
  headlessly, and a driven script took five Tab requests in its loop.
  `app.` now completes the app's attributes through the UI app's proxy.
- **`Cmd`-`Escape` taken out again.** Tested with the UI app in front and keys
  sent through System Events: `P` and `Cmd`-`K` reached the window, `Cmd`-`Q`
  quit through the app menu, and `Cmd`-`Escape` produced a modifier change and
  nothing else -- not even in an AppKit local event monitor, which sees key
  events before the window does. macOS keeps it system-wide. The shortcut,
  its AppKit Command check (`macos::command_down`), the monitor and the
  `NSEvent`/`block2` dependencies are gone; `request_close` stays, the close
  button's path. `Cmd`-`Q` is the way out of fullscreen.
- Opening a mesh from the files tab now starts from a fresh renderer too
  (`open_mesh` renews, drops the callbacks and records itself as the last
  run): a cube example's settings were still in force around the mesh.
- **`Cmd`-`Q` did nothing either**, with a script loaded: sent to a UI app
  running `two_spheres/main.py`, held or playing, it was lost, and worked
  only once `F` had been pressed; on an empty UI app it quit. It came from
  winit's default menu, whose `terminate:` the pumped loop loses. The menu is
  off (`with_default_menu(false)`) and the window takes `Cmd`-`Q` itself,
  before egui, through `request_close` -- so an edited script asks, which the
  menu never did. Held, playing, fullscreen, empty: all four quit.
- **`Cmd`-`Q` still did nothing for the user**, where it had quit in the test:
  the user had been typing. With a text field focused -- editor, console --
  egui turns text input on, winit hands every key to the system's text
  handling, and none came back as a key: reproduced with the console focused,
  no key event at all, `x` or `Cmd`-`Q`. An AppKit local monitor on key-down
  (`macos::watch_quit_keys`, `block2` back as a dependency) takes `Cmd`-`Q`
  ahead of all of it and the window quits by `request_close`. Focused and
  unfocused: both quit.
  And matched by the character, not the key: the monitor and the window both
  looked for key 12, which is Q on a QWERTY layout only; with the keyboard set
  to another layout `Cmd`-`Q` stayed dead for the user. `charactersIgnoringModifiers`
  in the monitor, `logical_key` in the window, so it follows the layout macOS
  is set to, as other apps do. `objc2-foundation` (`NSString`) for the former.

