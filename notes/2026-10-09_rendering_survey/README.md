# Rendering survey for kalast: shadows, anti-aliasing, ray tracing, GI, 240 fps

Date: 2026-10-09. Web research plus a read of the wgpu 30.0.1 sources kalast builds against and a Metal probe on
this M1 Pro. No code was changed.

Conventions: citation numbers `[n]` are local to each section and resolve in that section's source list.
`[unverified]` = could not be confirmed from a source; `[extrapolation]`, `[derived]`, `[estimate]` = arithmetic
done for this survey; `[assessment]` = judgement; `[speculation]` = not supported by a source. For UE6 only
Epic's own statements are reported.

Contents: 0 Bottom line · L Local check of wgpu 30.0.1 · 1 Unreal Engine 5.x/UE6 · 2 Soft shadows ·
3 Anti-aliasing · 4 Ray tracing from wgpu · 5 GI and denoising, planetary practice · 6 Performance at 240 fps ·
A Verification notes.

---

## 0. Bottom line for kalast

### 0.1 Mode (b), "a few images, absolutely error-free": is a real-time ray tracer the right tool?

**Ray tracing, yes; "real-time", no.** What games call real-time ray tracing is 1–2 rays per pixel, ReSTIR
reuse, a denoiser and TAA/TSR or an ML upscaler (UE Lumen/MegaLights §1.3–1.4, Bevy Solari §4.1). All of that is
biased, history-dependent and not reproducible (§3, §5.3–5.4). The engines surveyed here keep a separate,
unbiased, progressive path tracer for reference images: UE's Path Tracer with jittered "spatial samples" and TAA off (§1.5),
Bevy Solari's non-realtime reference (§4.1), pbrt-v4. The right tool for mode (b) is that: a progressive GPU path
tracer on the same hardware ray-tracing API, run to a stated error tolerance.

**It is reachable from kalast's current stack.** wgpu 30.0.1 exposes `EXPERIMENTAL_RAY_QUERY` on Vulkan, DX12
and Metal. This M1 Pro qualifies (macOS 26.4.1, `supportsRaytracing` = true), with traversal done by Apple's
driver in shader code: M1 has no RT hardware (§L, §4.1, §4.3). Caveats:
- The API is experimental, and Metal support is young (merged in v29, March 2026, with no CI coverage) (§4.1).
- There is no public RT-pipeline API, but inline ray queries in compute or fragment shaders are all this needs.
- On Windows, DX12 reports no ray queries unless DXC is found; Vulkan does not have this problem (§L, §4.1).

**Recipe, in the order the literature does it:**
1. **Geometry.** One full-resolution BLAS per body (3.1M triangles; the limit is 2^28), compacted, in a
   two-instance TLAS. Never use the LOD cut. Keep coordinates body-local and offset rays robustly (Wächter &
   Binder). DXR specifies a watertight intersection test; Metal's guarantee is unknown (§L, §4.4).
2. **Sun.** Next-event estimation (NEE) over the limb-darkened disc: importance-sample the radial CDF of I(μ)
   with stratified or Owen-scrambled Sobol' samples, and use the sin² form in f32 (§5.1).
   - Outside penumbrae one sample is exact. Inside, the error is ≤ 0.5/√N for independent samples and falls
     about as N^-0.75 for stratified ones (§5.1).
   - "Exact" means the limb-darkening-weighted visible area of the disc. The 1-D "vertical fraction above the
     horizon" used by one lunar tool errs by up to 0.077 (§2).
3. **Bounces, such as permanently shadowed regions (PSRs).** Single scattering dominates visible PSR light: in
   a bowl, ρf ≈ 0.02 per extra bounce (§5.2). Naive path tracing needs about 10^5 paths per pixel for 1% there.
   Instead:
   - do NEE to sunlit facets treated as emitters, or
   - final-gather over a per-facet radiosity solved with the TPM's own view factors. This is the planetary
     community's validated route (Topo3D, MoCSI, Potter et al., ShadowCam modelling by Mahanti et al.) (§5.5–5.6).
4. **Pixels.** Integrate each pixel's area with jittered subpixel samples, using a box filter (detector pixel)
   or the instrument PSF as SurRender does. About 570 samples per pixel hold a full-contrast limb to one 8-bit
   step (§3).
5. **Nothing temporal or learned.** No denoiser (HORUS shows a learned denoiser dropping and enlarging features
   (§5.4)), no ReSTIR reuse, no TAA/TSR/DLSS/FSR/MetalFX, no firefly clamp.
   - Fixed seeds and a plain average.
   - Stop on a per-pixel standard-error target.
6. **Validate** against analytic cases before trusting images:
   - the Ingersoll spherical bowl (others reach ≲1 K or 1–2%);
   - the sphere antumbra kalast already checks;
   - a cross-check of the shadow-map path (§5.6, §2).

**Cost** (extrapolated from Blender Open Data and vendor figures, §4.3–4.4; measure before relying on it):

| Workload | RTX 5080 | M1 Pro |
|---|---|---|
| 4K image, 1024 disc samples per pixel | 0.6–1.7 s | 15–45 s |
| Same with 3 bounces | 15–50 s | 6–28 min |
| TPM per-facet disc fraction, 2×3.1M facets × 256 rays, per step | 0.1–0.3 s | 3–8 s |

The RTX 5080 is about 19× the M1 Pro in Blender Cycles; an M3 Pro would be about 3.6× (§4.3). So mode (b) is a
progressive offline render: seconds to a minute on the 5080, minutes on the M1 Pro. That suits "a few images".

### 0.2 Where kalast's current shadow pipeline sits in the literature

| kalast piece | Closest published method | Documented limit |
|---|---|---|
| 32-direction "sheet" walk over shadow-map texels | Backprojection / micro-patch soft shadow mapping; horizon method in light space (§2) | Seen from one viewpoint (the Sun's centre) at finite texel size: gaps leak, sheets bridging depth jumps over-shadow |
| 32 equal-light bits per direction, merged by OR | Bitmask soft shadows (Schwarz & Stamminger 2007): the literature's fix for occluder fusion (§2) | Disc discretised into 1024 cells |
| `second_depth` | Depth peeling / multilayer shadow maps (§2) | 3 layers removed "most" leaks in Bavoil et al. 2008; no fixed count is guaranteed |
| Screen-space march toward the Sun | Bend Studio screen-space shadows; UE contact shadows (§2) | View-dependent; disocclusion. Must never feed the TPM |
| Depth-pyramid penumbra pre-check | AMD Hybrid Shadows tile classification; Boksansky et al. adaptive penumbra rays (§2) | — (this is where rays would go) |
| PCF taps on the receiver's tangent plane | Receiver-plane depth bias / adaptive depth bias (Dou et al. 2014) (§2) | A filter, not an integral over the disc |

UE's SMRT, PCSS and its variants (PCSS+, Filament's DPCF, AMD CHS), and VSM/EVSM/MSM are all less exact than
this (§1.1, §2):
- SMRT and PCSS use one averaged blocker depth or only the first depth layer.
- VSM, EVSM and MSM leak light.
- Epic itself calls ray-traced shadows the highest-quality option.

No shadow-map method can make mode (b) exact. kalast's interactive mode is already at or beyond the
state of the art for a disc light [assessment]. The remaining gaps are exactly the ones rays close.

### 0.3 Mode (a), 240+ fps with two 3.1M-facet bodies

- **Budget.** 4.17 ms per frame. Rasterising all full-resolution passes (main view, two body layers, near
  layer) is about 25M triangles per frame, roughly 1.6 ms on an RTX 5080 [speculation, from CuRast's measured
  RTX 5090 rates]. No figure exists for the M1 Pro; measure it (§6.1).
- **Patterns from shipped engines:**
  - **LOD to about 1 triangle per pixel or texel in every view, including shadow layers** (Nanite). This is
    for display only: Nanite's 2 px shadow LOD error would become a physics error in the TPM (§1.2, §6.1).
  - **GPU cluster culling.** Frustum, two-pass hierarchical-Z, and cone back-face culling against the Sun in
    shadow layers (about 25% of clusters on scan-like meshes, 50% per triangle). Compact surviving indices
    into one `draw_indexed_indirect`, because multi-draw is emulated on Metal (§6.2–6.3).
  - **Cache each Sun layer in the body frame.** Re-render it only when the Sun direction moves past a
    threshold tied to a texel or to the disc. Flax uses 0.8°, which is too coarse for a 0.53° Sun.
    - A paused simulation then costs nothing.
    - Didymos turns at 0.044°/s at 1× time, so time-lapse defeats caching. For a continuously moving sun,
      Epic stopped caching and lowered resolution while the light moves (§1.1, §6.4).
    - Keep self-shadow and mutual-shadow layers apart, as UE separates static and dynamic depth (§6.4).
    - Scroll the near layer, Insomniac-style, rather than redraw it (§6.4).
  - **Rays only where penumbrae are.** Use kalast's depth-pyramid tiles (the AMD Hybrid Shadows pattern). On
    the RTX 5080, 1–4 rays per pixel cost a few ms at 1440p [extrapolation], so penumbra rays could replace the
    walk, second depth and screen-space march there. On the M1 Pro, ray tracing is not a 240 fps tool (§4.4–4.5).
- **wgpu 30 limits to design around:**
  - one queue, so no async compute;
  - no 64-bit atomics on the M1 Pro, so no Nanite- or Bevy-style software rasteriser there;
  - mesh shaders only in WGSL on Vulkan;
  - multi-draw-indirect-count only on DX12 and Vulkan (§L, §6.3).
- **TPM isolation.** The TPM must never read display caches, LOD or screen-space results. With one queue,
  display shadow work competes with TPM GPU work, so skip display work whenever the script needs the GPU (§6.3,
  §6 takeaways).
