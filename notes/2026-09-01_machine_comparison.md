# Machine comparison: work laptop vs home desktop

Answers the "Comparing machines" section of `2026-09-01_HANDOFF.md`, run on the
home desktop after moving the work there. Both sets are `maturin develop
--release`.

## Hardware

| | work laptop | home desktop |
|---|---|---|
| CPU | Apple M-series | AMD Ryzen 7 9800X3D, 8 core |
| GPU | integrated, unified memory | NVIDIA RTX 5080, Vulkan |
| memory | unified, ~400 GB/s | 2x32 GB DDR5-5600 dual channel, ~89.6 GB/s |
| GPU memory | shared with CPU | 16 GB discrete, across PCIe |

The memory rows are the ones that explain everything below.

## `examples/analytical/machine_benchmark.py`

Milliseconds. Model and insolation rows are per timestep; radiance is per call.

| subsystem | facets | laptop | desktop | desktop is |
|---|---|---|---|---|
| GPU TPM + insolation | 10k | 0.083 | **0.044** | 1.9x faster |
| GPU TPM + insolation | 100k | 0.294 | **0.084** | 3.5x faster |
| GPU TPM + insolation | 3.1M | 8.793 | **4.663** | 1.9x faster |
| GPU radiance x7 | 10k | 1.324 | **0.170** | 7.8x faster |
| GPU radiance x7 | 100k | 1.451 | **1.237** | 1.2x faster |
| GPU radiance x7 | 3.1M | 9.396 | 27.385 | **2.9x slower** |
| CPU TPM | 10k | 1.355 | 2.993 | 2.2x slower |
| CPU TPM | 100k | 15.793 | 39.218 | 2.5x slower |
| CPU insolation | 10k | 0.198 | 0.190 | equal |
| CPU insolation | 100k | 1.905 | 3.350 | 1.8x slower |
| CPU insolation | 3.1M | 64.559 | 113.148 | 1.75x slower |
| CPU radiance x7 | 10k | 0.364 | 0.230 | 1.6x faster |
| CPU radiance x7 | 100k | 3.447 | 3.574 | equal |
| CPU radiance x7 | 3.1M | 129.540 | 191.130 | 1.5x slower |

**The GPU model path -- what a spin-up and phase 2 actually spend time in -- is
1.9x to 3.5x faster here.** That is the result that mattered for moving the
work, and it held up in practice: phase 2 measured 12.0 s/step against the
laptop's ~26.

Two rows contradict that, and both are real rather than noise.

### Why the CPU paths are *slower* on the faster CPU

A 9800X3D should not lose to a laptop on numpy. It does because these paths are
memory-bandwidth bound, and the laptop's unified memory has about 4.5x the
bandwidth of this machine's dual-channel DDR5 (400 vs 89.6 GB/s).

The size dependence is the proof. CPU insolation is *identical* at 10k (0.190
vs 0.198), where the working set fits in the X3D's large cache, and 1.75x
slower at 3.1M, where it cannot. A clock-speed or IPC difference would show a
constant ratio at every size; a bandwidth difference shows exactly this.

Worth knowing before optimising anything CPU-side here: on this machine the
numpy paths will not go faster without reducing traffic, not by working the
cores harder.

### Why GPU radiance inverts at 3.1M

It is 7.8x faster at 10k and 2.9x slower at 3.1M -- the crossover is the
readback, not the compute. `RadianceGpu::compute`
(`src/tpm/radiance.rs:329`) reads back `n_facets * n_bands * 4` bytes:

| facets | readback | desktop time |
|---|---|---|
| 10k | 0.28 MB | 0.170 ms |
| 100k | 2.8 MB | 1.237 ms |
| 3.1M | **88 MB** | 27.385 ms |

The top end works out to ~3.2 GB/s effective, which is a PCIe-mapped readback
with a blocking wait, not a 960 GB/s card doing arithmetic. On unified memory
there is no copy at all, so the laptop never pays this.

