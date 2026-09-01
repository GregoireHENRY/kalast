# The thermophysical model and radiance on the GPU

One column per facet, stepped in a compute shader, state resident between
steps. Headless: no window, so a spin-up is a plain script.

**Why**: profiling the numpy path at 10,000 facets and 34 nodes,
`step_conduction` is **93.5 %** of the cost -- 1.13 ms of 1.21 ms a step --
and it is a stencil over 340,000 independent nodes. The 10k mesh was never the
problem; it is that the same spin-up extrapolates to **479 hours at 3.1M
facets**, which is not a run anyone starts.

## Results

| | facets | CPU ms/step | GPU ms/step | speed-up | spin-up CPU | spin-up GPU |
|---|---|---|---|---|---|---|
| decimated | 10,000 | 1.339 | 0.117 | **11.4x** | 1.21 h | **6.6 min** |
| full-res | 3,145,728 | 532.3 | 23.2 | **23.0x** | **479.6 h** | **20.9 h** |

**The full-resolution shape model moves from 20 days to under a day.** State is
428 MB in f32 at 3.1M, so memory was never the constraint -- time was.

### f32 is not a problem

WGSL has no f64 and the numpy path is float64, so the two cannot agree exactly.
Measured over 1,000 steps of a full diurnal cycle, surface swinging ~50 K:

| | mean abs diff | p99 | max |
|---|---|---|---|
| surface T, 10k | 0.00002 K | 0.00007 | **0.00012 K** |
| surface T, 3.1M | 0.00002 K | 0.00005 | **0.00009 K** |
| whole column, 3.1M | 0.00001 K | | 0.00009 K |

That is **0.001x the 0.1 K Newton threshold**, and four orders of magnitude
below the +/-6.6 K a single flipped shadow sample is already worth at a
terminator facet. `examples/analytical/tpm_gpu_vs_cpu.py` runs the comparison.

## How

Two entry points in `shaders/tpm.wgsl`, dispatched back to back each step:

- `surface` -- one thread per facet, Newton on the radiative boundary. Each
  reads only its own three shallowest nodes, so there is no sharing and it
  writes in place, exactly as the CPU path does.
- `conduction` -- one thread per node, **out of place**. The stencil reads its
  neighbours, so updating in place would let a thread read a value another had
  already advanced. numpy gets this for free by evaluating the whole
  right-hand side before assigning; on the GPU it needs two buffers and a
  flip, which is why `GpuTpm` keeps a ping-pong pair and two bind groups.

The base node's zero-gradient condition copies the node above it *after* that
node has advanced, so the thread handling it recomputes rather than reading a
neighbour the same dispatch may not have written yet.

**Dispatch shape.** A dispatch is capped at 65,535 workgroups per dimension.
At 3.1M facets the conduction pass needs 1,671,168, so it goes out
two-dimensionally and the shader rebuilds the linear index from a stride
carried in the uniform. This is the first thing that breaks when moving from
the decimated mesh to the real one, and it fails as a validation error rather
than as a wrong answer.

There was nothing to build on: `kalast.gpu.compute` is dead code whose Python
shim imports `kalast._rs.gpu`, a module that does not exist, and the only
compute shader in the tree doubles an array. `hemicube.rs` was the working
template.

## Radiance, and mixing the backends freely

Radiance is a 1D table lookup: `BandRadiance` tabulates
`eps * integral(B(T,w) R(w) dw)` over 20,000 temperatures and interpolates.
`numpy.interp` binary searches; the table is a `linspace`, so on the GPU the
index is arithmetic and no search happens at all.

**The two stages are independent.** The model and the radiance conversion each
run on the CPU or the GPU as the caller chooses, and all four combinations
work and agree:

| TPM | radiance | how the temperatures cross |
|---|---|---|
| CPU | CPU | never leave numpy |
| GPU | CPU | one readback, `GpuTpm.surface()` |
| CPU | GPU | one upload, `set_temperatures` |
| GPU | GPU | **nothing moves** -- `bind_tpm` reads the column buffer in place |

The last row needs both built on one `GpuContext`; separately constructed, each
makes its own device and the buffers cannot be shared. `bind_tpm` refuses
across contexts rather than silently falling back. Since `GpuTpm` ping-pongs
between two state buffers, a bind group is made for each and `compute(front)`
selects -- nothing is allocated per call.

