# 2026-10-09 — Unreal Engine 5.8 against kalast, and PBR

Asked: "ok for UE 5.8, just compare techniques and if anything can be learnt
or used, i heard a lot also about PBR". The sources and figures are in
`2026-10-09_rendering_survey/README.md` (§1 Unreal, §2 shadows, §3
antialiasing, §5 global illumination); this is the comparison it was for.

UE 5.8 shipped on 17 June 2026 and is likely the last 5.x; UE6's early access is
announced for the end of 2027, with no rendering technique named.

## Technique by technique

| UE 5.8 | What it is | kalast | What to take |
|---|---|---|---|
| Virtual Shadow Maps | 16k virtual maps in 128 px pages, only the pages the screen needs drawn; clipmaps for the Sun; pages cached until something moves | A layer per body fitted to what the camera sees, a near layer, LOD cuts per layer; `shadows.cache` keeps layers in each body's frame | Done in kind: texels where the image is, a cache. Not worth porting the page tables: Epic stops caching a Sun that moves continuously, which kalast's always does in a running simulation |
| Shadow Map Ray Tracing (SMRT) | Soft shadows from rays marched through the first depth layer, 8 rays a pixel, a gap-filling guess behind occluders | The disc walked over each slice in 32 directions, a second depth layer, the camera's image for nearby relief | kalast is the more exact (bitmask fusion, second layer, limb darkening); Epic's own docs call ray-traced shadows the best |
| Contact shadows | A short march in screen space | `near_blocked` in the penumbra scan | The same idea, as Bend Studio's. Only in the image: the per-facet query does without it, the TPM's shadows not depending on the camera |
| Nanite | Cluster DAG, GPU culling, a visibility buffer, software raster of small triangles with 64-bit atomics | The LOD patch tree, cut per view and per shadow layer on the CPU | GPU culling into one indirect draw; the software raster needs 64-bit atomics, which the M1 Pro lacks. kalast's frames are GPU-bound (Didymos pair 1.8 ms GPU of 1.9) |
| Lumen | Real-time GI: screen traces, distance fields, a surface cache, temporally filtered | None in the image | Not as such: biased and history-dependent. Light bounced into shadowed craters from radiosity over the TPM's view factors, or path traced (the ray tracing handoff) |
| MegaLights | Many lights, sampled stochastically, ray traced, denoised | One light | Nothing |
| Hardware RT, Path Tracer | Inline ray queries; a separate unbiased progressive reference with jittered samples, TAA off | Being started on the home PC (`2026-10-09_HANDOFF_ray_tracing.md`) | The template for `quality = "reference"` |
| TSR | Temporal upscaling and antialiasing from history | MSAA 1/2/4, overlay antialiasing | Not for exports (history, not reproducible); for interactive use MSAA plus SMAA or FXAA, about 1 ms |
| Substrate | Layered materials | Reflectance laws per body | Nothing |

## PBR

In games "physically based rendering" (Karis, "Real Shading in Unreal Engine
4", SIGGRAPH 2013 course; Epic's "Physically Based Materials") means:

- a microfacet BRDF -- GGX distribution, Smith shadowing, Schlick's Fresnel --
  with base colour, metallic and roughness, energy-conserving;
- lights in physical units (lux for the Sun, candela), a camera exposure in
  EV100 or aperture, shutter and ISO;
- image-based lighting from the sky;
- a tonemap (filmic, ACES) from the scene's radiance to the display.

kalast is physically based in the sense that matters for planetary images, a
step further than games:

- **Reflectance**: laws fitted to the bodies themselves -- Lambert,
  Lommel-Seeliger/Lambert, Hapke with roughness, opposition surge and
  porosity -- each giving the radiance factor I/F, validated against AFC and
  TIRI images (`2026-10-08_mars_phase_fit/`, `2026-10-08_deimos_afc_photometry/`).
  GGX describes polished and metallic surfaces; regolith is Hapke's.
- **Units**: a lit pixel is the exposure times I/F (`light.exposure`,
  `shading.srgb_mode = 1`), linear, no tonemap.

What PBR practice has that kalast could still take:

1. **Floating-point exports.** An 8-bit PNG holds I/F to 1/255 of the exposure
   and clips. A 32-bit float image (TIFF or `.npy`) would carry the radiance
   itself, as the radiance products already do.
2. **Radiance in physical units** for an image: W m^-2 sr^-1, the Sun's flux
   at the body's distance in the camera's band, rather than I/F times an
   exposure -- what a detector counts.
3. **A tonemap for display only**, never in an export, for scenes with both a
   lit limb and a shadowed crater's floor in view.
4. **The camera as an instrument**: integrating each pixel's area (the
   reference's jittered samples), the PSF, and noise -- SurRender's approach
   (survey §3).
5. **A white-furnace check**: a law at albedo 1 under a uniform sky reflects
   what it receives -- a test of energy conservation the laws do not have yet.
6. **Light from the sky and the surroundings**: kalast's atmosphere already
   lights shadows from the dust; for an airless body, the light other facets
   reflect (radiosity over the view factors) is the equivalent of IBL.

None of these needs a GGX lobe. Offered, not started.