It still beats this machine's own CPU by 7x (27.4 against 191.1 ms), so the GPU
path remains the right one here. It just loses the *cross-machine* comparison.

**This matters for the cluster plan** in the handoff. Discrete-GPU nodes pay
this on every radiance call, and radiance is wanted per output frame. Options,
in rough order of appeal: keep radiance on-device and only read back what is
actually written to a product; batch several frames per readback; or overlap
the copy with the next frame's compute. None of these are needed on unified
memory, which is why the issue did not show up before.

## Phase 2

Measured over a 2-minute window at the configured settings (Didymos 100k with a
10k view-factor proxy, Dimorphos 10k, `HEATING = "mutual"`, 12 deg rebuild
cadence, 5 bounces):

| | laptop | desktop |
|---|---|---|
| per step | ~26 s | **12.0 s** |
| 1,309-step segment | ~9.5 h | **4.36 h** |

This changes the handoff's own advice. Dropping Didymos's heating was suggested
to get an 8-hour run under an hour; at 4.36 h that trade is much less
attractive, and the pre-flight below puts Didymos's self term at the keep
threshold rather than clearly below it. Run kept as configured.

## Heating pre-flight, re-derived on the re-cut meshes

`heating_preflight.py` at 2027-01-21 05:36 UTC. Upper bounds -- conduction
damps the real response by roughly 2x.

| body | term | dT mean | dT max | verdict |
|---|---|---|---|---|
| Didymos | self | 0.093 K | 3.19 K | keep, on the max criterion |
| Didymos | mutual | 0.092 K | 0.59 K | droppable |
| Dimorphos | self | 2.203 K | 40.72 K | keep |
| Dimorphos | mutual | 1.797 K | 8.63 K | keep |

Consistent with the laptop's numbers (+0.07 K mean, +2 K worst facet for
Didymos), slightly higher, which is expected on independently re-cut meshes.

## Setup notes for this machine

- Decimated meshes did not exist here and were re-cut from the 3.1M originals
  with `examples/mesh/decimate.py`, so they use the `preservenormal` recipe
  rather than the MeshLab defaults that permitted face flipping.
- Phase 1 was re-run for all three states, on the GPU:

  | state | mesh | steps | rate | wall |
  |---|---|---|---|---|
  | `didymos_tpm_3orbit_v2` | 100k | 3,243,534 | 0.06 ms/step | ~3 min |
  | `dimorphos_tpm_v2` | 10k | 644,850 | 0.04 ms/step | ~25 s |
  | `didymos_tpm_3orbit` | 10k | 3,243,534 | 0.03 ms/step | ~110 s |

  Didymos's area-weighted surface mean came out at 259.2 K on both the 10k and
  the 100k mesh, which is a useful cross-resolution check that the spin-up is
  not resolution-sensitive.

- **Phase 1's `OUT` does not match phase 2's `RESTART` names.** Phase 1 writes
  `{body}_tpm`; phase 2 restarts from `didymos_tpm_3orbit_v2` and
  `dimorphos_tpm_v2`. The `_v2` names exist only in phase 2's `RESTART` dict,
  so they have to be set by hand when re-running phase 1.
- **Didymos's phase 1 must use the 100k mesh**, not the 10k one `tpm.py`
  names. Phase 2 loads Didymos at 100k and hard-checks the restart state's
  facet count and mesh sha256 against it. `heating_preflight.py` separately
  wants 10k states under the non-`_v2` names, so all three are needed.
- Paths repointed by the single substitution `/Users/gregoireh/data` ->
  `C:/data`, recorded with the exceptions in `local_paths.toml`.
- Not present on this machine: the `hera/tiri/` tree (band response and the
  Mars-swingby image lists), so `tiri_fits.py`, `tiri_movie.py` and
  `analytical/backends.py` cannot run here. The TPM and phase 2 do not need it.
