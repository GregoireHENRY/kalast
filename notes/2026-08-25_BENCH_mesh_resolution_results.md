# Results: mesh-resolution render benchmark (100k vs 3.1M facets)

Answers the benchmark requested in `2026-08-25_HANDOFF_bench_100k_vs_3M.md`, run on
gregoireh's personal machine (Windows 11), then re-run on the work laptop
(macOS, M1 Pro) to settle the open question below. Headline: the benchmark as
specified does **not** measure mesh throughput on *either* machine, because
the render loop was silently vsync-capped. Numbers below are the corrected
ones.

All runs are `maturin develop` **debug** builds, matching the handoff's
methodology (and therefore the work laptop's numbers). Absolute rates would
change substantially in release -- see "Debug build caveat" at the end.

## Hardware

- CPU: AMD Ryzen 7 9800X3D (8-core)
- GPU: NVIDIA GeForce RTX 5080, driver 32.0.16.1074, Vulkan backend
- RAM: 61.4 GB
- Display: 1920x1080 @ **239 Hz** -- this matters, see below
- Integrated AMD Radeon also present; wgpu correctly picks the 5080 via
  `PowerPreference::HighPerformance`

## The problem with the naive numbers

Running exactly as the handoff describes gave **100k -> 202.6 it/s** and
**3.1M -> 211.1 it/s**. Full-res faster than decimated is backwards, so this
was investigated rather than reported.

Cause: `src/app/window.rs` set `present_mode: caps.present_modes[0]`, and on
this adapter that list is `[Fifo, FifoRelaxed, Mailbox, Immediate]` -- so
**`Fifo`, i.e. vsync**, on a 239 Hz display. The 100k config was pinned to the
refresh rate; the full-res config was genuinely slower than the cap so it was
unaffected. The two "matching" numbers were a measurement artifact.

Confirmed directly: with `vsync = true` the loop reports **239.46 it/s**,
i.e. exactly the display refresh rate.

## Corrected numbers

| Config | As-shipped (Fifo/vsync) | Uncapped, export on | Uncapped, export off |
|---|---|---|---|
| 100k facets/body | 202.6 it/s *(capped)* | **591.8 it/s** | 1133.1 it/s |
| 3.1M facets/body | 211.1 it/s | **210.4 it/s** | 231.4 it/s |

Comparable-methodology answer (export on, shadow pass on):
**592 it/s at 100k, 210 it/s at 3.1M** -- about **5.0x** and **3.5x** the work
laptop's 119.5 / 60.

True mesh-resolution cost on this GPU: **2.8x** with export on, **4.9x** with
export off, for ~31x the vertices. Not the ~2x the handoff expected.

Full-res is the only config here that is actually GPU-bound; export costs it
just 9%, whereas it *halves* the 100k rate (1133 -> 592).

## The work laptop's reference numbers were also artifacts — CONFIRMED

**Settled 2026-08-26 by re-running on the work laptop.** The hypothesis above
was right, and the counter-evidence resolves in its favour too.

Work laptop hardware: Apple M1 Pro, built-in Liquid Retina XDR (ProMotion,
**120 Hz**). `debug_window = True` reports its surface present modes as
`[Fifo, Immediate]` — so the old `caps.present_modes[0]` did select `Fifo`,
exactly as diagnosed on the Windows machine.

Same 600-iteration methodology, `maturin develop` debug build, at `a7b0dc9`:

| Config | vsync on, export on | vsync off, export on | vsync off, export off |
|---|---|---|---|
| 100k facets/body | 28.8 it/s | 22.9 it/s | **333.4 it/s** |
| 3.1M facets/body | 27.7 it/s | 22.6 it/s | **55.2 it/s** |

Clean GPU measurement (export off, vsync off): **333.4 vs 55.2 = 6.0x** for
~31x the vertices. The original handoff reported 119.5 / 60 and concluded the
"~2x slowdown is expected, not a bug, nothing needs reworking". That was wrong
on the measurement, and therefore on the conclusion:

- **119.5 ≈ 120 Hz**, this panel's exact refresh rate.
- **60 = exactly half of 120**, the Fifo half-rate cliff.

True cost of full-res is ~6x here, ~4.9x on the RTX 5080 — consistent, and
both far from 2x.

The "counter-evidence" (shadow-off giving ~97 it/s, not a clean divisor of
120) does not rescue the old numbers: under Fifo, when frame time straddles
the vblank boundary some frames hit it and some miss, so a *cumulative
average* lands between the quantised rates. 97 sits between 60 and 120, which
is what a mixed-regime run looks like.

**Second, independent artifact in the old numbers.** 119.5 was measured with
export on, back when the async queue was unbounded — so it partly counted
queue growth rather than completed work, the same effect measured on the
Windows machine. With today's bounded queue the equivalent config is 22.9
it/s. Two separate measurement errors happened to stack in the same
direction.

Both root causes are now fixed: `vsync` (default `true`, set `False` for
timing) and `export_max_queued` (default `64`).

## Frame export: unbounded queue, measured

Chasing a 2880-exports-vs-1304-files gap in the first run turned up a real
problem. `FrameExporter::export_frame` never drops a frame and always
increments `next_index`; the written indices were contiguous `0..1303`. So
every frame *was* queued -- the PNG encode workers simply fell ~1576 frames
behind, and everything still queued was abandoned when the process was killed.

Each pending frame pins a mapped GPU buffer, so the backlog is pure RAM.
Measured at 100k, vsync off, sampling RSS once per 2 s:

| Mode | RSS after ~16 s | Frames on disk (500-iteration run) | Rate |
|---|---|---|---|
| async (`export_sync = False`) | **30 GB**, growing ~2 GB/s | 32 / 500 | 626 it/s |
| sync (`export_sync = True`) | **340 MB**, flat | 500 / 500 | 4.84 it/s |

At uncapped frame rates this OOMs a 61 GB machine in roughly 30 s of sustained
export.

The async path's headline rate is also largely fictional. Measured sustained
on-disk write rate while rendering: **~5.6 frames/s** across all 8 workers --
about 110x slower than the 626 it/s the loop reports. The sync mode's 4.84 it/s
is essentially the honest end-to-end export throughput of this debug build;
the two agree, which is what confirms the sync path is not itself pathological.

The 8 truncated trailing files the handoff mentions were reproduced exactly
(8 = the save-worker pool size). Sync mode produced zero corrupt files.

## New config options added as a result

Both default to preserving existing behaviour.

- **`vsync`** (default `true`) -- `false` selects `PresentMode::Immediate`,
  uncapping the render loop. Set it to `false` for any timing work.
- **`export_sync`** (default `false`) -- `true` blocks the render loop on each
  frame's copy, encode and write, bounding memory to a single frame buffer at
  a large throughput cost.
- **`export_max_queued`** (default `64`) -- the bounded async queue, added
  after the above. Blocks the render loop once more than N frames are
  outstanding. `0` restores the old unbounded behaviour.

See `CONFIG_options.md` for the full reference.

`export_max_queued` turned out to be a **pure win**, not a trade-off:

| | unbounded (`0`) | bounded (`64`) | `export_sync` |
|---|---|---|---|
| Reported rate | 626 it/s | 45 it/s | 4.84 it/s |
| Frames actually reaching disk | ~5.6/s | 45/s | 4.84/s |
| RSS after ~16 s | 30 GB, growing | 627 MB, flat | 340 MB, flat |

Bounding the queue made real throughput **8x higher** (5.6 -> 45 frames/s)
while cutting memory ~50x. Unbounded, the render thread starves the encoder
pool and thrashes 30 GB of RAM, so almost nothing reaches disk; the 626 it/s
it reported was queue growth, not work completed.

This also means the "uncapped, export on" column in the table above is no
longer reproducible with default settings -- it measured the unbounded path.
The mesh-resolution comparison itself is unaffected, since the export-off
column is the clean GPU measurement and the two resolutions were measured
identically.

## Unrelated findings from this session

Two things turned up while documenting the config; both are written up
properly elsewhere, noted here so the trail is not lost.

- **PCF shadow filtering was wrong.** `shadow_pcf > 0` averaged onto a
  variable pre-set to 1.0, over-brightening every filtered shadow -- 13x at
  `shadow_pcf = 1`. Fixed; before/after renders and measurements in
  `2026-08-25_pcf_shadow_comparison/`.
- **`render_back_face` was never wired up.** Every pipeline hardcoded
  `cull_mode: None`. Now connected to the main render pass; culled vs unculled
  differ in 5 of 1,040,400 pixels on the closed Didymos/Dimorphos meshes.

Also worth knowing: **the committed sweep outruns Hera's ephemeris.**
`afc_eclip_didy_manual.py` runs to `etf = 2027-05-01`, but `spkpos` for HERA
relative to DIDYMOS throws `SPKINSUFFDATA` before that epoch under
`hera_plan.tm`. A full uninterrupted run will fail near the end. Pre-existing,
not introduced here.

## Debug build caveat

Everything above is an unoptimized build. The ~200 ms/frame encode cost is
dominated by the per-pixel row-copy and BGRA swap loops in
`src/app/gpu.rs::save_job`, which are exactly the kind of code debug Rust
punishes hardest. A release build would move the export numbers a lot and the
render numbers somewhat. The cross-machine comparison is still valid because
both sides used `maturin develop`.

## Reproducing

The benchmark scripts were throwaway copies in a scratch directory, not
committed -- per the handoff's own instruction not to run
`examples/hera_didymos/afc_eclip_didy_manual.py` directly for benchmarking.
To redo it: copy that script, shorten `etf` (the committed script sweeps
6 months, 2026-11-05 -> 2027-05-01; ~1 month is plenty to reach steady state),
point `app.config.export_dir` at a scratch path, set `app.config.vsync = False`,
and print `sim.state.iteration / elapsed` every 20 iterations.

Read the rate at the iteration where the sweep ends. Past that point `tick`
returns early, the loop idles, and the cumulative average drifts upward --
those later numbers are meaningless.

## Local data layout (this machine)

Differs from the handoff's macOS paths:

- Meshes, flat, both resolutions: `C:\data\mesh\`
- Full-res `.obj` also at `C:\data\spice\hera\kernels\dsk\` (identical files)
- Meta-kernel: `C:\data\spice\hera\kernels\mk\hera_plan.tm` -- note the repo
  script furnishes `hera_plan_local.tm`, which does not exist here
- `hera_plan.tm` and `hera_ops.tm` had `PATH_VALUES` changed from `'..'` to
  `'C:/data/spice/hera/kernels'`. Forward slashes resolve fine on Windows and
  stay consistent with the `$KERNELS/ck/...` entries. The pristine originals
  survive as the byte-identical `*_v182_20260823_001.tm` twins.
- `hera_plan.tm` verified: 64 kernels, `spkpos`/`pxform` resolve.
  `hera_ops.tm` loads but has no ephemeris coverage at 2026-12-05.
