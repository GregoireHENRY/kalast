# 2026-10-09 — cascaded shadow maps; quality presets

Asked: "One quality switch with three settings: fast (point Sun, cached
shadows), accurate (disc plus second depth) and reference (ray traced)" --
"ok but also point sun without cached shadow, maybe also add CSM even if they
are less precise for even faster shadow like how they do in most recent video
games ... for quick look at a scene".

## Cascades (`shadows.cascades`)

`n` layers over slices of the camera's view and one over the scene, in place
of a layer per body (`Window::update`: `cascade_region`, the splits three
quarters logarithmic, one quarter even, from the camera's near plane to its
far; each slice's box within the scene's, fitted as a body's layer is, its far
side cutting the casters). A point uses the finest cascade that holds it
within 95 % of its width and its depth (`shadow_layer_at`, `CASCADE_INNER`),
else the scene's; the per-facet query each point's own (one per facet, from
its centre, read a corner past that cascade's edge: 7 facets of 20,000 a
quarter hidden). In this mode the layers are not per body: no near layer, no
horizon map, no cache, the body and the others not apart
(`Shadows::per_body_layers`).

`tests/test_cascades.py`: a plate 200 wide and a wall, a body of its own (in
the plate's mesh the level of detail folded it into a tent shading the whole
plate, as `tests/test_near_layer.py` had found); 1024 texels, the camera 13
from the shadow's edge: 1.6 px from nine tenths to a tenth with three
cascades, 22.1 px with one layer over the scene; the per-facet query right on
all 20,000 facets.

**Not quicker here.** GPU-bound frames, M1 Pro, frames a second:

| | Didymos pair from 1 km, 1600 x 1000 | Dimorphos from 68 m, 3234 x 1774 |
|---|---|---|
| a layer per body, 4096, PCF 2 | 518 | 206 |
| kept (`cache`), paused | 689 | 242 |
| one layer over the scene, 2048, PCF 1 | 773 | 185 |
| 2 cascades, 1024, PCF 1 | 577 | 175 |
| 3 cascades, 1024, PCF 1 | 550 | 172 |
| 3 cascades, 2048, PCF 1 | 526 | 155 |

Each cascade draws every body; a layer per body is already fitted to what the
camera sees of it, its cut as fine as the image there. Cascades put a small
map's texels where the camera looks, which a layer per body does by itself.

## Quality (`config.quality`)

`Quality` in `config.rs`: `"quick"` one layer over the scene, a point Sun,
2048, PCF 1; `"fast"` a layer per body kept from frame to frame; `"point"` the
defaults; `"accurate"` the disc, `second_depth`, the near layer. Each sets the
Sun, the layers, cascades (0), the cache, PCF and the resolution; read back,
the name the settings are, or `None`. Python: `config.quality = "fast"`; the
settings: a button each above the shadows' rows. `"quick"` was to be the
cascades; the table made it the one layer. `tests/test_quality.py`.

A `"reference"`, ray traced, waits for the ray tracing
(`2026-10-09_HANDOFF_ray_tracing.md`).

## PCF where it shows

Profiled on Dimorphos from 68 m, 3234 x 1774, a point Sun (frames a second,
the image's pass): MSAA 4 and PCF 2, 206 (4.38 ms); PCF 0, 307 (2.79); PCF 1,
232 (3.83); MSAA 1 and PCF 2, 232 (3.85); the level of detail 2 or 4 pixels,
the same -- PCF is most of it, its five first taps about 1 ms at this size.

A kernel no wider than the image's pixel is not taken in the image
(`fs_shaded`: `radius x texel <= pixel`, the pixel from `dpdx`, `dpdy` of the
surface in `fs_main`): its blur is within the pixel. The per-facet query
takes it always, having no pixels. The Didymos pair from 1 km at 1600 x 1000,
texels finer than pixels: 518 to 677 frames/s at PCF 2. The close view above,
texels 3 cm against 1.7 cm pixels: unchanged, 200 at PCF 2 -- 242 with the
cache and nothing moving. The crater's PCF test sees the filter as before
(the darkness integral -1.0 % at pcf 4, was -1.2 %).
