# Full-resolution Didymos/Dimorphos meshes and the wgpu buffer limit

Recorded while setting up `examples/hera_didymos/`, when loading both
full-resolution shape models at once panicked.

> **Resolved on 2026-08-25.** The device now requests `adapter.limits()`
> instead of `wgpu::Limits::default()`, whose conservative 256 MiB cap is
> what this hit (`src/app/window.rs`). Both full-resolution meshes load.
> Kept because the mesh paths below are still the ones the examples use, and
> because the same cap will resurface on any backend that really is limited
> to 256 MiB.

## The panic

Loading the two largest shape models for Didymos and Dimorphos at the same
time:

```
thread '<unnamed>' (21141316) panicked at wgpu-29.0.4/src/backend/wgpu_core.rs:1614:18:
wgpu error: Validation Error

Caused by:
  In Device::create_buffer
    Buffer size 717225984 is greater than the maximum buffer size (268435456)
```

717 MB is the unflattened Didymos vertex buffer alone, against a 268 MB cap.

## Meshes

Full resolution, ~3.1M facets each:

```
/Users/gregoireh/data/spice/hera/kernels/dsk/g_01165mm_spc_obj_didy_0000n00000_v003.obj
/Users/gregoireh/data/spice/hera/kernels/dsk/g_00243mm_spc_obj_dimo_0000n00000_v004.obj
```

Decimated with meshlab to 100k facets. No longer needed to make the examples
run, but they are the `shadow_path` proxies -- ~1.5x faster for 9 differing
pixels of 1,040,400, see `2026-08-26_shadow_mesh_comparison/`:

```
/Users/gregoireh/data/mesh/didymos/g_01165mm_spc_obj_didy_0000n00000_v003_decimated_100k.obj
/Users/gregoireh/data/mesh/dimorphos/g_00243mm_spc_obj_dimo_0000n00000_v004_decimated_100k.obj
```