- **Anti-aliasing at 240 fps.** Use MSAA 4× (the M1 Pro's maximum) plus SMAA 1x or FXAA, about 1 ms
  (WGSL versions exist in Bevy). Alternatively, use TAA that resets on any camera or simulation change. MSAA does
  nothing for shadow-map or penumbra aliasing (§3).

### 0.4 One-line answers per question

1. **UE 5.x/UE6.**
   - Virtual Shadow Maps, the Nanite cluster pipeline and SMRT port to wgpu, except the 64-bit-atomic software
     raster on this M1 Pro.
   - Lumen and MegaLights are biased, temporally denoised and built for many lights.
   - UE's only reference is its Path Tracer.
   - 5.8 (June 2026) is likely the last 5.x. UE6 early access is "end of 2027 (ish)", and no rendering
     technique for it has been announced (§1).
2. **Soft shadows.** Only rays (or exact horizons for heightfields) integrate the disc against all occluders.
   Every filtering method has a documented failure mode. kalast's walk is the bitmask/backprojection family
   (§2).
3. **Anti-aliasing.**
   - Interactive: MSAA plus SMAA/FXAA, or TAA that resets on change.
   - Export: a plain average of N jittered frames.
   - ML upscalers are vendor-locked and drift with model updates, so they are unfit for science (§3).
4. **Ray tracing from wgpu.**
   - Ray queries work in wgpu 30 on Vulkan, DX12 (with DXC) and Metal, including on this M1 Pro.
   - Apple's ray tracing is hardware on M3 and later, software on M1/M2.
   - A WGSL compute BVH (CWBVH or BVH4) is the fallback (§4).
5. **GI and denoising.**
   - Use NEE to the disc, with radiosity over the TPM's view factors or a final gather for bounces.
   - Denoisers (SVGF, NRD, OIDN, OptiX) are for previews only; NRD has no Metal path, OIDN runs on Metal and
     CUDA.
   - Validate against the Ingersoll bowl (§5).
6. **240 fps.**
   - LOD to about 1 triangle per pixel and texel.
   - GPU cluster culling with compacted indirect draws.
   - Sun layers cached in the body frame, with a Sun-motion threshold.
   - Penumbra-only expensive work.
   - Measure the M1 Pro's raster rate (§6).

---

## L. Local check: what wgpu 30.0.1 exposes, and this M1 Pro

Read from the crate sources kalast builds against (`wgpu`, `wgpu-hal`, `wgpu-types`, `naga` 30.0.1 in
`~/.cargo/registry`), plus a Metal capability probe run on this machine (Apple M1 Pro, macOS 26.4.1).
This is primary evidence, not a web claim.

| Feature (wgpu 30.0.1) | Vulkan | DX12 | Metal | This M1 Pro (Apple7, Mac2, Metal3) |
|---|---|---|---|---|
| `EXPERIMENTAL_RAY_QUERY` (BLAS/TLAS + inline ray queries in any stage) | yes (`VK_KHR_ray_query`, `acceleration_structure`, buffer device address) | yes when DXR tier 1.1 and SM 6.5, **and only if DXC is used**: the default `Dx12Compiler::Auto` takes static DXC (cargo feature `static-dxc`, off by default), else `dxcompiler.dll` on PATH, else FXC (SM 5.1, no ray queries) | yes when macOS 15+ and `supportsRaytracing && supportsRaytracingFromRender` | **exposed** (probe: both `true`), but M1 has no RT hardware: Apple's driver traverses in shader code |
| `EXPERIMENTAL_RAY_TRACING_PIPELINES` (raygen/hit/miss shaders) | flag settable by the Vulkan HAL, but **no public pipeline API** in `wgpu`/`wgpu-core` 30.0.1 | no | no | no |
| `EXPERIMENTAL_RAY_HIT_VERTEX_RETURN` | yes | no | no (naga's MSL writer: `unimplemented!`) | no |
| `ACCELERATION_STRUCTURE_BINDING_ARRAY` | yes | yes | no | no |
| BLAS compaction | `Queue::compact_blas` exists | | | |
| AS limits | guaranteed minimum: 2^28 primitives per BLAS, 2^24-1 TLAS instances; **Metal: 1 acceleration structure per shader stage** (shares buffer slots) | | | 3.1M triangles per body is far inside the limit |
| `SHADER_INT64_ATOMIC_MIN_MAX` (Nanite-style 64-bit visibility-buffer raster) | with `VK_KHR_shader_atomic_int64` | SM 6.6 | Apple9 (M3+), or Apple8 + Mac2 (M2) | **no** |
| `TEXTURE_INT64_ATOMIC` | with `VK_EXT_shader_image_atomic_int64` | SM 6.6 | Apple9, MSL 3.1 | **no** |
| `EXPERIMENTAL_MESH_SHADER` | yes, WGSL through naga | yes, passthrough shaders only | Apple7+/Metal3, passthrough MSL only | yes, but no WGSL |
| `MULTI_DRAW_INDIRECT_COUNT` | Vulkan 1.2 | yes | no; `multi_draw_indirect` is emulated as a loop of `draw_indirect` | no |
| `SUBGROUP` | yes | yes | yes (SIMD-scoped ops) | yes |
| `SHADER_F64` | yes | | no | no |

Notes:

- The doc comment on `EXPERIMENTAL_RAY_QUERY` in wgpu-types 30.0.1 still says "Supported platforms: Vulkan",
  while wgpu-hal sets the flag on DX12 and Metal too, and naga 30 has ray-query writers for SPIR-V, HLSL and
  MSL (MSL 2.4+). The documentation lags the code; the DX12 and Metal paths are younger and, by that
  measure, less exercised [assessment].
- kalast creates its instance with the default descriptor (`src/app/window.rs`), so on Windows the DX12 path
  would report no ray queries unless DXC is found; the Vulkan path does not have this problem.
- The feature is named EXPERIMENTAL and its doc says it "may have major bugs" and breaking changes
  (tracked in gfx-rs/wgpu issues #1040 and #6762).
- Consequence for an exact reference mode: the same WGSL ray-query code can run on the RTX 5080
  (Vulkan or DX12, RT cores) and on this M1 Pro (Metal, driver traversal, slower). No separate compute
  BVH is needed for correctness on either machine; a compute BVH would only be a fallback for
  adapters that lack the feature.
- Consequence for a Nanite-style path: the 64-bit atomic software rasterizer (Nanite, Bevy's virtual
  geometry) cannot run on this M1 Pro through wgpu 30, which leaves hardware raster of meshlets with
  indirect draws (emulated multi-draw on Metal) or mesh shaders written in MSL.
- Robust ray offsets matter for grazing Sun rays (kalast's own note of 2026-10-09: rays from 1 cm above
  the surface pass over a bump that rays from 1 mm do not). The standard references are Woop, Benthin &
  Wald, "Watertight Ray/Triangle Intersection", JCGT 2(1), 2013
  (https://jcgt.org/published/0002/01/05/paper.pdf), and Wächter & Binder, "A Fast and Robust Method for
  Avoiding Self-Intersection", Ray Tracing Gems ch. 6, 2019
  (https://research.nvidia.com/publication/2019-03_fast-and-robust-method-avoiding-self-intersection).
  On DX12 the fixed-function test is specified to be watertight: the DXR functional spec has a
  "Watertightness" subsection with a top-left rule for shared edges
  (https://microsoft.github.io/DirectX-Specs/d3d/Raytracing.html). The equivalent guarantee for Metal's
  intersector, software or hardware, was not checked [unverified].

---

## 1. Unreal Engine 5.x and UE6

Status as of Oct 2026: UE 5.8 shipped on 17 June 2026 at Unreal Fest Chicago and was presented as likely the
last 5.x release [18]. UE6 previews are announced for "late 2027ish" (§1.8).

### 1.1 Virtual Shadow Maps (VSM)

**Problem.** One shadow system that holds detail from centimetres to the horizon, with cost tied to the
shadow texels visible on screen rather than to scene size [1][2].

**Algorithm.**
- Each shadow map is 16k×16k virtual, split into 128×128 pages. Only pages that some screen pixel needs are
  allocated and rendered [1].
- Directional lights use clipmaps, levels 6 to 22. Each level is a full 16k map covering twice the radius of
  the previous one: about 64 cm at level 6, about 40 km at level 22 [1].
- Pages are marked from the depth buffer: each pixel is projected into light space [1][3].
- Nanite draws all directional lights in one pass. Clusters are culled against the mask of needed pages.
  Hardware-rasterised clusters translate virtual to physical page per pixel, using atomic writes [1][3].
- Shadow LOD is set so 1 shadow texel ≈ 1 screen pixel, with a default 2 px error bias. A short screen-space
  trace hides the mismatch between primary-view and shadow triangles. Shadow cost "scales with resolution,
  not scene complexity", times the number of lights per pixel [3].

**Caching.**
- Static and dynamic casters are kept in separate depth layers [1].
- A caster that moves invalidates the pages its bounds cover; world-position-offset materials invalidate
  every frame [1].
- "Any light movement or rotation will invalidate all cached pages for that light" [1].
- Fortnite Chapter 4 has a continuously moving sun. Epic found that even slow rotation reshuffles the page
  table every frame; a page can only be reprojected if every source page was mapped in the previous frame,
  which sparse VSMs rarely meet. Epic therefore stopped caching the sun
  (`r.Shadow.Virtual.Cache.ForceInvalidateDirectional`) and cut the sun's effective resolution by about half
  to hold 60 fps [2].
- Separate LOD biases apply while a light moves (`r.Shadow.Virtual.ResolutionLodBiasDirectionalMoving`),
  with a gradual return once it stops [1].
- "Coarse pages" give low-resolution coverage of the whole frustum for volumetrics [1][2]. "Distant" local
  lights collapse to a single 128² page with updates throttled to about one per frame [2]. One-pass
  projection cut the local-light loop from 1.56 ms to 1.08 ms [2].

**Changes by version.** 5.6: receiver masks, including with caching for directional lights [11]. 5.7:
directional receiver masks on by default (about 10 MB, "often fairly significant" gains in uncached, dynamic
scenes) [10]. 5.8: an invalidation budget (`r.Shadow.Virtual.DeferredInvalidationBudget`) and an
experimental "Prefiltered distant" mode: distant casters drawn at much lower resolution with prefiltering and
temporal reprojection, which "often matched ground truth better than SMRT" [9][25].

**SMRT (Shadow Map Ray Tracing).**
- Rays are shot toward the light, spread over Source Radius (local lights) or Source Angle (directional) [1].
  The directional default Source Angle is 0.5357°, the Sun's angular diameter (search snippet of the UE API
  page, not fetched) [17].
- Samples along each ray are projected into the VSM and depth-tested; no geometry is intersected [1].
- Defaults: 8 rays per pixel at Epic scalability, 4 to 8 samples per ray recommended
  (`r.Shadow.Virtual.SMRT.RayCount*`, `SamplesPerRay*`); 0 rays gives hard shadows [1].
- The penumbra is clamped (`MaxRayAngleFromLight`, `RayLengthScaleDirectional`) because rays "bend" away from
  the ideal test [1].
- The VSM stores only the first depth layer, so occluders hidden behind it are missed and leak light. A
  gap-filling heuristic extrapolates depth behind the first occluder, and Epic documents "inconsistent
  penumbra" artefacts [1]. With few rays the penumbra is noisy and relies on TAA/TSR to resolve.
- Versus PCSS: PCSS takes one average blocker depth then one variable-size filter; SMRT marches each ray
  through the depth map. Both are heuristics.

**Cost.** "Shadow projection is purely a function of the total number of shadow map samples … across the
screen", independent of page count or caching; non-Nanite casters are "much more expensive" to render into
VSMs [1]. The docs give no ms figures.

**Platforms.** PS5, Xbox Series, DX12/SM6, Vulkan with 64-bit atomics, Apple M2+ (beta) [1][16].

**wgpu 30 mapping.** Ports without vendor extensions: page marking is compute over the depth buffer, the page
table a storage buffer; shadow depth needs only 32-bit `atomicMax` (core WGSL on buffers; `TEXTURE_ATOMIC`
on Metal needs MSL 3.1); SMRT is plain texture loads. On Metal, `MULTI_DRAW_INDIRECT_COUNT` is absent and
multi-draw is emulated.

### 1.2 Nanite

**Problem.** Pixel-scale geometry at a cost that scales with screen resolution rather than scene complexity [3].

**Algorithm** [3]: 128-triangle clusters, grouped, simplified and split into a DAG. The LOD cut keeps
projected error under 1 px; each cluster is tested in parallel against its own and its parent's error, so the
DAG is never traversed. Hierarchical culling uses persistent threads (about 25% faster than naive). Two-pass
occlusion culling: test against last frame's HZB, draw, rebuild HZB, re-test the rejects. Clusters with edges
under 32 px are software-rasterised, on average 3× faster than Epic's best primitive-shader hardware path.
Depth test via 64-bit `InterlockedMax` into the visibility buffer (30-bit depth, 27-bit cluster, 7-bit
triangle); the hardware path also writes with 64-bit atomics. A deferred material pass follows.

**Numbers** (PS5, about 2496×1404 upsampled to 4K, 2021) [3]: over 1 billion source triangles reduce to about
25M rasterised; about 2.5 ms for culling and raster to a complete visibility buffer; about 2 ms for the
visibility-buffer-to-GBuffer material pass; about 5.6 bytes per Nanite triangle streamed (11.4 bytes per
input triangle).

**Recent additions.** 5.6: instance culling in chunks of 64 [11]. 5.7: minimum-LOD culling, HZB priming for
camera cuts; experimental Nanite Foliage and Voxels [10][19]. 5.8: tessellation toggle in VSMs,
skinned/assembly support [9]. Instances capped at 16M. **Mac:** M2+ (beta); not on M1 [16].

**wgpu 30 mapping.** The offline cluster DAG and culling compute with indirect draws are portable. The 64-bit
visibility-buffer software raster needs `SHADER_INT64_ATOMIC_MIN_MAX` or `TEXTURE_INT64_ATOMIC`: present on
Vulkan/DX12 (RTX 5080), **absent on this M1 Pro (Apple7)**, which leaves hardware raster with an R32/RG32Uint
target and depth test. An unfinished community port showed 32-bit-atomic workarounds on M1 [20].
Persistent-thread culling depends on inter-workgroup progress guarantees that GPU APIs generally do not
specify [21]; the portable alternative is one dispatch per level. Mesh shaders are experimental, WGSL only on
Vulkan.

### 1.3 Lumen GI

**Problem.** Dynamic multi-bounce diffuse light and reflections without baking.

**Algorithm.** Screen traces first; software ray tracing against each mesh's distance field for the first 2 m,
then a merged global distance field to 200 m by default (up to 800 m) [6]; the HWRT path adds a far field to
1 km [6]. Surface cache with up to 12 cards per mesh [6], update budget fixed at 512×512 texels per frame, 2
bounces per update plus further bounces through frame-to-frame feedback, for under 1/16 of Lumen's budget
[4]. Final gather: screen-space radiance cache at 1/16 resolution per axis with 8×8 octahedral probes and
product importance sampling; a world-space radiance cache for distant lighting; temporal filter and contact AO.
Design principle: "fixed update cost, variable lighting latency" [4].

**Budgets** [5]: 4 ms at 60 fps (High) and 8 ms at 30 fps (Epic) at 1080p internal on consoles, for GI and
reflections; each lower level costs about half. HWRT should stay under about 100k instances on consoles.

**Measured on PS5 at 1080p** [4]:

| Demo | SWRT final gather | HWRT final gather |
|---|---|---|
| Land of Nanite | 0.94 ms | 10.03 ms (overlapping meshes) |
| Lyra | 1.21 ms | 1.41 ms |
| Matrix Awakens | 1.83 ms | 1.72 ms |

In Matrix Awakens, reflections with HWRT plus hit lighting cost 11.54 ms against 2.44 ms with the surface
cache [4]. Recent: 5.6 deprecates SWRT detail traces, HWRT is the scaling path [11]; 5.7 half-resolution
integration saves about 0.5 ms on console at 1080p [10]; 5.8 Lumen Lite (beta) uses irradiance fields
[9][18]. **Artefacts:** leaks through walls thinner than 10 cm, lag when lighting changes [6]. **Mac:** SWRT
on M1+, HWRT on M2+ experimental [16].

**wgpu 30 mapping.** The HWRT path could use ray queries (exposed on all three backends); distance fields and
the surface cache are a large engineering effort. Lumen is biased and temporally filtered: it cannot be a
reference.

### 1.4 MegaLights

**Version history.** Experimental in 5.5, Beta in 5.7 [10][19], Production-Ready in 5.8 [9].

**Algorithm** [7]: each pixel picks N light samples by weighted reservoir sampling, guided by a list of visible
lights per 8×8 tile built from history plus a 20% budget for currently hidden lights; spatio-temporal blue
noise; sampling at half resolution. Each sample traces a short conservative screen-space ray, then hardware RT
(inline RT is the default on consoles, being faster there). Visible samples are shaded, then one denoiser pass
covers all lights. ReSTIR reuse was rejected (about 23× more traces than 1 spp, and correlated samples hurt
the denoiser); one trace per pixel gives about 0.8 spp. Per-light shadow options: ray tracing (default), VSM,
or screen space [8].

**Cost** [7]. PS5, 1080p, 1 spp, 941 shadowed area lights, 20–80 lights per pixel: **5.51 ms** for all direct
lighting. Opaque part: sampling 0.70 ms, screen traces 0.47 ms, hardware RT 1.35 ms, shading 0.55 ms,
denoising 0.96 ms; translucency and fog the rest.

**Sun.** Directional lights are off by default in MegaLights; the docs recommend deferred lighting with VSM for
a strong sun [8]; the talk caps the directional light at 50% of samples [7].

**wgpu 30 mapping.** Feasible with ray queries, but it solves the many-lights problem; kalast has one light.

### 1.5 Hardware RT and the Path Tracer

**UE hardware RT.** Nanite meshes enter the BVH as low-detail "fallback meshes" by default
(`r.RayTracing.Nanite.Mode 0`), or as streamed geometry with mode 1 [12]. Above 100k instances, scene update
costs become "significant" [12].

**Path Tracer** [13]: progressive and "unbiased", sharing no code with the real-time ray tracing. Accumulates
while the camera is still; Russian roulette; settings for maximum bounces, samples per pixel, filter width. A
firefly clamp ("Max Path Intensity") biases the result. Movie Render Queue renders in tiles. Denoisers: NNE
Denoiser (default; OIDN networks on GPU), OIDN on CPU ("identical results"), NFOR (temporal, for animation),
OptiX (NVIDIA only). Epic's docs do not describe its light sampling (NEE, MIS, how the sun cone is sampled)
[unverified]; whether the sun model includes limb darkening is also [unverified].

**Platforms.** The docs list Windows, DX12 and DXR GPUs [13]. One reading of the 5.8 release notes reported
"Path Tracing is now supported on Mac" through Metal ray shaders, macOS ≥ 26.4 because of a driver bug [9];
a second pass for this survey did not find that sentence in the release-notes text, and the 5.8 macOS
requirements page lists Nanite/VSM as M2+ (beta) and Lumen HWRT/MegaLights as M2+ (experimental) but does not
mention the path tracer [16]: treat path tracing on Mac as [unverified].

**Reference-quality AA in Movie Render Queue.** "Spatial samples" re-render the frame with the camera
jittered; with more than 8 samples Epic says to set the anti-aliasing override to None [14].

**wgpu 30 mapping.** Inline ray queries work in compute on Vulkan, DX12 and Metal (experimental); ray tracing
pipelines are Vulkan-only. A progressive path tracer with NEE to a cone/disc light is straightforward WGSL.

### 1.6 TSR (Temporal Super Resolution)

**Algorithm** [15]: history kept at display resolution, or 200% at High and above (4× more expensive history
update); reprojection with depth and velocity parallax heuristics, on async compute; shading rejection by a
"handwritten convolution network"; flicker analysis against moiré; older frames resurrected from a texture
array; thin-geometry detection added 5.6–5.8 [9][10][11].

**Cost** [15]: Fortnite Chapter 4 on PS5/XSX about 1.5 ms effective with about 0.5 ms hidden by async compute;
a doc example on unstated hardware: about 0.79 ms at 100% screen percentage, 0.43 ms at 50%. At 50% screen
percentage and 60 Hz, a disocclusion needs 66.6 ms to reach 1 spp.

**Artefacts.** Ghosting from bad motion vectors, flicker on Nanite detail, frame-rate-dependent heuristics.
Runs on D3D11/12, Vulkan, Metal. **wgpu 30 mapping:** portable compute, but history-dependent, so not
deterministic.

### 1.7 Substrate

Production-Ready in 5.7 [19]: a slab-based layered material framework. Not relevant to kalast's photometric
laws.

### 1.8 UE6: Epic's statements only

- **4 May 2025** (article date; Lex Fridman podcast). Sweeney: previews "perhaps two to three years from
  now"; the biggest limitation is "the single-threaded nature of game simulation" [22].
- **17 June 2026**, State of Unreal, Unreal Fest Chicago [23][24]: UE6 "will debut in 2027 (ish)", Early
  Access at the end of 2027 and full release 12 to 18 months later; UE5 and UEFN merged, with Verse; Marcus
  Wassmer: "doing better rendering and running games", faster cooks, fewer default shaders (fewer PSO hitches).
- **No rendering algorithm has been announced.** Anything beyond this is speculation.

### Mapping summary (wgpu 30.0.1)

| Technique | Needs | RTX 5080 (Vulkan/DX12) | M1 Pro (Metal) |
|---|---|---|---|
| VSM page system and SMRT | compute, 32-bit atomics | yes | yes; multi-draw emulated |
| Nanite software raster | 64-bit atomic max | yes | **no** (Apple7) |
| Nanite-style hardware raster with culling | indirect draws | yes | yes, no indirect count |
| Lumen/MegaLights HWRT, Path Tracer | ray query | yes (experimental) | yes (experimental, driver traversal on M1) |
| TSR | compute | yes | yes |

### Takeaways for kalast

- **For a sun that keeps moving, Epic's answer is not to cache**: render only the shadow texels the screen
  needs (page marking plus cluster culling) and lower resolution while the light moves. Caching pays only while
  sim time is paused; kalast's Sun direction changes every step in each body's frame.
- **No shadow-map method here is exact.** SMRT, like kalast's 32-direction walk and second depth layer, is a
  heuristic; Epic documents leaks from hidden occluders and a gap-filling fix, and in 5.8 calls a prefiltered
  approach closer to ground truth than SMRT.
- **UE's reference is a separate, unbiased, progressive path tracer** with jittered spatial samples, TAA off,
  the denoiser optional. That is the template for the "error-free" toggle: ray queries in wgpu 30 on all three
  backends, fixed jitter and seeds, no TSR/TAA, no firefly clamp, no denoiser in exported images.
- **A Nanite-style software rasteriser is blocked on this M1 Pro** (no 64-bit atomic min/max; Epic itself
  requires M2+ for Nanite and VSM on Mac). Use hardware raster with compute culling and indirect draws there.
- **Nanite renders shadows at about 2 px of LOD error.** Any LOD in the shadow pass becomes a physics error in
  the TPM's per-facet shadow fractions: the TPM needs full-resolution casters or per-facet ray queries; LOD is
  acceptable only for display.
- **Lumen and MegaLights are biased, temporally denoised, built for many lights.** Patterns worth reusing in
  interactive mode: screen-space trace first, then the BVH; 1 ray per pixel plus a denoiser. Multi-bounce
  light in permanently shadowed craters should come from radiosity or path tracing, not Lumen-like caches.

### Sources (section 1)

1. Epic, "Virtual Shadow Maps" (UE docs, rev. 2025-11). https://dev.epicgames.com/documentation/en-us/unreal-engine/virtual-shadow-maps-in-unreal-engine
2. Lauritzen & Olsson, "Virtual Shadow Maps in Fortnite Battle Royale Chapter 4", UE tech blog, 2023 (read via archive.org copy). https://www.unrealengine.com/en-US/tech-blog/virtual-shadow-maps-in-fortnite-battle-royale-chapter-4
3. Karis, Stubbe, Wihlidal, "Nanite: A Deep Dive", SIGGRAPH 2021 Advances. https://advances.realtimerendering.com/s2021/Karis_Nanite_SIGGRAPH_Advances_2021_final.pdf
4. Wright, Narkowicz, Kelly et al., "Lumen: Real-time GI in UE5", SIGGRAPH 2022 Advances. https://advances.realtimerendering.com/s2022/SIGGRAPH2022-Advances-Lumen-Wright%20et%20al.pdf
5. Epic, "Lumen Performance Guide". https://dev.epicgames.com/documentation/en-us/unreal-engine/lumen-performance-guide-for-unreal-engine
6. Epic, "Lumen Technical Details". https://dev.epicgames.com/documentation/en-us/unreal-engine/lumen-technical-details-in-unreal-engine
7. Narkowicz & Costa, "MegaLights: Stochastic Direct Lighting in UE5", SIGGRAPH 2025 Advances. https://www.advances.realtimerendering.com/s2025/content/MegaLights_Stochastic_Direct_Lighting_2025.pdf
8. Epic, "MegaLights". https://dev.epicgames.com/documentation/en-us/unreal-engine/megalights-in-unreal-engine
9. Epic, "Unreal Engine 5.8 Release Notes", 2026. https://dev.epicgames.com/documentation/en-us/unreal-engine/unreal-engine-5-8-release-notes
10. T. Looman, "UE 5.7 Performance Highlights" (from Epic's notes), 2025. https://tomlooman.com/unreal-engine-5-7-performance-highlights/
11. T. Looman, "UE 5.6 Performance Highlights", 2025. https://tomlooman.com/unreal-engine-5-6-performance-highlights/
12. Epic, "Hardware Ray Tracing". https://dev.epicgames.com/documentation/en-us/unreal-engine/hardware-ray-tracing-in-unreal-engine
13. Epic, "Path Tracer". https://dev.epicgames.com/documentation/en-us/unreal-engine/path-tracer-in-unreal-engine
14. Epic, "Cinematic Rendering Image Quality Settings" (Movie Render Queue). https://dev.epicgames.com/documentation/en-us/unreal-engine/cinematic-rendering-image-quality-settings-in-unreal-engine
15. Epic, "Temporal Super Resolution". https://dev.epicgames.com/documentation/en-us/unreal-engine/temporal-super-resolution-in-unreal-engine
16. Epic, "macOS Development Requirements" (UE 5.8). https://dev.epicgames.com/documentation/en-us/unreal-engine/macos-development-requirements-for-unreal-engine
17. Epic API, `UDirectionalLightComponent::LightSourceAngle` (UE 4.26; search snippet). https://docs.unrealengine.com/4.26/en-US/API/Runtime/Engine/Components/UDirectionalLightComponent/LightSourceAngle/index.html
18. GameFromScratch, "Unreal Engine 5.8 Released", 17 June 2026. https://gamefromscratch.com/unreal-engine-5-8-released/
19. CG Channel, "Unreal Engine 5.7 is here", 12 Nov 2025. https://www.cgchannel.com/2025/11/unreal-engine-5-7-five-key-features-for-cg-artists/
20. P. Turner, ue5-nanite-macos (archived August 2024). https://github.com/philipturner/ue5-nanite-macos
21. Sorensen et al., "Specifying and Testing GPU Workgroup Progress Models", OOPSLA 2021. https://multicore.doc.ic.ac.uk/publications/gpu-oopsla-21.html
22. Wccftech, "First UE6 info … preview in 2-3 years", 4 May 2025. https://wccftech.com/first-unreal-engine-6-info-shared-by-epics-tim-sweeney-preview-versions-in-2-3-years-goal-is-to-go-multithreaded/
23. GamesBeat, "UE6 will combine UE5 and UEFN … (State of Unreal)", June 2026. https://gamesbeat.com/unreal-engine-6-will-combine-ue5-and-uefn-into-a-unified-engine-state-of-unreal/
24. Inven Global, "Tim Sweeney: UE6 will fundamentally change the development paradigm", June 2026. https://www.invenglobal.com/articles/22928/ceo-tim-sweeney-unreal-engine-6-will-fundamentally-change-the-development-paradigm
25. T. Looman, "UE 5.8 Performance Highlights" (quotes the 5.8 VSM changes: Prefiltered Distant, `r.Shadow.Virtual.DeferredInvalidationBudget`), 2026. https://tomlooman.com/unreal-engine-5-8-performance-highlights/

---

## 2. Soft-shadow filtering

**What "exact" means here.** At a receiver the Sun term is F·(n·s)·S, where S is the visible fraction of the
solar disc weighted by limb darkening:

S = ∫disc I(μ)·V(ω) dω / ∫disc I(μ) dω

The disc's angular diameter is about 0.533°/r[au] [34]. Neckel & Labs (1994) fitted I(μ) with fifth-order
polynomials at 30 wavelengths from 303 to 1099 nm [35]. A method is "exact" only if it computes S against all
occluder layers with that weighting.

Worked numbers for a straight horizon with a linear limb-darkening coefficient u = 0.6 (u assumed; arithmetic
done for this survey):
- A uniform disc in place of a limb-darkened one changes S by at most 0.020 (absolute).
- The 1-D "vertical fraction of the disc above the horizon" used by a lunar landing-site tool [34] is off by up
  to 0.058 against the uniform-disc area and up to 0.077 against the limb-darkened value.
- Near grazing it is off by about ×4: with the horizon at 0.9 radii it gives 0.05 where the true value is
  0.013.

| Method | Correct against the Sun's disc? | Main artifacts | Published cost |
|---|---|---|---|
| CSM + fixed PCF (Poisson/rotated disc, optimised "Witness" PCF, receiver-plane bias, normal offset) | No: a constant-width blur of a hard shadow, no contact hardening | Bias trade-off between acne and peter-panning | Optimised 2×2→7×7 PCF ≈ +0.4 ms at 1080p (HD 7950); 2–3 ms with a naive grid [9] |
| PCSS [1]; PCSS+ [3]; DPCF [6]; AMD CHS [8] | Plausible only: assumes light, blocker and receiver are parallel planes, with one averaged blocker depth [1] | Wrong occluder fusion [12]; the search radius caps penumbra width; banding and noise | 36 search + 36–256 PCF taps [1]. 1600×1200, 8800 GT: 8.3/10.4/18.5 ms frame for 5×5 search + 5×5/9×9/17×17 PCF [2]. 1080p, GTX 980: 8.08 ms frame with a 15×15 search [12] |
| HFTS: frustum-traced hard shadow blended into PCSS [4][5] | Exact hard shadow (32 spp) near contact; PCSS beyond | Blend zone; needs conservative raster (Maxwell+) | GTX Titan X, 1080p (read off bar charts): PCSS ≈1.5–2.1, FT ≈2.7–4.2, HFTS ≈3.9–6.3 ms [4] |
| VSM / SAVSM / ESM / EVSM / layered VSM [2][9][10] | No: statistical bounds on hard visibility; softness from a summed-area table plus PCSS | Light leaking when the kernel holds several depths; breaks if receivers are missing from the map [2] | EVSM4, 2048² cascades, 4×MSAA and mips ≈ +11.5 ms over 7×7 PCF (1024²: ≈ +3 ms) [9]. SAVSM: 6.5 ms of a 14.7 ms frame (8800 GT) [2] |
| MSM [11]; moment soft shadow mapping [12] | Sharpest lower bound from 4 moments; soft version uses the PCSS framework | Weak leaking; wrong fusion when more than 2 surfaces fall in the kernel | 64 bits/texel (16-bit quantised) [11]. Soft: 2.11–2.51 ms full frame vs 8.08 ms for PCSS (1080p, 1024² map, GTX 980) [12] |
| Backprojection / micro-patches [13][14][16] | Close to a 1024-sample reference [14], but uses one depth image from the light's centre | Gaps leak; overlaps over-shadow; gap filling overestimates [14] | 24 fps vs 41 fps for hard shadows at 768² (8800 GTS); hierarchy build < 1 ms for 1k² [14] |
| + bitmask over light samples [15] | Correct fusion over the samples; can combine several depth maps | The light is discretised | No numbers verified |
| + depth peeling / multilayer maps [17][18] | Catches hidden occluders up to k layers | k not guaranteed; slight over-shadowing remains | 3 layers removed "most" leaking [17] |
| SMRT, UE virtual shadow maps [19] | Plausible: rays spread over the Sun's angular size, tested against the first depth layer only | Gap-fill extrapolation heuristic; penumbra clamped; noise at low ray counts | 8 rays/pixel at Epic quality, 4–8 samples per ray [19] |
| SDF cone tracing [20][21] | Approximate cone from the closest-distance value | Depends on SDF resolution; rigid meshes only in UE | About one ray march [20] |
| Screen-space shadows (Bend Studio, UE contact shadows) [22][23] | No: depth buffer plus a thickness guess | Disocclusion, off-screen occluders, thickness assumption | 60 samples, full screen, 1440p, PS5: 0.19 ms [22] |
| Ray tracing + denoiser (NRD SIGMA, AMD FidelityFX) [24]–[28] | Each ray unbiased; limited by samples per pixel and by the denoiser | Temporal lag, ghosting, blur | SIGMA 0.40 ms at 1440p on an RTX 4080 [24]; UE defaults to 1 spp [28] |
| Reference: Monte Carlo, soft shadow volumes for ray tracing, sample-based, horizon maps [29]–[34] | Exact in the limit (MC); exact per light sample [30][31]; exact for heightfields (horizons) | Cost; horizon methods miss overhangs | 128–1024 light samples per pixel at real-time rates [31]; 1024² heightfield, 64 directions at 23 fps on a GTX 280 [32] |

### Notes per method

**CSM + PCF.** MJP measured optimised bilinear PCF (Castaño's technique from The Witness) as far cheaper than
naive grids, and describes receiver-plane depth bias (from screen-space derivatives) as working well "except
degenerate cases" [9]. Taking PCF taps on the receiver's tangent plane is the same idea per tap: Dou et al. (I3D
2014) compute a per-fragment bound and pick the optimal bias inside it; Ehm et al. (2015) extend this to PCF and
PCSS [37] (their paper could not be read; repository returned 503). This removes acne without a global bias,
but the result is still a filter, not an integral over the disc.

**PCSS family.**
- **PCSS.** Penumbra width (d_receiver − d_blocker)·w_light/d_blocker, from similar triangles. Blockers are
  averaged, because the minimum gives "artifacts when transitioning between blockers" [1].
- **Main failure.** Every PCSS-type method replaces occluders at different depths with one at their average
  depth; in [12] a pillar's contact shadow goes soft because of a wall further back.
- **PCSS+** (NVIDIA ShadowWorks) adds directional-light support with adaptive quality per cascade, aliasing
  reduction ("penumbra control as blocker depth reaches zero"), a "convergence testing" algorithm for speed,
  and dithered radii against banding [3].
- **DPCF.** The only concrete use found is Google Filament, documented as "PCF with contact hardening
  simulation", citing Kevin Myers' "Shadows of Cold War" (Treyarch, GDC 2021) [6][7]. In Filament v1.50.0 it
  runs one 12-tap rotated-Poisson blocker search, reuses those taps for filtering, then blends a hardened
  kernel with the soft result by penumbra ratio (Filament's PCSS uses 16 + 16 taps) [6]. Current Filament
  marks DPCF deprecated (falls back to PCSS). What the "D" stands for is never stated; Unity HDRP docs and a
  general search found nothing else.
- **AMD CHS**: a contact-hardening kernel in the ShadowFX library (DX11/DX12) [8].
- **"Cone-traced PCSS"** is not an established term. Nearest families: SDF cone tracing [20][21], ray marching
  through depth maps (SMRT [19], multilayer transparent shadow maps [18]), min-max hierarchical blocker search
  [2][14].

**HFTS.** lerp(FT, PCSS, L) with L = saturate(blocker distance / world scale · percentage of hard shadow), on the
first cascade only [4]. FT is a frustum-traced irregular z-buffer: 32 spp hard shadows at roughly twice the cost
of 1 spp, structure rebuilt in under 2 ms [5].

**Filterable maps.** VSM leaks when several occluder depths share the kernel (Chebyshev's bound
σ²/(σ²+(d−μ)²) tends to 1); the leak-reduction remap darkens penumbrae as a side effect [10][2]. Summed-area
tables give arbitrary kernel sizes but consume float precision: a 512² table uses 18 of the 23 mantissa bits
[10]. EVSM needs 4×FP32 per texel [9]. Moment soft shadow mapping stores an integer summed-area table of 4
moments (4×32-bit) and does the blocker search in one table query; its cost scales with shadow-map texels, not
output pixels, which is why it beats PCSS; it still leaks on short-range shadows [12].

**Backprojection.** Shadow-map texels are unprojected as micro-patches, backprojected onto the light, and the
occluded areas summed [13][16][2]. Inherent errors: gaps leak, overlaps over-shadow, gap filling worsens the
overestimation [14]. ASSM (2007) detects occluder contours and integrates radially, close to the reference; the
remaining error is the "single light sample approximation" (one depth image from the light's centre), largest
where penumbrae of occluders at very different depths merge; the authors suggest layered depth images [14].
**Bitmask soft shadows**: one bit per light sample; OR-ing removes double-counted overlaps, solves occluder
fusion and lets several depth maps be combined [15]. **Depth peeling**: three peeled layers removed most leaking,
not guaranteed (an oblique quad still leaks), slight over-shadowing remains, results "similar to ray tracing"
[17].

**SMRT.** Unreal's docs: only the first depth layer is stored, so overlapping occluders leak; a "gap-filling
heuristic" extrapolates depth behind the first occluder; the penumbra is clamped so samples do not "bend"; there
are "limitations inherent to using the data from a single shadow map projection", and "ray-traced shadows …
generally provide the highest quality solution" [19].

**Screen-space shadows.** Bend Studio: each screen-space ray maps to one 64-thread wavefront, depth shared
through local memory with an edge-aware manual bilinear depth; 60 samples by default, the first 4 forced hard;
deterministic ("does not rely on dithering or random sampling"); disocclusion is a "fundamental limitation …
they cannot replace shadow maps, only complement them"; 0.19 ms at 1440p on PS5; Apache-2.0 HLSL [22]. UE
contact shadows use a fixed sample count, so noise grows with shadow length [23] (search summaries; an Epic
forum post gives "8 steps", unchecked against the docs).

**Ray tracing + denoising.** Disc lights are sampled as a cone of directions [36] (weighting or
importance-sampling by I(μ) is a suggestion, not from a source). UE uses 1 spp plus a denoiser by default [28].
NRD SIGMA needs hit distance and penumbra radius, supports directional lights with angular size, runs at 1 ray
per pixel, recommends blue noise against shimmer, costs 0.40 ms at 1440p on an RTX 4080, and ships as HLSL for
D3D11/D3D12/Vulkan [24]. AMD's FidelityFX shadow denoiser takes at most 1 jittered ray per pixel packed as 8×4
tile bitmasks, then reprojection and three à-trous passes [25]. AMD Hybrid Shadows classifies tiles with the
cascaded shadow maps (fully lit and fully shadowed tiles get no rays), takes the ray interval from the cascades,
has a "Sun Solid Angle" parameter, traces 1 spp then denoises [26]. Boksansky et al. concentrate shadow rays and
filtering on penumbrae adaptively [27]. These denoisers are temporal filters: a single frame is not exact.

**Reference methods.** Ratio estimator: shadowed illumination splits exactly into analytic unshadowed shading
times a stochastic visibility ratio [29]. Soft shadow volumes for ray tracing: silhouette edges plus one ray
reconstruct visibility, "exactly the same image" as many-ray tracing, 10–100× faster [30]. Exact horizons in N
azimuths can be computed in linear time [32]. Planetary practice: Mazarico et al. compute horizon elevations
from 240 m LOLA models and validate against LRO WAC images [33]; the ICAT tool uses 720 azimuths (0.5°) and a
0.533° disc but computes "only the vertical fraction"; STK treats the Sun as a point (roughly a 50% threshold);
APL's LunarShader applies a sigmoid to a centre ray [34].

### Takeaways for kalast

- **The 32-direction "sheet" walk is the horizon method done in light space** [32][33][34], with gap filling
  built in as in backprojection [13][14]. It is exact for occluders forming one layer (a heightfield) provided
  each azimuth's horizon is integrated as a disc area weighted by I(μ); the vertical-fraction shortcut errs by
  up to 0.077. Its expected bias is over-shadowing where a sheet bridges a depth jump, the overestimation the
  gap-filling literature documents [14][17].
- **To merge layers and occluders without double counting**, keep a bitmask per receiver over disc samples
  weighted by limb darkening [15]. A second peeled layer catches most hidden occluders, but [17] needed three,
  and no fixed number of layers is guaranteed.
- **PCF, PCSS, VSM, MSM and SMRT all fail in documented ways** (parallel planes, one averaged blocker, leaking,
  first layer only) [1][10][12][19]: fine for display, not for the TPM's shadow fractions.
- **The screen-space near-field march matches Bend's method**: cheap (0.19 ms at 1440p), deterministic, but
  view-dependent and limited by disocclusion [22]. It should never feed the TPM; in the error-free mode replace
  it with rays.
- **For the error-free mode, rays to a stratified disc, no denoiser.** Rays cover hidden occluders without extra
  layers; the ratio estimator [29] keeps unshadowed shading noise-free; send rays only from penumbra tiles that
  the existing depth-pyramid check finds, as AMD Hybrid Shadows does [26], so cost scales with penumbra area.

### Sources (section 2)

1. Fernando, "Percentage-Closer Soft Shadows", SIGGRAPH 2005 — https://http.download.nvidia.com/developer/presentations/2005/SIGGRAPH/Percentage_Closer_Soft_Shadows.pdf
2. Bavoil, soft shadow mapping talk, GDC 2008 — https://developer.download.nvidia.com/presentations/2008/GDC/GDC08_SoftShadowMapping.pdf
3. NVIDIA ShadowWorks product page (PCSS+); release notes — https://archive.docs.nvidia.com/gameworks/content/gameworkslibrary/visualfx/shadowworks/product.html ; https://docs.nvidia.com/gameworks/content/gameworkslibrary/visualfx/shadowworks/releasenotes.html
4. Story, "Hybrid Frustum Traced Shadows", GDC 2016 — https://developer.download.nvidia.com/gameworks/events/GDC2016/jstory_hfts.pdf
5. Wyman, Hoetzlein, Lefohn, "Frustum-Traced Raster Shadows", I3D 2015 — https://research.nvidia.com/labs/rtr/publication/wyman2015frustum
6. Filament Options.h (main) and shadowing.fs (v1.50.0) — https://raw.githubusercontent.com/google/filament/main/filament/include/filament/Options.h ; https://raw.githubusercontent.com/google/filament/v1.50.0/shaders/src/shadowing.fs
7. Myers, "Shadows of Cold War", GDC 2021 (Treyarch listing) — https://www.treyarch.com/studio-culture/2021/07/Treyarch_at_GDC_2021
8. AMD ShadowFX (CHS) — https://github.com/GPUOpen-Effects/ShadowFX
9. MJP, "A Sampling of Shadow Techniques", 2013 (updated 2015) — https://therealmjp.github.io/posts/shadow-maps/
10. Lauritzen, "Summed-Area Variance Shadow Maps", GPU Gems 3, 2007 — https://developer.nvidia.com/gpugems/gpugems3/part-ii-light-and-shadows/chapter-8-summed-area-variance-shadow-maps
11. Peters & Klein, "Moment Shadow Mapping", I3D 2015 — https://momentsingraphics.de/I3D2015.html
12. Peters et al., "Beyond Hard Shadows: Moment Shadow Maps for Single Scattering, Soft Shadows and Translucent Occluders", I3D 2016 — https://momentsingraphics.de/I3D2016.html
13. Guennebaud et al., "Real-time Soft Shadow Mapping by Backprojection", EGSR 2006 — https://www.labri.fr/perso/guenneba/SoftShadowMapping_egsr06.php
14. Guennebaud et al., "High-Quality Adaptive Soft Shadow Mapping", Eurographics 2007 — https://www.labri.fr/perso/guenneba/docs/ASSM_eg07.pdf
15. Schwarz & Stamminger, "Bitmask Soft Shadows", Computer Graphics Forum 26(3), 2007 — https://diglib.eg.org/handle/10.2312/CGF.v26i3pp515-524
16. Atty et al., "Soft Shadow Maps: Efficient Sampling of Light Source Visibility", Computer Graphics Forum 2006 — https://diglib.eg.org/items/50427b8c-d560-4829-9172-578f2f212821
17. Bavoil, Callahan, Silva, "Robust Soft Shadow Mapping with Backprojection and Depth Peeling", JGT 2008 — https://www.sci.utah.edu/~csilva/papers/jgt08.pdf
18. Xie, Tabellion, Pearce, "Soft Shadows by Ray Tracing Multilayer Transparent Shadow Maps", EGSR 2007 — https://diglib.eg.org/handle/10.2312/EGWR.EGSR07.265-276
19. Epic, Virtual Shadow Maps (SMRT) — https://dev.epicgames.com/documentation/en-us/unreal-engine/virtual-shadow-maps-in-unreal-engine
20. Epic, Distance Field Soft Shadows — https://dev.epicgames.com/documentation/en-us/unreal-engine/distance-field-soft-shadows-in-unreal-engine
21. Tan et al., "RTSDF", arXiv 2022 — https://arxiv.org/abs/2210.06160
22. Bend Studio / Aldridge, Screen Space Shadows (SIGGRAPH 2023 slides and code) — https://www.bendstudio.com/blog/inside-bend-screen-space-shadows/
23. Epic, Contact Shadows — https://dev.epicgames.com/documentation/unreal-engine/contact-shadows-in-unreal-engine
24. NVIDIA NRD README (SIGMA) — https://github.com/NVIDIA-RTX/NRD
25. AMD FidelityFX Denoiser — https://gpuopen.com/manuals/fidelityfx_sdk/techniques/denoiser/
26. AMD FidelityFX Hybrid Shadows — https://gpuopen.com/manuals/fidelityfx_sdk/samples/hybrid-shadows/
27. Boksansky, Wimmer, Bittner, "Ray Traced Shadows: Maintaining Real-Time Frame Rates", Ray Tracing Gems, 2019 — https://www.cg.tuwien.ac.at/research/publications/2019/BOKSANSKY-2019-RTS
28. Epic, Ray Tracing and Path Tracer feature properties — https://dev.epicgames.com/documentation/en-us/unreal-engine/ray-tracing-and-path-tracer-features-properties-in-unreal-engine
29. Heitz, Hill, McGuire, "Combining Analytic Direct Illumination and Stochastic Shadows", I3D 2018 — https://research.nvidia.com/publication/2018-05_Combining-Analytic-Direct
30. Laine et al., "Soft Shadow Volumes for Ray Tracing", SIGGRAPH 2005 — https://www.cse.chalmers.se/~uffe/SoftShadowVolumesForRayTracing.pdf
31. Sintorn, Eisemann, Assarsson, "Sample Based Visibility for Soft Shadows using Alias-free Shadow Maps", EGSR 2008 — https://research.chalmers.se/en/publication/85193
32. Timonen & Westerholm, "Scalable Height Field Self-Shadowing", Eurographics 2010 — https://diglib.eg.org/handle/10.2312/CGF.v29i2pp723-731
33. Mazarico et al., lunar polar illumination from LOLA topography, Icarus 2011 — https://ntrs.nasa.gov/citations/20120010094
34. De Rosa et al., landing-site characterisation for ESA's Lunar Lander, 2012 — https://arxiv.org/pdf/1208.5587
35. Neckel & Labs, Solar Phys. 153, 91 (1994), via Witzke et al. 2024 — https://arxiv.org/abs/2310.05652
36. PBRT (3rd ed.), §14.2 Sampling Light Sources — https://www.pbr-book.org/3ed-2018/Light_Transport_I_Surface_Reflection/Sampling_Light_Sources
37. Dou et al., "Adaptive Depth Bias for Shadow Maps", I3D 2014 — https://www.aminer.org/pub/53e9a154b7602d9702a430af/adaptive-depth-bias-for-shadow-maps ; Ehm et al. 2015 (details unverified) — https://dspace.zcu.cz/items/a6ee579f-d41f-422d-bcb0-80d9d67cfc01

---

## 3. Anti-aliasing

| Method | Quality | Cost (published) | Temporal behaviour / determinism | From wgpu 30 / WGSL |
|---|---|---|---|---|
| SSAA (render N×, filter down) | Reference: integrates edges, shading, shadow lookups, sub-pixel objects | ≈N× fragment work and N× target memory | No history; deterministic for a fixed pattern | Trivial (larger target, or N jittered passes + compute resolve) |
| MSAA 2/4/8× | Geometric edges only; shader runs once per pixel per triangle [17] | 2× render overhead 1.57 ms (GTX 470, 1080p) [1] | No history; deterministic | Core. 1×/4× guaranteed, 2/8/16× per adapter [26]. M1 Pro: 1, 2, 4, no 8 (measured) [25]. RTX: 1/2/4/8 typical [unverified for 5080] |
| FXAA (Lottes 2009) | Luma-edge blur; blurs textures/text; no sub-pixel recovery [3] | "1 ms ballpark" with SMAA 1x [1] | Single frame, deterministic; shimmers in motion | WGSL in Bevy [10] |
| SMAA 1x | Sharper than FXAA, accurate gradients | 1.02 ms (GTX 470, 1080p) [1] | Single frame, deterministic | WGSL in Bevy (1x only, luma edges) [10] |
| SMAA S2x / T2x / 4x | Close to SSAA 16× (paper Fig. 13) [1] | 2.04 / 1.32 / 2.34 ms (same setup) [1] | T2x and 4x reproject the previous frame → history | No WGSL port found (Bevy lacks T2x [10]) |
| SRAA (2011) | "comparable to 4-16x shading" at 1× shading [2] | 1.8 ms at 1280×720 [2] | Single frame; needs super-res depth+normals | No implementation found [unverified] |
| TAA (Karis 2014 [4]) | Also filters shading/specular aliasing; soft | sub-ms class [unverified] | Ghosting, blur, thin lines "may flicker noisily or disappear" [5][10]; EMA never converges | WGSL in Bevy: depth + motion vectors, MSAA off, `reset` flag [10] |
| UE TSR | Best non-ML temporal upscaler | ≈1.5 ms effective on PS5/XSX (0.5 ms hidden by async compute); 0.79 ms at 100 % / 0.43 ms at 50 % screen percentage, desktop [6] | History, "history resurrection", flicker heuristics [6] | UE only |
| DLSS SR / DLAA (CNN E,F; transformer J,K; 2nd-gen L,M = "DLSS 4.5") | Highest real-time quality | RTX 5080, 1080p→4K: 0.62 (E/F), 1.31 (J/K), 1.74 (M), 2.24 ms (L); 198-456 MB VRAM at 4K [7] | History + network; presets "subject to change with each revision", OTA model updates, driver overrides [7][8] | `dlss_wgpu` 6.0 (wgpu 30, DLSS 310.9.1): **Vulkan only**, Windows/Linux x86-64, SR + Ray Reconstruction [9]; optional in Bevy [10] |
| FSR 2 / 3.1 (MIT) | Good temporal upscaler | 4K Quality: 1.1 ms RX 6950 XT, 1.2 ms RX 6800 XT [11] | History; Halton(2,3) jitter, 18 (Quality) to 72 (Ultra Perf) phases [11] | C++ with DX12/Vulkan backends; Rust FFI `fsr2-rs` [13]; no WGSL port found |
| FSR 4 (ML, "Redstone" family) | ML quality | none published | History + network | RX 9000 (RDNA 4) only, **DX12 only**, Windows, signed DLLs; else falls back to FSR 3.1.5 [12]. Unreachable from wgpu |
| XeSS SR | ML; XMX on Arc, DP4a (SM 6.4) elsewhere [14] | none found | History + network; closed source [14] | DX11/DX12/Vulkan [14]; only via wgpu-hal Vulkan interop [speculation]; no macOS |
| MetalFX spatial/temporal; Metal 4 denoised upscaler + frame interpolation | ML temporal upscaler; denoiser fused into upscaling, needs normals, roughness, diffuse/specular albedo [15] | none published [15] | History + network | wgpu-hal Metal interop; `bevy_metalfx` 0.4.2 does spatial/temporal/interpolation on wgpu 29 [16] |
| Jittered accumulation ("progressive SSAA") | Converges to the filtered reference | N raster frames | Deterministic for a fixed sequence on a fixed GPU; no reprojection | Trivial |

**What MSAA leaves.** Only coverage and depth are per sample; the fragment shader runs "only once for each pixel where the triangle covers at least one subsample" [17]. Shadow-map lookups, PCF taps, the penumbra walk, texture and BRDF evaluation happen in the shader, so their aliasing passes through MSAA untouched. Reading `@builtin(sample_index)` in WGSL turns on per-sample shading (Vulkan `sampleRateShading`, GL `gl_SampleID` semantics) [18], at SSAA cost on covered samples. On the M1 Pro, wgpu reports 1/2/4 samples for Bgra8Unorm(Srgb) and Depth32Float [25]; wgpu-hal's Metal backend assumes 1 and 4 and asks Metal about 2/8/16 [26].

**Post-process AA.** SMAA (GTX 470, 1080p): 1x 1.02 ms, T2x 1.32, S2x 2.04, 4x 2.34 ms; 4x and T2x are 1.46× and 4.09× faster than MSAA 8× in forward rendering. T2x weights the reprojected sample by w = 0.5·max(0, 1 − K·sqrt(|‖v_c‖−‖v_p‖|)), K = 30, giving up AA at disocclusions to avoid ghosting [1]. FXAA/SMAA 1x cannot recover a feature that the single sample missed; SRAA needs super-resolved depth and normals [2].

**Temporal methods never become references.** TAA is "temporally-amortized supersampling": sample accumulation plus history validation [5]. With an exponential moving average of weight α, the variance of the accumulated value falls only to α/(2−α) of one sample: about 19 effective samples at α = 0.1. History clamping/clipping then biases it further (derived here, not from a source). A cumulative mean (α_k = 1/k) of a static scene is exact supersampling. That is DLSS's debug "accumulation mode", in which "a static scene should resolve perfectly … identical to the same scene rendered at full resolution without DLSS" [7]. It is a debugging aid, not the shipped path.

**ML upscalers and reproducibility.** Their output is the network's output. NVIDIA's guide says presets "are subject to change with each revision" and documents OTA model updates. DLSS 4.5's presets L/M arrived in January 2026, and users can force presets through a driver override [7][8]. Same input → same output therefore holds at best for a pinned DLL on one GPU. Across vendors they are different algorithms: DLSS is NVIDIA-only, FSR 4 is RDNA 4/DX12-only, XeSS's model differs between XMX and DP4a, and MetalFX is Apple-only. They are trained to produce plausible detail, which is wrong for radiometric output [assessment].

**Reference-image anti-aliasing.**
- Offline engine practice: Unreal's Movie Render Queue renders each output frame "multiple times, each time jittering the camera". It recommends anti-aliasing method "None" when using 8+ spatial/temporal samples [23].
- pbrt-v4 merges reconstruction and prefiltering. It draws the image-plane offset from the filter itself and weights by f/p (weight 1 for a box) [19]. Filters (default radius): box 0.5, triangle 2, Gaussian 1.5 (default), Mitchell 2, Lanczos sinc 4. Samplers: halton, independent, stratified, sobol, paddedsobol, zsobol (default, 16 spp) [20]. Box "allows high-frequency sample data to leak", Gaussian blurs slightly, and Mitchell's negative lobes sharpen [19].
- SurRender (Airbus space image simulator) samples flux "stochastically … within the pixels (including the probability density function defined by the PSF)". It outputs irradiance in W/m², aims rays at small objects at sub-pixel level, and was validated on analytic cases including PSF effects. It ran at 0.2 Hz on CPU for 1024² at 128 rays/pixel [24].
- Sequences: DLSS was trained with Halton jitter and FSR 2 uses Halton(2,3) [7][11]. R2 (plastic-constant additive recurrence, Roberts 2018) is a simple 2-D alternative [22]. Scrambled Sobol (pbrt's default) suits integrands with many dimensions: pixel × Sun disc × bounce [20].
- Samples needed. With stratified sampling, pixel variance falls as N⁻² in smooth regions and as ≈N^-3/2 in pixels containing edges [21]. Derived here: a straight full-contrast edge under an n×n jittered grid crosses ≈(4/π)·n strata, each a Bernoulli trial of mean variance 1/6, so RMS ≈ 0.46·N^-3/4 of the edge contrast. That gives 16 spp 5.8 %, 64 → 2.0 %, 256 → 0.72 %, 1024 → 0.25 %, 4096 → 0.09 %. Holding a lit limb against black sky to one 8-bit LSB RMS (0.39 %) takes ≈570 spp; 0.1 % takes ≈3.5k. A regular 16×16 grid quantizes an axis-aligned edge to 1/16 steps (worst case 3.1 %), so use jittered or QMC samples, never a regular grid.
- Shadows: screen-space jitter averages the shading, but not the shadow map's own texel staircase or PCF bias. Those errors live in the shadow map and are identical in every jittered frame unless the shadow projection is also jittered by a sub-texel amount per accumulated frame, or visibility is ray traced [assessment].

### Sources (section 3)
1. Jimenez, Echevarria, Sousa, Gutierrez, "SMAA: Enhanced Subpixel Morphological Antialiasing", CGF/Eurographics 2012. https://www.iryoku.com/smaa/downloads/SMAA-Enhanced-Subpixel-Morphological-Antialiasing.pdf (project: https://www.iryoku.com/smaa/)
2. Chajdas, McGuire, Luebke, "Subpixel Reconstruction Antialiasing", I3D 2011. https://research.nvidia.com/publication/2011-02_subpixel-reconstruction-antialiasing
3. "Fast approximate anti-aliasing" (Lottes 2009 whitepaper; licence), Wikipedia. https://en.wikipedia.org/wiki/Fast_approximate_anti-aliasing
4. Karis, "High-Quality Temporal Supersampling", SIGGRAPH 2014 Advances. https://advances.realtimerendering.com/s2014/
5. Yang, Liu, Salvi, "A Survey of Temporal Antialiasing Techniques", CGF 39(2), 2020. https://research.nvidia.com/labs/rtr/publication/yang2020survey
6. Epic, "Temporal Super Resolution in Unreal Engine" (5.8 docs). https://dev.epicgames.com/documentation/en-us/unreal-engine/temporal-super-resolution-in-unreal-engine
7. NVIDIA, DLSS Super Resolution Programming Guide 310.6.0, March 2026. https://raw.githubusercontent.com/NVIDIA/DLSS/main/doc/DLSS_Programming_Guide_Release.pdf
8. HotHardware, "NVIDIA DLSS 4.5 Benchmarked", Jan 2026. https://hothardware.com/news/nvidia-dlss-45-benchmarked
9. `dlss_wgpu` 6.0.0 docs. https://docs.rs/dlss_wgpu
10. `bevy_anti_alias` 0.20 docs (crate, TAA, SMAA). https://docs.rs/bevy_anti_alias ; https://docs.rs/bevy_anti_alias/latest/bevy_anti_alias/taa/struct.TemporalAntiAliasing.html ; https://docs.rs/bevy_anti_alias/latest/bevy_anti_alias/smaa/index.html
11. AMD, FidelityFX-FSR2 README (MIT, timings, jitter). https://github.com/GPUOpen-Effects/FidelityFX-FSR2
12. AMD GPUOpen, FSR 4 page. https://gpuopen.com/fidelityfx-superresolution-4/
13. `fsr2-rs` Rust bindings. https://github.com/notapenguin0/fsr2-rs
14. Intel XeSS SDK (search-result summaries: SR on DX11/DX12/Vulkan, DP4a/XMX, closed licence). https://github.com/intel/xess ; https://tomshardware.com/pc-components/gpus/intel-finally-releases-the-xess-2-0-sdk-for-developers-technology-still-gated-by-closed-source-barriers
15. Apple, WWDC25 session 211, "Go further with Metal 4 games". https://developer.apple.com/videos/play/wwdc2025/211/
16. `bevy_metalfx` 0.4.2 docs. https://docs.rs/bevy_metalfx
17. Pettineo (MJP), "A Quick Overview of MSAA". https://therealmjp.github.io/posts/msaa-overview/
18. Dawn commit "Enable sampleRateShading for WGSL builtin sample_index". https://git.axiodl.com/encounter/dawn-cmake/commit/9d92f31f21cf857882b34d270770c47a92047e4c ; Khronos `gl_SampleID`: https://registry.khronos.org/OpenGL-Refpages/gl4/html/gl_SampleID.xhtml
19. Pharr, Jakob, Humphreys, PBR 4th ed., "Image Reconstruction". https://pbr-book.org/4ed/Sampling_and_Reconstruction/Image_Reconstruction
20. pbrt-v4 file format (samplers, filters). https://pbrt.org/fileformat-v4
21. Mitchell, "Consequences of stratified sampling in graphics", SIGGRAPH 1996 (via search summary). https://people.csail.mit.edu/ericchan/bib/pdf/p277-mitchell.pdf
22. Roberts, "The Unreasonable Effectiveness of Quasirandom Sequences", 2018. https://extremelearning.com.au/?p=71
23. Epic, "Cinematic Rendering Image Quality Settings" (Movie Render Queue). https://dev.epicgames.com/documentation/en-us/unreal-engine/cinematic-rendering-image-quality-settings-in-unreal-engine
24. Brochard et al., "Scientific image rendering for space scenes with the SurRender software", IAC 2018, arXiv:1810.01423. https://arxiv.org/pdf/1810.01423
25. kalast, `notes/2026-10-07_msaa_counts_and_overlay_antialiasing.md` (local probe on the M1 Pro).
26. wgpu-hal 30.0.1, `src/metal/adapter.rs` (local crate source, sample-count mask).

### Takeaways for kalast
- Use two paths. **Interactive:** MSAA 4× (the M1 Pro's maximum) plus SMAA 1x or FXAA (Bevy's WGSL as reference), or TAA that resets on any camera or simulation change. All are about 1 ms or less. **Export:** reset, render N frames with a fixed jitter sequence (R2/Halton/Sobol), take a cumulative mean, with no reprojection and no ML.
- For spacecraft images, make the accumulation filter the instrument's pixel aperture ⊗ PSF (SurRender's approach), or a box for an ideal flux-preserving detector. Do not use an aesthetic filter.
- Budget about 500-1000 spp for limbs exact to 8 bits. At about 4 ms per raster frame that is 2-4 s per image [derived/speculation].
- Jitter the shadow-map and penumbra inputs per accumulated frame as well, or use ray-traced visibility in the reference mode. Neither MSAA nor screen jitter removes shadow-map aliasing.
- Keep DLSS, FSR, XeSS and MetalFX out of science output. They are vendor-locked, their models drift (OTA updates, presets), and they are reachable only through backend interop: `dlss_wgpu` is Vulkan-only, FSR 4 is DX12-only, MetalFX goes through wgpu-hal Metal.
- For bit-reproducible runs on one machine, accumulate in a fixed order without float atomics. Across vendors, expect differences at float-rounding level only [assessment].

---

## 4. Ray tracing from wgpu in 2026

### 4.1 What wgpu 30 provides

**Release history** (from the CHANGELOG [1])
- v0.16 (Apr 2023): HAL-only ray query and acceleration structures, Vulkan.
- v23 (Oct 2024): BLAS/TLAS and ray queries in the public API, Vulkan (#6291).
- v24 (Jan 2025): DXR in wgpu-hal (#6777); WGSL ray-flag constants; candidate intersections.
- v25 (Apr 2025): hit-triangle "vertex return" (Vulkan only); BLAS compaction in HAL.
- v26 (Jul 2025): BLAS compaction in the public API; AS limits; extra AS vertex formats.
- v27 (Oct 2025): the AS feature merged into `EXPERIMENTAL_RAY_QUERY`.
- v28 (Dec 2025): `enable wgpu_ray_query;` mandatory; SPIR-V undefined-behaviour fixes.
- v29 (18 Mar 2026): Metal acceleration structures (#8071, merged 18 Feb 2026), making ray queries usable
  end-to-end on Metal; TLAS binding arrays; naga front/back-end for WGSL ray-tracing pipelines.
- v30 (1 Jul 2026): procedural AABB BLAS; SPIR-V output for RT pipelines; Metal ray-query safety guards
  (#9442); Metal bindless storage buffers.

**Backends in 30.0.1** (local crate sources [2]; see also section L)
- **Vulkan:** ray query (VK_KHR_ray_query, acceleration_structure, buffer_device_address,
  deferred_host_operations) and vertex return.
- **DX12:** ray query when DXR tier ≥ 1.1 and SM ≥ 6.5. **Gotcha** (re-checked in `wgpu-types` 30.0.1
  `backend.rs`): the default `Dx12Compiler::Auto` uses static DXC only if the non-default `static-dxc` cargo
  feature is on, otherwise `dxcompiler.dll` on PATH, otherwise FXC (SM 5.1), which means **no ray queries**.
  kalast creates its instance with the default descriptor (`src/app/window.rs`), so if wgpu picks DX12 on
  Windows it would not see ray queries unless DXC is present (`WGPU_DX12_COMPILER` also selects it). Vulkan is
  unaffected.
- **Metal:** ray query when macOS ≥ 15 (MTLResidencySet) and `supportsRaytracing && supportsRaytracingFromRender`;
  this M1 Pro (macOS 26.4.1, Apple7) reports both true [33]. naga emits `metal::raytracing::intersection_query`;
  no vertex return on MSL.
- **Stale doc comment:** `EXPERIMENTAL_RAY_QUERY` still says "Vulkan" only; the code exposes DX12 and Metal.
- **RT pipelines:** naga support and HAL entry points exist (the `EXPERIMENTAL_RAY_TRACING_PIPELINES` flag can be
  set by the Vulkan HAL), but wgpu / wgpu-core 30.0.1 have **no public pipeline API** (re-checked: no
  ray-tracing-pipeline symbols in either crate). The design is open (#6760); Metal has no pipeline concept
  (#8560) [3].

**WGSL and limits**
- `enable wgpu_ray_query;` · `var tlas: acceleration_structure;` · `var rq: ray_query;` ·
  `rayQueryInitialize(&rq, tlas, RayDesc(flags, cull_mask, t_min, t_max, origin, dir))`, then
  `rayQueryProceed`; `rayQueryGetCommittedIntersection` returns `{kind, t, instance_index, primitive_index,
  barycentrics, front_face, …}`; flags include `RAY_FLAG_TERMINATE_ON_FIRST_HIT`.
- Ray queries work in fragment and compute shaders; the `ray_shadows` example traces fragment-shader shadow
  rays, which is kalast's case [4][5].
- Limits: BLAS up to 2^29 primitives on DX12 and 2^28 on Metal (driver-reported on Vulkan); TLAS up to 2^24
  instances. On Metal, acceleration structures share the 31-slot buffer table
  (`max_buffers_and_acceleration_structures_per_shader_stage`); Bevy had to repack its buffers for this [14].

**Maturity**
- Everything is `EXPERIMENTAL_*` ("may have major bugs… breaking changes") [2]; releases come quarterly.
- Missing: AS update/refit ("currently unsupported" [4]), NaN-vertex handling (#8204), opacity micromaps,
  PTLAS/CLAS [3].
- Open bugs [7]: #9215 Metal BLAS→TLAS builds not synchronised (fence fix in progress; workaround: separate
  submissions); #9100 Metal RT tests fail on M3 ("seems to only happen on tests"); #8825 compaction flake in CI;
  #9946 TLAS build costs milliseconds on the CPU at ~10^5 instances.
- Metal RT is not covered by CI: the PR notes the runner "does not support hardware ray tracing" [6].
- naga adds ray-query initialisation checks by default; `create_shader_module_trusted` with
  `ShaderRuntimeChecks::unchecked()` removes them [2]; their cost is not measured [unverified].
- Open issue #9414 reports Metal timestamps as all zeros on macOS 26 [8]; worth checking against kalast's
  `gpu_timing` on the Mac [unverified whether kalast is affected].

**Browser WebGPU:** no ray tracing. gpuweb #535 open since 2020 [9]; not on Chrome's roadmap [10]. Only compute
emulation exists (WebRTX; EA Gigi with tinybvh).

**Real user — Bevy Solari** (inline ray queries only [11])
- 0.17 (Sep 2025): ReSTIR DI, ReSTIR GI, a world-space irradiance cache, DLSS-RR, plus a non-realtime reference
  path tracer [13].
- RTX 3080, 1600×900 upscaled to 3200×1800: DI 1.4–2.5 ms, GI 0.8–3.4 ms, DLSS-RR 5.8–6.3 ms, total 8.2–14.6 ms
  [11]; 0.18: 7.3–14.1 ms [12]. "NVIDIA only in practice" because of DLSS-RR [12].
- Metal support merged on Bevy main on 28 Jul 2026 (wgpu 30); no denoiser on Metal yet; GPU spans read 0 ms
  [14].
- Solari v8 (Sep 2026) turned ReSTIR off by default: "~twice as fast" on RTX 3080, less memory, fewer
  correlation artefacts, slightly worse small-shadow detail [15].

### 4.2 The compute alternative

**CWBVH** [16]: 8-wide BVH with 8-bit quantised child boxes, 80-byte nodes [17], octant-ordered traversal,
compressed stack (12 entries in shared memory), dynamic ray fetch, Woop's watertight triangle test. Titan X
Pascal (11 TFLOPS, 480 GB/s) at 2048²: 1.36×/2.36×/2.70× faster than Aila–Laine for primary, first-diffuse and
4th-bounce rays.

| Scene | Triangles | Primary | 1st diffuse | 4th bounce |
|---|---|---|---|---|
| Conference | 283k | 2642 Mrays/s | 1224 Mrays/s | 1093 Mrays/s |
| Powerplant | 12.8M | 322 Mrays/s | 271 Mrays/s | 172 Mrays/s |

Hierarchy memory 7.7–8.6 B per triangle, triangles excluded. Stackless traversal (Binder–Keller) is 1.24–1.32×
faster than the baseline in the same tests.

**tinybvh** [17]: binned SAH/SBVH builders; BVH2/BVH4/CWBVH GPU layouts; kernels in OpenCL, GL compute, DX12 and
Vulkan (no WGSL listed). Claims "up to 5 billion rays/s" in Sponza (262k triangles) on an RTX 5080 *laptop* with a
plain binary BVH and no RT cores; quotes EA Gigi's WebGPU path as "as fast as … DXR" without numbers.

**GPU builders** — H-PLOC [18], Radeon 7900 XT, full build into a 4-wide BVH:

| Builder | Hairball (2.9M tris) | Bistro (3.8M tris) | Notes |
|---|---|---|---|
| H-PLOC | 2.65 ms | 4.38 ms | |
| LBVH | 2.36 ms | 3.82 ms | SAH 10–24% worse |
| PLOC++ | 3.41 ms | 5.61 ms | |

For rigid bodies a one-off CPU build cached to disk suffices. No published WGSL traversal benchmark was found
[unverified].

### 4.3 Metal: M1 vs M3 and later

- **M1/M2:** the Metal intersector runs as GPU shader code and suffers from SIMD divergence [19]. A third party
  claims a general-purpose instruction helps ray-box tests [20, unverified].
- **M3+ (family 9):** fixed-function per-ray traversal plus reordering of intersection-function calls. Apple
  advises the intersector API; the intersection-query API "increases… scratch memory… and disables the reorder
  stage" [19]. Inference: wgpu's query-based path forgoes that reordering on M3+.
- **Apple's claims:** M3 renders "up to 2.5×" faster than M1 in pro apps (baseline M1 Max) [21]; M5's
  "third-generation ray-tracing engine" gives "up to 45%" more graphics in RT apps vs M4 [22].
- **Blender** enables MetalRT by default only on M3+ ("hardware for ray traversal") and keeps its own BVH2 on
  M1/M2 [24].
- **Blender Open Data 4.5.0** (medians, samples/min summed over scenes) [25]:

| Device | Median score |
|---|---|
| M1 Pro 16c | 483 (n=44) |
| M2 Pro 19c | 902 |
| M3 Pro 18c | 1756 |
| M4 Pro 20c | 2568 |
| M5 Pro 20c | 3673 (n=3) |
| RTX 5080 (OptiX) | 9138 (n=1431) |
| RTX 4090 (OptiX) | 11062 |

  RTX 5080 ≈ 19× the M1 Pro; M3 Pro ≈ 3.6×.
- **MoltenVK:** no ray tracing shipped (issue #427 open since 2018; opt-in WIP PR #2771 still open as of
  6 Oct 2026) [26]. Metal through wgpu is the only route on the Mac.

### 4.4 Ray throughput ballparks

Assumptions: a 3M-triangle mesh, 1–2 instances, minimal shading. Values in Grays/s.

| Device / method | Coherent shadow (first hit) | Primary | Incoherent diffuse | Provenance |
|---|---|---|---|---|
| RTX 5080, hardware RT | 5–15 | 3–10 | 1–4 | [extrapolation] from the anchors below [27][28][29] |
| RTX 5080, WGSL compute | 2–6 | 1.5–5 | 0.5–2 | measured anchors [17] (5 Grays/s, laptop, 262k tris) and [16], scaled [extrapolation] |
| M1 Pro, Metal intersector or WGSL compute | 0.2–0.6 | 0.1–0.5 | 0.03–0.15 | [extrapolation] from ~10 TFLOPS per Gray/s [27] at ≈5.2 TFLOPS (derived), 200 GB/s [23], and the ×19 Blender gap [25] |
| M3/M4 Pro, hardware RT | ~1–3 | — | — | M1 Pro × 3.6–5.3 from Blender [25] [extrapolation] |

Anchors: RTX 2080 Ti 10 Grays/s, Pascal in software 1.1 Grays/s, "~10 TFLOPS/Giga Ray" [27]; no official
gigarays figure since Turing, a user reports "10+" Grays/s on an RTX 4090 [28]; RTX 5080: 170.6 RT TFLOPS (RTX
4080: 112.7), 56.3 FP32 TFLOPS, 960 GB/s, twice Ada's ray-triangle rate [29].

What that means for kalast [extrapolation]:

| Workload | RTX 5080 | M1 Pro |
|---|---|---|
| One ray per pixel at 1440p | 0.25–0.75 ms | 6–18 ms |
| 4K image, 1024 disc samples/px (8.5 G rays) | 0.6–1.7 s | 15–45 s |
| TPM: 2×3.1M facets × 256 rays, per step | 0.1–0.3 s | 3–8 s |
| 4K, 1024 spp × 3 bounces (~51 G rays) | 15–50 s | 6–28 min |

**Correctness.** DXR requires watertight intersection: "gaps between triangles sharing edges must never appear"
across the f32 range within a BLAS; a top-left rule prevents double hits; results are deterministic on the same
device and driver [30]. Vulkan: "should", "expected and tested"; advises keeping geometry near the origin and
offsetting `t_min` [31]. Metal: no published guarantee found [unverified]. BLAS compaction saves "at least 50%"
in some games [32]. A compute CWBVH for 3.1M triangles is about 140–180 MB including triangles [estimate].

### 4.5 Bottom line

- **Usable now** for an opt-in progressive reference mode on both machines.
- **kalast's case avoids the gaps:** BLAS built once per rigid body and compacted, a 2-instance TLAS rebuilt each
  frame; missing refit, RT pipelines and Metal vertex return do not matter.
- **Risks:** experimental API churn; a young Metal path with no CI coverage.
- **Not a 240 fps tool on the M1 Pro.** On the RTX 5080, 1–4 rays per pixel fit in a few ms, so a hybrid of
  shadow maps plus ray-traced penumbrae is plausible there.
- **Fallback:** a WGSL compute traversal (BVH4 or CWBVH with the Woop test) behind the same interface for setups
  without `EXPERIMENTAL_RAY_QUERY`; on M1 it should be roughly on par with Metal's software intersector
  [inference from Blender's choice].

### Takeaways for kalast

- A ray-query reference mode works today on both the M1 Pro and the RTX 5080. Treat it as a progressive offline
  renderer: seconds on the 5080, tens of seconds to minutes on the M1 Pro.
- On Windows, enable wgpu's `static-dxc` feature or ship `dxcompiler.dll`; otherwise DX12 silently reports no
  ray queries. The Vulkan backend is unaffected.
- Full-resolution BLAS (no LOD cut), compacted. On Metal, keep BLAS and TLAS builds in separate submissions
  until #9215 lands, and budget for the 31 buffer slots.
- Exactness comes from sampling, not from the hardware: a stratified, importance-sampled limb-darkened disc with
  fixed seeds; body-local instance transforms and a `t_min` offset because positions are f32.
- The same code can produce per-facet disc fractions for the TPM, a validation reference for the shadow-map
  path.
- Time ray-tracing passes on the Mac by CPU wall-clock after `device.poll` if #9414 affects kalast's timestamps.

### Sources (section 4)

1. wgpu CHANGELOG.md (trunk, read 2026-10-09) — https://github.com/gfx-rs/wgpu/blob/trunk/CHANGELOG.md
2. wgpu/wgpu-types/wgpu-hal/naga 30.0.1 sources (crates.io, read locally) — https://crates.io/crates/wgpu/30.0.1
3. wgpu Ray Tracing Tracking Issue #6762 — https://github.com/gfx-rs/wgpu/issues/6762
4. wgpu ray-tracing documentation module — https://github.com/gfx-rs/wgpu/blob/trunk/wgpu/src/documentation/extensions/ray_tracing.rs
5. wgpu `ray_shadows` example — https://github.com/gfx-rs/wgpu/blob/trunk/examples/features/src/ray_shadows/shader.wgsl
6. wgpu PR #8071 — https://github.com/gfx-rs/wgpu/pull/8071
7. wgpu issues #9215, #9100, #8825, #9946, #8204 — https://github.com/gfx-rs/wgpu/issues/9215 (and siblings)
8. wgpu issue #9414 — https://github.com/gfx-rs/wgpu/issues/9414
9. gpuweb issue #535 — https://github.com/gpuweb/gpuweb/issues/535
10. Chrome, "What's next for WebGPU" (2024) — https://developer.chrome.com/blog/next-for-webgpu
11. JMS55, Solari in Bevy 0.17 (2025) — https://jms55.github.io/posts/2025-09-20-solari-bevy-0-17
12. JMS55, Solari in Bevy 0.18 (2025) — https://jms55.github.io/posts/2025-12-27-solari-bevy-0-18
13. Bevy 0.17 release notes — https://bevy.org/news/bevy-0-17/
14. Bevy PR #25123 — https://github.com/bevyengine/bevy/pull/25123
15. Bevy PR #25513 — https://github.com/bevyengine/bevy/pull/25513
16. Ylitie, Karras, Laine, "Efficient Incoherent Ray Traversal on GPUs Through Compressed Wide BVHs", HPG 2017 — https://users.aalto.fi/~laines9/publications/ylitie2017hpg_paper.pdf
17. tinybvh README — https://github.com/jbikker/tinybvh
18. Benthin et al., H-PLOC, HPG 2024 — https://gpuopen.com/download/publications/HPLOC.pdf
19. Apple tech talk 111375 (2023) — https://developer.apple.com/videos/play/tech-talks/111375/
20. P. Turner, metal-benchmarks (third party) — https://github.com/philipturner/metal-benchmarks
21. Apple Newsroom, M3 (2023) — https://www.apple.com/newsroom/2023/10/apple-unveils-m3-m3-pro-and-m3-max-the-most-advanced-chips-for-a-personal-computer/
22. Apple Newsroom, M5 (2025) — https://www.apple.com/newsroom/2025/10/apple-unleashes-m5-the-next-big-leap-in-ai-performance-for-apple-silicon/
23. Apple Newsroom, M1 Pro/Max (2021) — https://www.apple.com/newsroom/2021/10/introducing-m1-pro-and-m1-max-the-most-powerful-chips-apple-has-ever-built/
24. Blender PRs #114296, #120299 — https://projects.blender.org/blender/blender/pulls/114296 ; https://projects.blender.org/blender/blender/pulls/120299
25. Blender Open Data (4.5.0, queried 2026-10-09) — https://opendata.blender.org/ ; https://opendata.blender.org/about/
26. MoltenVK #427 and PR #2771 — https://github.com/KhronosGroup/MoltenVK/issues/427 ; https://github.com/KhronosGroup/MoltenVK/pull/2771
27. NVIDIA, Turing In-Depth (2018) — https://developer.nvidia.com/blog/nvidia-turing-architecture-in-depth/
28. NVIDIA forum, RTX 4090 gigarays (Jan 2026) — https://forums.developer.nvidia.com/t/what-is-the-performance-of-the-rtx-4090-measured-in-gigarays/358704
29. NVIDIA RTX Blackwell whitepaper (2025) — https://images.nvidia.com/aem-dam/Solutions/geforce/blackwell/nvidia-rtx-blackwell-gpu-architecture.pdf
30. DXR Functional Spec — https://microsoft.github.io/DirectX-Specs/d3d/Raytracing.html
31. Vulkan spec, Ray Traversal — https://docs.vulkan.org/spec/latest/chapters/raytraversal.html
32. NVIDIA, Tips: AS Compaction — https://developer.nvidia.com/blog/tips-acceleration-structure-compaction/
33. Local Metal probe in this session (M1 Pro, macOS 26.4.1): `supportsRaytracing` = true, `supportsRaytracingFromRender` = true, Apple7 = true, Apple8/9 = false.

---

## 5. Global illumination and denoising for reference images

Lines marked *(derived)* are arithmetic done for this survey, not a sourced figure; [assessment] marks
judgement.

### 5.1 Direct sunlight: next-event estimation (NEE) to a limb-darkened disc

- **Geometry.** The Sun's angular radius is 0.2666° at 1 au and 0.175° at Mars. Disc solid angle
  Ω = 2π(1−cosθ) ≈ 6.8×10⁻⁵ sr at 1 au *(derived)*.
- **How pbrt-v4 samples it.** A sphere light seen from outside is sampled uniformly inside its cone, pdf =
  1/(2π(1−cosθmax)). When sin²θmax < sin²(1.5°) pbrt switches to a Taylor form (1−cosθmax ≈ sin²θmax/2) to
  keep precision [1].
- **Why this matters in f32 WGSL.** 1−cos(0.2666°) = 1.08×10⁻⁵ while f32 spacing just below 1.0 is 6×10⁻⁸:
  the naive formula carries roughly 0.3–1% error in the pdf, and sampling cosθ uniformly in f32 leaves only
  ~180 distinct values across the disc; the sin² form avoids both *(derived)*.
- Cycles models its sun lamp as a cone and uses a low-distortion concentric map so 2D stratification survives
  [2].
- **Limb-darkening data.** Neckel & Labs (1994): 5th-order polynomials in μ at 30 continuum wavelengths,
  303–1099 nm [3]. The Hošek–Wilkie solar-radiance function used in rendering has a limb-darkened disc and needs
  samples across the disc, not at its centre [4].
- **Sampling recipe.** Importance-sample the disc by inverting the 1D radial CDF of I(μ(r))·r, tabulated per
  band. For an unoccluded receiver the estimator then has zero variance up to the ~10⁻⁵ variation of
  BRDF·cosθ across 0.27°: one sample gives the exact disc-integrated irradiance, and variance exists only
  inside penumbrae *(derived)*.
- **Penumbra convergence.** Independent samples: σ = √(f(1−f)/N) ≤ 0.5/√N, i.e. 6.3% at 64 samples, 1.6% at
  1024 *(derived)*. Stratified (jittered) sampling makes variance fall as N⁻² in smooth regions and N⁻¹·⁵ where
  an edge crosses the domain [5]; a penumbra is the edge case, so error falls roughly as N⁻⁰·⁷⁵. pbrt-v4's
  default is a blue-noise Owen-scrambled Sobol' sampler; Owen scrambling "gives an even better rate of
  convergence, especially at power-of-two numbers of sample points"; in pbrt-v4's test scene Sobol' and Halton
  gave about 10% lower MSE than independent sampling, scene-dependent [6].
- **MIS** with the balance heuristic [7] collapses to pure NEE here: a cosine-sampled diffuse ray hits a
  6.8×10⁻⁵ sr Sun with probability ≤ 2×10⁻⁵ *(derived)*. MIS matters only for specular materials.

### 5.2 Indirect light into permanently shadowed craters: where the variance is

- **The light source.** PSRs receive only secondary light reflected from sunlit topography [8]. ShadowCam
  notes that in a simple bowl "more than a third of the rim can be illuminated": an extended arc-shaped
  source spanning about 120°. The ShadowCam team's simulations use 60 m LOLA topography [9].
- **How many bounces matter** *(derived from the spherical-bowl geometry of Ingersoll et al. [10])*. Any two
  elements of a sphere's inner surface exchange with view factor dA/(4πR²), so every bowl element sees the
  fraction f = (h/r)²/(1+(h/r)²) of its hemisphere filled by crater: f ≈ 0.14 for depth/diameter 0.2. With
  visible albedo ≈ 0.12 the second bounce carries about 1.7% of the first. Single scattering dominates visible
  PSR light, consistent with Mazarico et al., who added singly-scattered light to their LOLA horizon model and
  found every PSR receives some scattered light during the year [11]. IR self-heating is where the full
  all-orders balance matters [10].
- **Monte Carlo noise** *(derived)*. A cosine-sampled ray from a PSR floor hits a sunlit wall with probability
  p (that wall's view factor); relative error √((1−p)/(Np)): with p = 0.05, 13.6% at 1024 paths per pixel,
  about 1.9×10⁵ paths per pixel for 1%.
- **Remedies.** (a) Treat sunlit facets as emitters carrying their radiosity and do NEE to them through a
  light BVH [12]; this is ReSTIR DI's home case (3.4M dynamic emissive triangles in under 50 ms per frame with
  at most 8 rays per pixel [13]). (b) Path guiding with an SD-tree [14]. (c) A deterministic final gather over
  precomputed facet radiosity (§5.5).

### 5.3 ReSTIR: what it buys for one Sun plus bounces

- **ReSTIR DI** [13]: equal error 6–60× faster unbiased, 35–65× biased. With a single small Sun it adds
  nothing over §5.1's NEE; it pays only when sunlit facets are the lights.
- **ReSTIR GI** [15] reuses multi-bounce paths across pixels and frames: MSE improvements of 9.3–166× at 1 spp.
- **GRIS / ReSTIR PT** [16]: "sample reuse introduces correlation, ReSTIR-style iterative reuse loses most
  convergence guarantees that RIS theoretically provides"; GRIS restores the conditions for unbiasedness.
- **ReSTIR PT Enhanced** (I3D 2026, Best Paper): 2–3× faster, adds "duplication maps" against spatiotemporal
  correlation [17]. Primer: SIGGRAPH 2023 "A Gentle Introduction to ReSTIR" course [18].
- [assessment] For reference images, even unbiased ReSTIR leaves correlated, blotchy error that does not fall
  as 1/√N, and per-pixel variance estimates lose meaning; biased variants converge to the wrong answer. ReSTIR
  suits the interactive preview, not the error-free export.

### 5.4 Denoisers

| Denoiser | Method | Platforms | Cost | Notes |
|---|---|---|---|---|
| **SVGF** [19] | Temporal accumulation + variance-guided à-trous wavelet filter | Shader-only, portable to WGSL [assessment] | 1 spp → 1080p in 10 ms (±15%) on 2017 hardware | Lag and ghosting; A-SVGF fixes these with sparse temporal gradients [20] |
| **NRD** [21] | ReBLUR (diffuse/specular), ReLAX (à-trous, built for RTXDI), SIGMA (shadow-only), REFERENCE (plain accumulation) | HLSL → DXBC/DXIL/SPIR-V via NRI on D3D11/12/Vulkan; **no Metal, no WebGPU** | RTX 4080, 1440p: ReBLUR diff+spec 2.55 ms, ReLAX 3.25 ms, SIGMA shadow 0.40 ms | Latest release v4.17.3 (30 Apr 2026) |
| **OIDN** [22] | CNN | CPU (x86, ARM64); CUDA (Turing–Blackwell); HIP (RDNA2–4); SYCL (Xe–Xe3); **Metal on Apple M1+, macOS 13+** | Quality modes high / balanced / fast (fast 1.5–2× faster); no official per-frame timings found [unverified] | v2.5.1 (18 Aug 2026); docs warn of "small numerical differences" between devices; Rust crate `oidn` 2.5.1 |
| **OptiX** [23] | AI denoiser trained on "tens of thousands of images rendered from one thousand 3D scenes" | NVIDIA GPUs only (CUDA) | — | HDR, AOV, temporal and 2× upscale models |

- **Bias.** Every reconstruction filter trades variance for bias [24].
- **Planetary evidence of that bias.** HORUS, a learned denoiser applied to LROC NAC images of PSRs [25], added
  no false features but removed features below its resolution limit, did not reliably resolve craters under
  ~8.5 m, and made boulders ~0.9 m (~11%) larger than in sunlit images.
- [assessment] Denoisers are for previews only; exported radiometric images should be the raw accumulation.

### 5.5 Caching and radiosity on the facet mesh

- **Irradiance caching** (Ward 1988): sparse evaluation of indirect irradiance, interpolation, record density
  from an error estimate, reuse across views [26]. Per-facet radiosity is the mesh-bound limit of the idea.
- **AO baking** gives unweighted visibility; on an airless body its complement is the sky view factor (useful
  for IR cooling to space) but it does not describe PSR light, which depends on which walls are sunlit
  [assessment].
- **Radiosity.** B = E + ρFB, solved as a Neumann series; Jacobi iteration "will typically converge to machine
  precision in O(1) iterations" [28]. Each bounce shrinks by ρ·maxᵢΣⱼFᵢⱼ, about 0.02 in the bowl above.
- **Memory** *(derived)*. A dense float32 view-factor matrix is 40 GB at 10⁵ facets, 4 TB at 10⁶, 38 TB at
  3.1M. Sparse or hierarchical storage is mandatory [27].
- **Potter et al. 2023** compress the matrix with quadtree/octree blocks and sparse SVD [28]. Assembly O(N²);
  storage and mat-vec about O(N). At N ≈ 33k: sparse ≈ 500 MB vs compressed ≈ 50 MB at ε = 10⁻² (read off
  their Fig. 3). Accuracy 1–2% relative ℓ₂ temperature error against the analytic crater; De Gerlache
  discrepancies well below 1 K. Largest test: 197k facets (comet 67P); visibility by CGAL AABB-tree ray
  casting.
- **Lagged scattering in time-stepping models.** Schörghofer's Topo3D multiplies a sparse view-factor matrix
  once per time step using the previous step's reflected flux, so each step adds one scattering order; it also
  has a truncated-SVD path [29]. MoCSI does the same ("each multiplication of the viewfactor matrix corresponds
  to one order of light scattering"), computes view factors by contour integrals (5-point Gauss–Legendre),
  rejects Monte Carlo as "prohibitively computationally expensive", and has not yet implemented Potter's fast
  algorithm [30].

### 5.6 Planetary-science practice and validation

- **Direct sunlight via horizon methods.**
  - Mazarico et al. 2011 [11]: 240 m LOLA DEMs and horizon profiles, the Sun a "finite disc"; validated against
    LRO WAC images; PSR areas 12,866 km² (north) and 16,055 km² (south).
  - Gläser et al. 2014 [31]: 20 m/px LOLA DTM cross-validated with NAC images; best sites lit 92.27% of the time
    at 2 m height, 95.65% at 10 m.
  - Topo3D [29] stores horizons on 180 azimuths, but its `flux_wshad` routine tests a point Sun against the
    horizon (binary, no disc).
- **Scattered light and IR in PSRs.**
  - Ingersoll et al. 1992 [10]: analytic Lambertian bowl, all scattering orders, the same temperature at every
    shadowed point. Benchmarks against it: MoCSI's 14,198-facet crater, residuals ≲1 K in shadow and ≲0.1 K in
    sunlight at 15° incidence [30]; Potter et al. 1–2% [28].
  - Paige et al. 2010 mapped Diviner cold traps, including 38 K at the LCROSS site [32]. A Paige-group
    ray-tracing plus radiosity model at 2 km resolution appears only in a search summary [unverified].
  - Hayne et al. 2021 combined analytic bowls with ray-traced Gaussian rough surfaces: about 40,000 km² of cold
    traps, 10–20% of it in micro cold traps [33].
- **Imaging PSRs.**
  - ShadowCam: over 200× more sensitive than NAC, 1.7 m/px [9].
  - Mahanti et al. synthesise PSR images with a **view-factor-based** secondary-illumination model (2022) [34],
    compared it with ShadowCam images of Shackleton (2023) [8], and tracked secondary light at Artemis III
    sites (2024) [35]. In 2026 they showed a Lommel–Seeliger-scaled model and a Lambert model both reproduce
    the detail of Shoemaker's PSR, the Lommel–Seeliger model "dramatically more accurate in scale" [36].
  - Martin et al. 2024: features of uniform albedo vary strongly in secondary light, depending on the median
    secondary phase angle [37].
  - Jia et al. 2024 inverted ShadowCam images with view-factor secondary light (shape from shading) and matched
    LOLA [38].
  - Kloos et al. 2021: scattered sunlight dominates in the visible and IR; equator-facing PSR slopes receive
    40–60% of the energy of pole-facing ones [39].
  - HORUS ray-traced a DEM to choose training images with matching incidence; PSR SNR ~1.4–2.4 [25].
- **Asteroid TPMs.** Rozitis & Green's ATPM includes multiple scattering of sunlight, global self-heating and
  rough-surface beaming on facet shape models [40]. MoCSI (A&A 2026) is open source and was applied to Ryugu
  [30].

### Takeaways for kalast

- **Direct light.** NEE over the limb-darkened disc with stratified or Owen-Sobol' samples, sin² form in f32.
  Away from penumbrae this is essentially exact with one sample; the same sampler gives the TPM exact,
  deterministic per-facet shadow fractions.
- **Bounces.** Reuse the TPM's view factors: Jacobi sweeps, or one order per TPM step as Topo3D and MoCSI do,
  give noise-free multi-bounce irradiance. With ρf ≈ 0.02, two or three visible orders suffice; the IR balance
  needs the full solve.
- **Validate first** against the Ingersoll bowl (others' bar: ≲1 K in shadow for MoCSI, 1–2% for Potter)
  before trusting PSR images.
- **Photometry.** The scattering law at each bounce vertex matters more than the bounce count: Lambert gets the
  pattern, Lommel–Seeliger the scale [36].
- **Pixel-exact PSR images.** Path-trace with NEE to the Sun plus NEE to sunlit facets as emitters, or
  final-gather over facet radiosity; naive path tracing needs about 10⁵ paths per pixel for 1%.
- **Error-free export.** No denoiser, no ReSTIR reuse; accumulate independent samples with a convergence
  metric. OIDN (Metal on M1+, CUDA on the RTX 5080) fits the preview; NRD has no Metal path.

### Sources (section 5)

DOIs were checked against Crossref by the researcher.

1. pbrt-v4, "Spheres" (sphere sampling). https://pbr-book.org/4ed/Shapes/Spheres
2. Blender Cycles PR #108996 (sun lamp as a cone). https://projects.blender.org/blender/blender/pulls/108996/files
3. Neckel & Labs, Sol. Phys. 153:91–114 (1994). doi:10.1007/BF00712494
4. Hošek & Wilkie, IEEE CG&A 33:44–52 (2013). doi:10.1109/MCG.2013.18; https://cgg.mff.cuni.cz/projects/SkylightModelling/
5. Mitchell, SIGGRAPH 1996:277–280. doi:10.1145/237170.237265; https://people.csail.mit.edu/ericchan/bib/pdf/p277-mitchell.pdf
6. pbrt-v4, "Sobol' Samplers". https://pbr-book.org/4ed/Sampling_and_Reconstruction/Sobol_Samplers
7. Veach & Guibas, SIGGRAPH 1995:419–428. doi:10.1145/218380.218498
8. Mahanti et al., JASS 40:131–148 (2023), doi:10.5140/JASS.2023.40.4.131; IGARSS 2023, doi:10.1109/IGARSS52108.2023.10282497
9. ShadowCam, "Complicated Lighting" and About pages. https://shadowcam.im-ldi.com/images/1357; https://www.shadowcam.asu.edu/about
10. Ingersoll, Svitek & Murray, Icarus 100:40–47 (1992). doi:10.1016/0019-1035(92)90016-Z
11. Mazarico et al., Icarus 211:1066–1081 (2011). doi:10.1016/j.icarus.2010.10.030; https://ntrs.nasa.gov/citations/20120010094; https://pgda.gsfc.nasa.gov/products/69
12. Conty Estevez & Kulla, PACMCGIT 1 (2018). doi:10.1145/3233305
13. Bitterli et al., ACM TOG 39 (2020). doi:10.1145/3386569.3392481
14. Müller, Gross & Novák, CGF 36:91–100 (2017). doi:10.1111/cgf.13227
15. Ouyang et al., CGF 40:17–29 (2021). doi:10.1111/cgf.14378
16. Lin et al., ACM TOG 41 (2022). https://research.nvidia.com/publication/2022-07_generalized-resampled-importance-sampling-foundations-restir
17. Lin, Kettunen & Wyman, ReSTIR PT Enhanced, I3D 2026. https://research.nvidia.com/labs/rtr/publication/lin2026restirptenhanced/
18. Wyman et al., SIGGRAPH 2023 course. https://intro-to-restir.cwyman.org/
19. Schied et al., HPG 2017. doi:10.1145/3105762.3105770; https://cg.ivd.kit.edu/english/svgf.php
20. Schied, Peters & Dachsbacher, PACMCGIT 1 (2018). doi:10.1145/3233301
21. NVIDIA NRD. https://github.com/NVIDIA-RTX/NRD
22. Intel OIDN docs, README and releases. https://www.openimagedenoise.org/documentation.html; https://github.com/OpenImageDenoise/oidn; Rust bindings https://github.com/Twinklebear/oidn-rs
23. NVIDIA OptiX denoiser. https://developer.nvidia.com/optix-denoiser
24. Zwicker et al., CGF 34:667–681 (2015). doi:10.1111/cgf.12592
25. Bickel et al., Nat. Commun. 12 (2021). doi:10.1038/s41467-021-25882-z; https://pmc.ncbi.nlm.nih.gov/articles/PMC8460740
26. Ward, Rubinstein & Clear, SIGGRAPH 1988. doi:10.1145/378456.378490
27. Hanrahan, Salzman & Aupperle, SIGGRAPH 1991. doi:10.1145/127719.122740
28. Potter et al., J. Comput. Phys. X 17:100130 (2023). doi:10.1016/j.jcpx.2023.100130; https://arxiv.org/abs/2209.07632
29. Schörghofer, Planetary-Code-Collection, Topo3D (`shadow_subs.f90`, `topo3d_subs.f90`, `Common/flux_noatm.f90`). https://github.com/nschorgh/Planetary-Code-Collection
30. Schuckart et al., A&A 706:A104 (2026), doi:10.1051/0004-6361/202557178; MoCSI docs https://mocsi.readthedocs.io/en/stable/mocsi_overview/validation.html and https://mocsi.readthedocs.io/en/v1.0.3/mocsi_simulation_examples/mocsi_simulation_examples.html
31. Gläser et al., Icarus 243:78–90 (2014). doi:10.1016/j.icarus.2014.08.013
32. Paige et al., Science 330:479–482 (2010). doi:10.1126/science.1187726
33. Hayne, Aharonson & Schörghofer, Nat. Astron. 5:169–175 (2021). doi:10.1038/s41550-020-1198-9
34. Mahanti et al., IEEE GRSL 19 (2022). doi:10.1109/LGRS.2022.3166809
35. Mahanti et al., PSJ 5:62 (2024). doi:10.3847/PSJ/ad1b50
36. Mahanti et al., IEEE GRSL 23 (2026). doi:10.1109/LGRS.2025.3642808
37. Martin et al., PSJ 5:207 (2024). doi:10.3847/PSJ/ad6005
38. Jia, Wu & Mahanti, ISPRS Archives XLVIII-3 (2024). https://isprs-archives.copernicus.org/articles/XLVIII-3-2024/245/2024/
39. Kloos et al., Acta Astronautica 178:432–451 (2021). doi:10.1016/j.actaastro.2020.09.012
40. Rozitis & Green, MNRAS 415:2042–2062 (2011). doi:10.1111/j.1365-2966.2011.18718.x

---

## 6. Performance at 240+ fps with ~6M triangles

### 6.1 The arithmetic

- At 240 fps the whole frame gets 4.17 ms of GPU time. Drawing 2 × 3.1M = 6.2M triangles once is 1.49 G tri/s. Every further full-geometry pass (a per-body Sun layer, the near layer) costs as much again. Two body layers, a near layer and the main view, all at full resolution, come to about 25M triangles per frame, or 6 G tri/s.
- **Measured hardware raster rates on dense meshes.** These come from CuRast (Schütz, Lipp, Kristmann, Wimmer, arXiv, April 2026), Table 2, at 3840×2160 [3]:
  - Vulkan indexed draws of a 3-billion-triangle instanced scene took 141.7 ms on an RTX 4090 and 125.7 ms on an RTX 5090. That is about 21–24 G tri/s.
  - Their CUDA software rasteriser drew the same scene in 9.6 ms on the 5090, about 310 G tri/s instanced. It drew 400M unique triangles in 5.25 ms, about 76 G tri/s.
  - On low-poly content the hardware is an order of magnitude faster. For Sponza (262k triangles) on the 5090, Vulkan took 0.018 ms and CuRast 0.239 ms.
- **The RTX 5080 has fewer raster units.** It is GB203 with 7 GPCs and 84 SMs, against 11 GPCs and 170 SMs on the 5090 [4]. If triangle setup scales with GPC count, a 5080 rasterises about 15 G tri/s [speculation]. That makes the 25M-triangle frame about 1.6 ms, roughly 40 % of the 240 fps budget, before any shading, PCF, penumbra walk, TPM or UI. I found no published or measured primitive rate for the M1 Pro; it has to be measured with timestamp queries [gap].
- **Why level of detail should aim at about one triangle per pixel.** Rasterisers are "highly parallel in pixels not triangles", and for tiny triangles "binning triangles is as much work as just writing the final pixels" [2].
  - The UE4 path would have rasterised more than 1 billion triangles for Nanite's demo frame. Nanite rasterises about 25M, a figure that stays roughly constant through the demo, in about 2.5 ms on PS5 at about 1404p [2].
  - For kalast: a 1440p frame has 3.7M pixels. A body covering a quarter of it gets about 0.9M pixels for its 3.1M facets, which is 3 or more triangles per pixel.
  - Nanite applies the same rule to shadows. It chooses shadow LOD at about one shadow texel of error, plus a 2-pixel LOD bias for shadows, so that "shadow cost scales with resolution, not scene complexity" [2].

### 6.2 GPU-driven rendering: techniques and measured gains

- **Clusters with GPU culling** (Haar & Aaltonen, SIGGRAPH 2015) [1]:
  - Meshes are split into 64-triangle clusters. The pipeline runs instance culling, then cluster expansion, then cluster culling (frustum, occlusion and triangle backface), then index compaction.
  - Precomputed 64-bit backface masks per cluster culled 10–30 % of triangles.
  - In Assassin's Creed Unity this gave 1–2 orders of magnitude fewer draw calls. The GPU culled 20–40 % of triangles, but that was "only small overall gain: <10% of geometry rendering". In shadow passes, 30–80 % of triangles were culled.
  - Occluder depth: the 300 best occluders cost about 600 µs, the downsample to 512×256 about 100 µs, and the result is combined with a reprojection of the last frame's depth.
- **Two-phase hierarchical-Z occlusion culling** [1]:
  - The method culls against the last frame's depth pyramid, draws what survives, rebuilds the pyramid, re-tests what was rejected, and draws the false negatives.
  - RedLynx's torture test had 250k moving objects on Xbox One at 1080p. It cost 2.3 ms of GPU time in total (object culling 0.28 + 0.26 ms, cluster culling 0.09 + 0.04 ms, drawing 1.60 ms, pyramid 0.06 ms) and 0.2 ms of CPU time.
  - Thanks to index compaction it used "only two DrawInstancedIndirect calls", with no ExecuteIndirect or MultiDrawIndirect.
- **Compute triangle culling** (Wihlidal, GDC 2016) uses compute shaders to filter triangles before the fixed-function pipeline, to go "beyond the limits of the fixed function hardware" [5]. Its numbers are behind the GDC Vault paywall and are not verified here.
- **Cluster cone culling versus per-triangle backface culling** (Kapoulkine, 2023) [6][7]:
  - Cone culling removes 25–28 % of triangles on dense scanned meshes (Kitten, Happy Buddha), but only 4–8 % on architectural scenes (Sponza, Lumberyard Interior).
  - Per-triangle culling reaches 50–53 %.
  - On an RTX 4070 Ti, kittens scene with LOD: 7.25 ms with cone culling, 7.63 ms with per-triangle culling, 6.96 ms with both.
  - Cluster size matters. Occlusion-culling efficiency is about 80 % at 64 triangles and about 66 % at 256. Cone-culling efficiency is 14 % at 64 and 3 % at 256. NVIDIA's guidance is 64 vertices with 84 or 126 triangles.
- **Visibility buffer**:
  - Burns & Hunt (2013) store a triangle and instance ID, 4 B per sample, instead of a G-buffer of 20 B or more per sample. They measured "little to no net benefit" at about 2M samples (1080p, no MSAA). The gain grows with MSAA sample count, especially on bandwidth-limited integrated GPUs [8].
  - The Forge's Triangle Visibility Buffer 2.0 (release 1.57, May 2024) "doesn't use draw calls anymore": two compute passes filter triangles and fill depth and visibility. It runs on D3D12, PS4/5, Xbox and macOS/iOS. The release notes publish no performance numbers [9].
- **Software rasterisation of tiny triangles**:
  - Nanite's compute rasteriser is "3x faster than hardware on average" compared with its fastest primitive-shader path [2].
  - Bevy sends clusters smaller than 64 px on both axes to a software rasteriser [10].
  - CuRast's figures are in 6.1.

### 6.3 What wgpu allows (wgpu 30.0.1, checked in the crate source [14])

- **One queue.** `Adapter::request_device` returns a single `(Device, Queue)`, so there is no async compute and no copy queue.
  - NVIDIA recommends pairing shadow-map rasterisation, which is "graphics-pipe dominated (CROP, PROP, ZROP, VPC, RASTER…)", with math-limited compute on an async queue [21].
  - In wgpu, shadow work therefore runs back to back with everything else, including the TPM.
- **Multi-draw indirect.**
  - `MULTI_DRAW_INDIRECT_COUNT` is available on DX12 and Vulkan only. On Metal, `multi_draw_indirect` is emulated as a series of `draw_indirect` calls.
  - Bevy 0.16's GPU-driven path (GPU transforms, multi-draw and bindless) is complete only on Vulkan. DX12, Metal and WebGPU get GPU transforms only, and Metal's bindless limits are "significantly lower" [11].
  - The portable workaround comes from [1]: compact the surviving clusters' indices into one buffer and issue a single `draw_indexed_indirect`.
- **Mesh shaders.** `EXPERIMENTAL_MESH_SHADER` exists on Vulkan, DX12 and Metal, but WGSL works only on Vulkan. Elsewhere it needs passthrough MSL or HLSL shaders.
- **64-bit atomics.**
  - On Metal, `SHADER_INT64_ATOMIC_MIN_MAX` needs Apple9, or Apple8 plus Mac2. `TEXTURE_INT64_ATOMIC` needs Apple9 and MSL 3.1.
  - The M1 Pro is Apple7 (probed on this machine), so it has neither. A Nanite-style software rasteriser with a 64-bit visibility buffer cannot run on it.
- **Bevy virtual geometry, the reference wgpu/WGSL implementation** [10][12][13]:
  - Design: meshlets of 255 vertices and 128 triangles, an LOD DAG, two-pass occlusion culling, and a 64-bit visibility buffer `(depth << 32) | cluster(25 bit) | triangle(7 bit)` written with atomics. A split routes small clusters to software rasterisation and the rest to hardware.
  - Bevy 0.15, RTX 3080 at 2240×1260, 3375 bunnies: the visibility buffer went from 4.97 ms to 0.93 ms.
  - Bevy 0.17 added BVH culling. 130,000 dragons render in about 3.5 ms on an RTX 4070 (3.1 ms geometry, 0.4 ms materials). More than 1M instances take about 4.5 ms. A 1,300-instance scene went from 2.2 ms to 1.3 ms.
  - The current docs say the plugin "requires … `TEXTURE_INT64_ATOMIC`", "works only on the Vulkan and Metal backends", and is incompatible with MSAA. It is still under `experimental`.
  - In practice: usable on the RTX 5080 through the Vulkan backend (not DX12) and on M3 or later Macs, not on the M1 Pro.

### 6.4 Shadow caching when the light moves slowly

- **Unreal Virtual Shadow Maps** [15]:
  - 16k² virtual maps split into 128² pages, cached from frame to frame. Static and dynamic geometry are kept as "two copies of depth".
  - However, "any light movement or rotation will invalidate all cached pages for that light".
  - With caching, the only regions updated each frame are "where objects are moving or edges of the frustum as the camera moves" [2].
  - Fortnite on PS5 and Xbox Series X|S turns Nanite, Lumen, VSM and TSR on only when "120 FPS Mode" is off [22].
- **Insomniac: CSM caching and scrolling** (SIGGRAPH 2012) [16]:
  - Static geometry is cached in a map. When the camera moves, the cache is scrolled laterally (a UV shift) and in depth (all depths offset). Only the slabs exposed by the scroll are rendered, and dynamic geometry is added on top.
  - It assumes "the light direction and shape is relatively stable across frames".
  - Result: about 70 % of static geometry is not re-rendered (PS3/360, 512² maps).
- **CryEngine cached shadows** [17]: cascades from a chosen index onward are rendered once and kept. They re-centre and update when the camera nears their border. The update mode can be none, once or continuous. They use about 130 MB of VRAM.
- **Cyberpunk 2077** (SIGGRAPH 2021 talk) [18]:
  - Up to 4 cascades rendered every frame, plus 4 "distant" levels whose rendering is "spread over multiple frames", so they are "refreshed every few seconds". The last level covers the 256 km² map.
  - Caster visibility is baked offline for 12 sun positions (every 2 h), which removed "millions" of shadow triangles.
  - Also: 8-plane frustum-slice culling, shadow-view occlusion culling, and throttled local-shadow refreshes with static/dynamic slices.
- **DOOM (2016)** [19] uses an 8k×8k atlas and regenerates "only the depth maps which need to be updated". The static part is cached and dynamic meshes are composited onto it.
- **Flax** [20] sets an update rate per light (1 means every frame, 0.5 every second frame, lower again at distance) and keeps a static atlas. It invalidates the directional-light cache when `dot(cached, current light direction) < 0.9999`, about 0.8°. This is explicit quantisation of the Sun direction.

### 6.5 Temporal amortisation: cost against quality

- **Updating every N frames trades lag for cost.** Round-robin or time-sliced updates divide the average cost by about N, but shadows lag the light by N frames. Cyberpunk accepts seconds of lag for distant shadows [18]; Flax accepts 0.8° of sun motion [20], which is more than the Sun's whole disc (0.53°/r[AU]).
- **No async overlap in wgpu.** Overlapping shadow rasterisation with compute is the usual way to hide its cost [21]; wgpu has no async queue for it.
- **Temporally accumulated shadow masks.** I found no primary source with numbers for half-resolution or checkerboarded shadow masks [gap]; temporal filtering of shadows belongs to questions 2 and 3.

### 6.6 Measured budgets

| Source | Hardware, resolution | Work | Cost |
|---|---|---|---|
| Karis 2021 [2] | PS5, ~2496×1404, upsampled to 4K | Nanite culling and raster, full visibility buffer (~25M triangles) | ~2.5 ms (cluster cull 0.41, raster 1.15 + 0.18, HZB 0.10) |
| Karis 2021 [2] | same | Visibility buffer to G-buffer (materials) | ~2 ms |
| Aaltonen 2015 [1] | Xbox One, 1080p | 250k objects, GPU-driven G-buffer | 2.3 ms GPU, 0.2 ms CPU |
| Haar 2015 [1] | Unity (PS4/XB1) | Occluder depth plus 512×256 downsample | 0.6 + 0.1 ms |
| Bevy 0.15 [10] | RTX 3080, 2240×1260 | 3375-bunny visibility buffer | 0.93 ms |
| Bevy 0.17 [12] | RTX 4070 | 130k dragons, geometry and materials | ~3.5 ms |
| Bevy 0.16 [11] | Laptop RTX 4090, Vulkan | Caldera hotel, whole frame | 10.16 ms (was 33.55) |
| Kapoulkine 2023 [7] | RTX 4070 Ti | Kittens, 49.4M triangles after culling | 6.96 ms per frame |
| CuRast 2026 [3] | RTX 5090, 4K | 3B instanced triangles | 125.7 ms hardware, 9.6 ms software |
| CuRast 2026 [3] | RTX 5090, 4K | Sponza, 262k triangles | 0.018 ms hardware |

**No primary source for a shadow-specific budget.** I found no primary source giving "sun shadows = X ms at 1440p–4K" for a shipped title. Epic's "Virtual Shadow Maps in Fortnite Battle Royale Chapter 4" returned HTTP 403 and was not read. What the evidence shows is that high-frame-rate modes drop the expensive shadows:

- Fortnite's 120 fps console mode turns VSM off [22].
- VALORANT targets 144+ fps with a forward renderer built on Unreal's mobile path, and 30 fps on Intel HD 4000 [23]. This is from a search summary only; the page returned 403.

### Takeaways for kalast

- **Rasterisation alone would take about 40 % of the 240 fps budget.**
  - Main view, two body layers and the near layer at full resolution make about 25M triangles per frame, about 1.6 ms on a 5080 [speculation], before shading, the penumbra walk, the TPM or the UI.
  - Cull each layer separately. Most clusters fall outside the near layer's footprint. Cone-cull clusters that face away from the Sun: expect about 25 % at cluster level on dense, scan-like meshes and about 50 % per triangle.
  - Apply the BVH LOD cut per view, with the error measured in that view's pixels or texels. It saves triangles in the main view and the 4096² layers, but almost none at 16384², where texels outnumber facets.
- **Cache each body's Sun layer in the body frame.**
  - Re-render a layer only when the Sun direction in that body's frame has moved past a threshold tied to the solar disc or to one texel of shadow-edge displacement. Flax's 0.8° is too coarse.
  - Didymos turns at about 0.044°/s at 1× time. A 0.05° threshold therefore means a re-render about once per simulated second: rarely when interactive, every frame above roughly 240× speed-up. A paused simulation costs nothing.
  - Keep self-shadowing and the other body as separate layers, as Unreal does with static and dynamic depth, so that orbital motion invalidates only the mutual-shadow part.
- **Scroll the near layer instead of redrawing it.** It is fitted to the camera, so Insomniac-style scrolling or page caching applies: shift the cache, render only the exposed slabs, and invalidate on Sun motion.
- **Keep the TPM out of every cache and time-slice.** The TPM must read layers rendered for its own Sun direction. Caching and time-slicing belong to the display path only, so physics never depends on stale data. Since wgpu has a single queue, display shadow work competes directly with the TPM's GPU work; skip it whenever the script needs the GPU.
- **Write for wgpu's lowest common denominator.**
  - No async compute.
  - Multi-draw indirect is emulated on Metal, so use index compaction and one `draw_indexed_indirect`.
  - Mesh-shader WGSL runs only on Vulkan.
  - The 64-bit-atomic software rasteriser (the Bevy meshlet style) needs Vulkan on the RTX 5080 or an M3 or later Mac, so keep a hardware-raster path for the M1 Pro and measure its triangle rate.
  - A visibility buffer pays mainly at high MSAA counts or with expensive per-sample shading [8].

### Sources (section 6)

1. U. Haar, S. Aaltonen, "GPU-Driven Rendering Pipelines", SIGGRAPH 2015 Advances in Real-Time Rendering. https://advances.realtimerendering.com/s2015/aaltonenhaar_siggraph2015_combined_final_footer_220dpi.pdf
2. B. Karis, R. Stubbe, G. Wihlidal, "A Deep Dive into Nanite Virtualized Geometry", SIGGRAPH 2021 Advances. https://advances.realtimerendering.com/s2021/Karis_Nanite_SIGGRAPH_Advances_2021_final.pdf
3. M. Schütz, L. Lipp, E. Kristmann, M. Wimmer, "CuRast: CUDA-Based Software Rasterization for Billions of Triangles", arXiv 2604.21749, April 2026. https://arxiv.org/abs/2604.21749 (tables: https://arxiv.org/html/2604.21749)
4. "Blackwell: GeForce RTX 5000 architecture and innovations", HWCooling, 2025. https://www.hwcooling.net/en/blackwell-geforce-rtx-5000-architecture-and-innovations-analysis/
5. G. Wihlidal, "Optimizing the Graphics Pipeline with Compute", GDC 2016 (abstract only). https://gdcvault.com/play/1023109/Optimizing-the-Graphics-Pipeline-With
6. A. Kapoulkine, "Meshlet size tradeoffs", 2023. https://zeux.io/2023/01/16/meshlet-size-tradeoffs/
7. A. Kapoulkine, "Fine-grained backface culling", 2023. https://zeux.io/2023/04/28/triangle-backface-culling/
8. C. Burns, W. Hunt, "The Visibility Buffer: A Cache-Friendly Approach to Deferred Shading", JCGT 2(2), 2013. https://jcgt.org/published/0002/02/04/paper.pdf
9. The Forge, Release 1.57 (May 2024), "Visibility Buffer 2.0 Prototype". https://github.com/ConfettiFX/The-Forge/releases/tag/v1.57
10. JMS55, "Virtual Geometry in Bevy 0.15", November 2024. https://jms55.github.io/posts/2024-11-14-virtual-geometry-bevy-0-15/
11. Bevy 0.16 release notes, April 2025. https://bevy.org/news/bevy-0-16/
12. Bevy 0.17 release notes, 2025. https://bevy.org/news/bevy-0-17/
13. Bevy `MeshletPlugin` docs (main branch). https://dev-docs.bevy.org/bevy/pbr/experimental/meshlet/struct.MeshletPlugin.html
14. wgpu 30.0.1 crate sources (as used by kalast): `wgpu-types/src/features.rs`, `wgpu-hal/src/metal/adapter.rs`, `wgpu/src/api/adapter.rs`. https://github.com/gfx-rs/wgpu (checked in the local cargo registry)
15. Epic Games, "Virtual Shadow Maps", Unreal Engine documentation. https://dev.epicgames.com/documentation/en-us/unreal-engine/virtual-shadow-maps-in-unreal-engine
16. M. Day, A. Hastings, M. Acton (Insomniac), "CSM Scrolling", SIGGRAPH 2012 Advances. https://advances.realtimerendering.com/s2012/insomniac/Acton-CSM_Scrolling(Siggraph2012).pdf
17. CRYENGINE documentation, "Cached Shadows". https://www.cryengine.com/docs/static/engines/cryengine-3/categories/1114113/pages/21267738
18. B. Dybisz, M. Witanowski, "Shadows Optimizations in Cyberpunk 2077", SIGGRAPH 2021 Talks. https://history.siggraph.org/wp-content/uploads/2022/06/2021-Talks-Dybisz_Shadows-Optimizations-in-Cyberpunk-2077.pdf
19. A. Courrèges, "DOOM (2016) – Graphics Study", 2016. https://www.adriancourreges.com/blog/2016/09/09/doom-2016-graphics-study/
20. Flax Engine, Shadows manual (https://docs.flaxengine.com/manual/graphics/lighting/shadows.html) and Flax 1.9 `ShadowsPass.cpp` (https://raw.githubusercontent.com/FlaxEngine/FlaxEngine/1.9/Source/Engine/Renderer/ShadowsPass.cpp)
21. NVIDIA, "Advanced API Performance: Async Compute and Overlap". https://developer.nvidia.com/blog/advanced-api-performance-async-compute-and-overlap
22. PSU, "Fortnite Battle Royale Chapter 4 brings Unreal Engine 5 features…", December 2022. https://www.psu.com/news/fortnite-battle-royale-chapter-4-brings-unreal-engine-5-features-including-nanite-lumen-virtual-shadow-maps/
23. Epic Games tech blog, "VALORANT's foundation is Unreal Engine" (returned 403; claims taken from a search-engine summary). https://www.unrealengine.com/en-US/tech-blog/valorant-s-foundation-is-unreal-engine

---

## A. Verification notes

Sections 1–6 were researched in parallel and each cites what it read. The coordinator re-checked the following
claims:

- **wgpu 30.0.1 features**, checked against the local crate sources:
  - ray query on Vulkan, DX12 and Metal;
  - 64-bit atomic min/max on Metal: Apple9, or Apple8 with Mac2;
  - mesh-shader WGSL only on Vulkan;
  - `MULTI_DRAW_INDIRECT_COUNT` only on DX12 and Vulkan;
  - DXC selection (`Dx12Compiler::Auto`, `static-dxc`);
  - no public ray-tracing-pipeline API in `wgpu`/`wgpu-core`;
  - AS limits, including 1 acceleration structure per stage on Metal.
- **M1 Pro capabilities.** A Metal probe on this machine (macOS 26.4.1) reported `supportsRaytracing`,
  `supportsRaytracingFromRender`, Apple7, Mac2 and Metal3 all true, and Apple8/9 false.
- **kalast's disc walk**, read from `notes/2026-10-08_sun_disc_every_shadow/README.md`: 32 directions × 32
  equal-light rings (linear limb darkening 0.56), with one bit per ring, and "a ring hidden by any texel, of any
  body, is one bit". §0.2 maps it to bitmask soft shadows on that basis.
- **UE 5.8.**
  - Confirmed on Epic's release notes: MegaLights is Production Ready and Lumen Lite is new.
  - Confirmed through Tom Looman's 5.8 summary: VSM "Prefiltered Distant" and
    `r.Shadow.Virtual.DeferredInvalidationBudget`.
  - Confirmed on the 5.8 macOS requirements page: Nanite and VSM M2+ (beta), Lumen hardware ray tracing and
    MegaLights M2+ (experimental).
  - **Not confirmed:** "Path Tracing is now supported on Mac (macOS ≥ 26.4)". It was not found on a second pass
    over the release-notes text, and the macOS page does not mention the path tracer.
- **DXR watertightness.** The DXR functional spec has a "Watertightness" subsection with a top-left rule.
- **CuRast** (arXiv 2604.21749, 23 April 2026), Schütz, Lipp, Kristmann, Wimmer: "2-5x (unique) or up to 12x
  (instanced) faster than Vulkan" for hundreds of millions of triangles, and "Vulkan remains an order of
  magnitude faster for low-poly meshes".
- **References used in §L:** Woop/Benthin/Wald (JCGT 2013) and Wächter/Binder (Ray Tracing Gems ch. 6, 2019).

**Known gaps:**
- No primary source gives a shadow-only millisecond budget for a shipped game at 1440p–4K (§6.6).
- No published WGSL BVH traversal benchmark was found (§4.2).
- No published primitive rate exists for the M1 Pro (§6.1).
- The ray-throughput tables in §4.4 are extrapolations, to be replaced by measurements.
- Epic's Fortnite VSM blog and its VALORANT blog returned HTTP 403. §1 read the former through an archive copy;
  §6's VALORANT claims come from a search summary.