Agreement against CPU/CPU is **4.3e-06 relative** at worst, over all
combinations and all seven TIRI bands.

### Which to use

`examples/analytical/backends.py` measures all four. Total time, 7 bands:

| facets | CPU/CPU | CPU/GPU | GPU/CPU | **GPU/GPU** |
|---|---|---|---|---|
| 10,000 | 0.296 s | 0.297 s | 0.025 s | **0.024 s** |
| 100,000 | 3.233 s | 3.232 s | 0.124 s | **0.120 s** |
| 3,145,728 | 35.25 s | 35.15 s | 1.49 s | **1.35 s** |

**The TPM belongs on the GPU at every size** -- 10x at 10k, 27x above it.

**Radiance does not**, below a point. Taken alone, GPU against CPU:

| facets | speed-up |
|---|---|
| 10,000 | **0.4x** -- slower |
| 100,000 | 1.9x |
| 3,145,728 | 3.0x |

The crossover is near 50,000 facets. Below it, dispatch and readback latency
cost more than `numpy.interp` does on so few lookups, and the honest advice is
to leave radiance on the CPU. This is why the stages are separable rather than
fused.

One trap worth recording: GPU radiance first measured *slower than CPU even at
3.1M*, and it was the binding, not the GPU. Returning `(n_facets, n_bands)` as
`Vec<Vec<f32>>` allocates once per facet -- 3.1M allocations -- and dominated
the readback. Reshaping one flat array instead took it from 0.9x to 3.0x.

## What is next, and where the time now goes

### Insolation moved too, and it was the bigger win

The boundary source term -- `S (1-A) cos(i) lit / d^2`, the `F` in
`F - eps sigma T^4 + k dT/dz = 0` -- was the last thing holding the loop on the
CPU. Timed on the real path rather than a synthetic proxy, at 3.1M facets:

| | ms |
|---|---|
| sun - position | 15.5 |
| its norm | 21.7 |
| dot with facet normal | 19.0 |
| clamp | 1.2 |
| flux | 3.8 |
| **total** | **61.1** |

against the 8.7 ms the model itself costs -- **seven times the step it feeds**.
The `spkpos` call is not the problem: one scalar call, ~0.05 ms, independent of
facet count. It is the per-facet vector arithmetic, and 151 MB of float64
temporaries churned every step.

What makes it wasteful is that **facet centres and normals are static in the
body frame**. They never change, yet the CPU path re-streamed 75 MB of each to
combine them with a sun direction that is three floats. `set_geometry` uploads
them once; `step_sun` then computes the flux on the GPU and a step uploads
three floats.

| | ms/step at 3.1M | 3.24M-step spin-up |
|---|---|---|
| numpy flux + upload | 64.4 | 58.1 h |
| **GPU insolation** | **8.4** | **7.6 h** |

**7.6x**, and the whole path is now 479.6 h -> 7.6 h, a factor of **63**.
Surface temperatures after a step agree with the numpy flux to 1.5e-05 K.

`step` still takes a host flux array, so a caller who computes the boundary
some other way -- radiative heating, a different insolation model -- keeps that
option. `set_lit` supplies a shadow fraction when there is one; a spin-up
leaves it at 1.

## Where the time went before that

**The bottleneck had moved off the GPU.** Timing the step with a precomputed
flux array, against building that array in numpy:

| facets | GPU step | numpy flux build | flux share of a real step |
|---|---|---|---|
| 10,000 | 0.145 ms | 0.043 ms | 23 % |
| 3,145,728 | 8.718 ms | 12.915 ms | **60 %** |

That table used a synthetic `cos` as a stand-in for the flux and so
understated it; the real path is 61 ms, measured above. Both are now on the
GPU.

Two other things not done:

- **Heating is still CPU-side.** Phase 2 reads surface temperatures back every
  step to feed the sparse view-factor matvec, which costs a readback the
  spin-up does not pay. Phase 2 is dominated by view-factor rebuilds anyway
  (see `2026-08-31_view_factors/`), so the synodic table matters more there
  than this does.
- **No shadowing in the GPU path.** The spin-up does not need it; phase 2 does,
  and `facet_shadow` already runs on the GPU but reads back.
