# The thermophysical model on the GPU

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

## What is next, and where the time now goes

**The bottleneck has moved off the GPU.** Timing the step with a precomputed
flux array, against building that array in numpy:

| facets | GPU step | numpy flux build | flux share of a real step |
|---|---|---|---|
| 10,000 | 0.145 ms | 0.043 ms | 23 % |
| 3,145,728 | 8.718 ms | 12.915 ms | **60 %** |

At full resolution more than half of every step is now CPU-side insolation --
`spkpos`, a dot product per facet, and a 12.6 MB upload. Moving insolation into
the shader would take the 3.1M spin-up from 20.9 h to something near 7.8 h, and
is the obvious next piece. That is the "radiance on shaders" half of the
original plan.

Two other things not done:

- **Heating is still CPU-side.** Phase 2 reads surface temperatures back every
  step to feed the sparse view-factor matvec, which costs a readback the
  spin-up does not pay. Phase 2 is dominated by view-factor rebuilds anyway
  (see `2026-08-31_view_factors/`), so the synodic table matters more there
  than this does.
- **No shadowing in the GPU path.** The spin-up does not need it; phase 2 does,
  and `facet_shadow` already runs on the GPU but reads back.
