# Handoff, 1 September 2026

Written to pick this up on another machine. What follows is state, not history;
the write-ups are in `2026-08-31_view_factors/`, `2026-09-01_gpu_tpm/` and
`2026-08-31_gis3d_tiri_product/`, and the standing task list is at the end of
`TIMELINE.md`.

## Where things stand

**Done and committed.** Self and mutual heating in the TPM; the hemicube far
plane fixed; the TPM, radiance and insolation all on the GPU; phase 1 re-run on
re-cut meshes for both bodies; the view-factor proxy.

**Phase 2 has not been run.** It is set up and probes correctly, at ~26 s/step,
which is **about 8 hours** for the 1,309-step segment. That is the thing to run
next, and it is why this is being moved to a faster machine.

## To resume

1. `maturin develop --release`. Debug is 2-15x slower and worthless for a run.
2. Point the examples at this machine's data — twelve scripts carry absolute
   paths. `CLAUDE.md` has the procedure; `local_paths.toml` records what the
   previous machine used.
3. **The shape models matter now.** The decimated Dimorphos meshes shipped
   before 1 September were cut with MeshLab defaults, which permit face
   flipping. Re-cut them with `examples/mesh/decimate.py`, or copy the re-cut
   files across. A phase-1 state is tied to its mesh by a sha256 fingerprint,
   and phase 2 refuses a mismatch, so a wrong mesh fails loudly rather than
   quietly.
4. Phase 1 must be re-run if the meshes differ from the ones the `_v2` states
   were built on. It is 15 minutes for both bodies on the GPU.

## Running phase 2

`examples/hera_didymos/tpm_phase2.py`, currently set to Didymos 100k with a
10k view-factor proxy, Dimorphos 10k, `HEATING = "mutual"`, 12 deg rebuild
cadence, 5 radiosity bounces. Restarts from `out/hera_didymos/*_v2`.

**Before starting an 8-hour run, decide about Didymos's heating.** Its entire
self-plus-mutual effect is +0.07 K in the mean and +2 K on the worst facet of
100,000, and computing it is most of the cost: its 100k rows over 110k of its
own columns cannot be proxied, because a hemicube origin on the real surface
would sit inside a coarse hull. Dropping it -- heating on Dimorphos only, where
the effect is +2.56 K area-weighted and +30 K peak -- would take the run from
~8 h to well under one. `examples/hera_didymos/heating_preflight.py` re-derives
that recommendation at any epoch in about a minute.

## Comparing machines

`examples/analytical/machine_benchmark.py` needs **no external data** -- no
kernels, no shape models -- so it runs on a fresh clone or a bare node. One
number per subsystem. On the laptop this was written on:

| subsystem | 10k | 100k | 3.1M |
|---|---|---|---|
| CPU TPM | 1.355 | 15.793 | — |
| **GPU TPM + insolation** | **0.083** | **0.294** | **8.793** |
| CPU insolation | 0.198 | 1.905 | 64.559 |
| GPU radiance x7 | 1.324 | 1.451 | 9.396 |
| CPU radiance x7 | 0.364 | 3.447 | 129.540 |

Milliseconds, but **not all per the same thing**: the model and insolation
rows are per timestep, while radiance is per *call* -- one conversion of every
facet across all seven bands, which is wanted per output frame rather than
every step. The GPU figures are memory-bandwidth bound, so on a discrete card
they should fall roughly in proportion to bandwidth; this machine has unified
memory at about 400 GB/s.

Two things the table already shows, worth keeping in mind when reading another
machine's: **GPU radiance loses below ~50,000 facets** (1.324 ms against 0.364)
because dispatch and readback cost more than `numpy.interp` does on so few
lookups, and **the CPU TPM is superlinear** between 10k and 100k (1.355 to
15.793, not 13.55) as the working set leaves cache.

## For a cluster — read before planning that work

The compute paths split in a way that decides what can run headless:

- **Headless already**: the TPM, insolation and radiance. `crate::gpu::Context`
  requests an adapter with `compatible_surface: None`, so a spin-up needs no
  display. This is the part that wants a cluster, and it is ready for one.
- **Needs a window**: everything that goes through the render loop — the
  hemicube view factors, the shadow map, `facet_shadow`, `facet_id`. These are
  driven by `winit` from `App::start`, which creates a real window and depends
  on redraw events. On a headless node there is nothing to create.

So **view factors are the blocker for cluster work**, not the model. The fix is
to give the render passes an offscreen path that does not go through `winit` --
they already draw into their own textures, and only the final blit and present
touch the swapchain, so the work is plumbing rather than rewriting. Until then
a node would need a virtual display, which is worth avoiding.

Also relevant: wgpu on Linux will want Vulkan, and the code asks for
`adapter.limits()` rather than the conservative defaults, because a 3.1M-facet
column set is a 0.43 GB buffer against a 256 MiB default cap.
