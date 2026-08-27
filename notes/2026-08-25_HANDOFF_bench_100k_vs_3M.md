# Handoff: mesh-resolution render benchmark (100k vs 3.1M facets)

Context for whichever Claude session picks this up: this continues a kalast
(Rust + wgpu asteroid thermophysical/render simulator) performance session
done on gregoireh's work laptop. The repo (`/Users/gregoireh/projects/kalast`)
is pushed to `origin/main` up to commit `1ed2e18` — start with `git pull`.

## What we're comparing

`examples/hera_didymos/afc_eclip_didy_manual.py` renders Didymos + Dimorphos
(Hera mission) with a moving camera/spacecraft. Two mesh resolutions:

- **Decimated**: ~100k facets each
  - `/Users/gregoireh/data/mesh/didymos/g_01165mm_spc_obj_didy_0000n00000_v003_decimated_100k.obj`
  - `/Users/gregoireh/data/mesh/dimorphos/g_00243mm_spc_obj_dimo_0000n00000_v004_decimated_100k.obj`
- **Full-resolution**: ~3.1M facets each, ~170MB `.obj` files, ~9.4M vertices
  after `flatten()` (currently what the script has committed)
  - `/Users/gregoireh/data/spice/hera/kernels/dsk/g_01165mm_spc_obj_didy_0000n00000_v003.obj`
  - `/Users/gregoireh/data/spice/hera/kernels/dsk/g_00243mm_spc_obj_dimo_0000n00000_v004.obj`

Also needed: the spice meta-kernel
`/Users/gregoireh/data/spice/hera/kernels/mk/hera_plan_local.tm`
(and whatever kernels it furnishes).

**First step on the new machine**: confirm all of the above exist at those
exact absolute paths (the example scripts hardcode absolute paths per
existing repo convention — no relative-path fallback). If the data lives
somewhere else on this machine, either symlink it into place or edit the
paths in the script for this test.

## Why this comparison, and what to expect

Two fixes landed this session that make the full-res mesh usable at all and
change its performance characteristics:

1. `src/app/window.rs`: device now requests `adapter.limits()` instead of
   wgpu's conservative default (256 MiB max buffer) — without this, loading
   the full-res mesh panics (`Buffer size 717225984 is greater than the
   maximum buffer size`). See `2026-08-24_hera_didymos_mesh_limits.md`.
2. `src/app/gpu.rs` (`MeshBuffer`): vertex buffer split into a static
   `geometry_buffer` (pos/tex/normal/tangent/bitangent, uploaded once) and a
   dynamic `attrib_buffer` (color/color_mode/extra, only re-uploaded when
   `Mesh.colors_dirty` is set via the new `mesh.mark_colors_dirty()` — this
   script never touches colors, so it uploads geometry exactly once).
   Instance buffer (transform) also updates in place instead of reallocating
   every frame. Before this fix the whole ~23MB/body vertex buffer was
   reallocated and re-copied *every single frame* regardless of resolution.

Reference numbers measured on the work laptop (lower-spec GPU), for
comparison — **the actual goal here is your personal computer's numbers for
the same two configs**:

| Config | Rate (work laptop) |
|---|---|
| 100k facets/body, shadow pass on | ~119.5 it/s |
| 3.1M facets/body, shadow pass on | ~60 it/s |

The ~2x slowdown at full-res is expected, not a bug — confirmed by
disabling the shadow pass as a diagnostic (which re-renders the full scene a
second time from the light's POV): that alone recovered most of the gap
(~97 it/s with shadow off at full-res). The rest is just ~31x more vertices
to transform even in a single pass. No further fix pending; this benchmark
is purely to see how a better GPU handles the same workload.

## How to benchmark (don't run the real script directly)

Don't just run `afc_eclip_didy_manual.py` as-is for benchmarking — it exports
every frame to `out/frames` for real use, and the 1-month-shortened sweep +
rate-printing used for benchmarking would be a confusing permanent edit to
that file. Instead, make a throwaway instrumented copy per run, e.g. in
`/tmp` — this is exactly what was done all session, pattern below.

```python
# Build this by copying afc_eclip_didy_manual.py and applying:
#
# 1. Add near the top:
import time
#
# 2. In tick(), right after computing `et` and before the `if et > etf`
#    check (or anywhere per-iteration), add:
if sim.state.iteration == 0:
    global _t0
    _t0 = time.perf_counter()
elif sim.state.iteration % 20 == 0:
    rate = sim.state.iteration / (time.perf_counter() - _t0)
    print(f"BENCH it={sim.state.iteration} rate={rate:.2f}it/s")
#
# 3. Shorten the sweep so steady-state is reached quickly, e.g. change
#    etf = spice.str2et("2027-05-01 00:00:00 UTC")
#    to
#    etf = spice.str2et("2026-12-05 00:00:00 UTC")  # ~1 month
#
# 4. IMPORTANT: point exports at a scratch directory, never the real
#    out/frames (a currently-running real export could be sharing that
#    directory — see the CLAUDE.md rule below). Right after
#    `app = kalast.app.App()`, add:
app.config.export_dir = "/tmp/kalast_bench/frames"
```

Then run it (it's a real GUI window — expect one to actually open):

```sh
cd /Users/gregoireh/projects/kalast
source .venv/bin/activate   # or: uv venv && uv pip install pip && maturin develop --uv
maturin develop              # rebuild after any git pull, before benchmarking
mkdir -p /tmp/kalast_bench
cd /tmp/kalast_bench
python -u /path/to/your/instrumented_copy.py 2>&1 | tee bench.log
```

Let it run until the `BENCH` rate stabilizes (watch it converge — takes a few
hundred iterations), then Ctrl-C. Do this once per mesh resolution: run once
with the script as committed (full-res paths), then swap the two mesh paths
in your copy to the `_decimated_100k` versions and run again. Report both
steady-state rates.

Cleanup after: `rm -rf /tmp/kalast_bench` (that's a throwaway dir you made
this session, safe to delete).

## Gotchas from this session, worth knowing upfront

- **`samply` (the profiler) hung/stalled when run on a backgrounded GUI
  process** in this environment — near-zero CPU, zero frames exported, no
  clear error. Root cause not identified (possibly a macOS permission/
  window-focus issue with sampling a backgrounded window). The print-based
  `BENCH` instrumentation above was used instead and works reliably — don't
  burn time re-attempting `samply` unless you have a specific reason to
  think this machine's environment differs.
- **Killing a run mid-flight** (Ctrl-C or otherwise) leaves a handful of
  trailing export files corrupted/truncated (as many as the save-worker
  thread pool size, currently 2-8 depending on CPU count) — harmless, not a
  bug, just don't treat the last few files in a scratch export dir as valid.
- **`~/.claude/CLAUDE.md` exists on the work laptop** with a standing rule:
  never `rm -rf` anything that isn't a directory created fresh in the
  current session for a throwaway test, without asking first — this is what
  the `/tmp/kalast_bench` scratch-directory pattern above is designed
  around. That file is local to the work laptop only (not synced to the
  Claude account) — if you want the same rule enforced on this machine, it'd
  need to be created here separately; ask if you want that set up.
- The mesh paths in `afc_eclip_didy_manual.py` are hardcoded absolute paths,
  matching every other example script in this repo — not a per-machine
  config system, just how the repo currently works.

## When done

Report both steady-state `it/s` numbers (100k and 3.1M) back, ideally with
GPU/CPU model for context. No code changes are expected as an outcome of
this benchmark — it's purely informational, comparing hardware.
