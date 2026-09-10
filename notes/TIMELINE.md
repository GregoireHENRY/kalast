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
