# 2026-10-12 — ray tracing on Metal (M1 Pro)

The home PC's handoff (`2026-10-12_HANDOFF_ray_tracing_to_laptop.md`): none of
the ray tracing had run on Metal. Pulled, built, tried on the work laptop
(M1 Pro, macOS 26.4.1, Metal, no ray-tracing hardware: Apple's driver
traverses in shader code).

## The main pass did not build

`config.shadows.rays = True`, a step: the main pass's fragment stage failed to
compile in Metal -- `rq_2 = metal::raytracing::intersection_query<...> {};`,
"overload resolution selected deleted operator '='". A `var rq: ray_query`
declared inside a loop (the bounce loop of `ray_bounced`,
`shaders/sun_rays_image.wgsl`) is reset by naga on each pass by assigning a
fresh query, and Metal's `intersection_query` has its assignment deleted. Now
declared once before the loop; `rayQueryInitialize` resets it on each use. The
two other queries (`ray_blocked`, `ray_seen_point`) were declared at their
functions' top and were fine. Rule for the WGSL: a `ray_query` never in a
loop's or a branch's body.

Then `sim.rays` is `True`.

## The tests

All four ray-tracing tests pass on Metal, unchanged:

- `test_facet_shadow_rays`: a point Sun exact on all 1,716 facets across the
  wall's edge; the disc rms 0.14 %, worst 0.77 %; the probe moves 85 facets,
  worst 0.20 %.
- `test_image_rays`: 0 dark pixels on wholly lit facets, 0 lit on wholly
  hidden ones, from above and obliquely, the Sun a point and a disc -- the
  pixel-exact check where Metal's undocumented watertightness would show.
- `test_reference`: stopped by itself at 1,680 samples, error 0.00195; the lit
  plate 0.42261; the penumbra within 0.0017 of float64; the same to the bit
  summed again.
- `test_reference_bounce`: Ingersoll's bowl, 1 bounce 0.12497 (0.125), 2
  bounces 0.18763 (0.1875).

The whole suite, 42 tests, passes -- `test_horizon_map` too, which failed on
the PC ("not more or less on the whole", mean +0.058): +0.040 here, under its
bound. A margin that differs by GPU, not a fault of either.

## Cost

Wall clock, median, the Didymos pair from 1 km at iteration 702 (Dimorphos's
shadow across Didymos), the PC's figures beside:

| | M1 Pro, maps | M1 Pro, rays | RTX 5080, rays |
|---|---|---|---|
| a step, every facet, point Sun, 1600 x 1000 | 16.2 ms | 147 ms | 7.4 ms |
| a step, every facet, disc (64 rays, probe 16) | 29.8 ms | 2,924 ms | 30.8 ms |
| an image, 1200 x 800, point Sun | 1.7 ms | 27 ms | 0.8 ms |
| an image, 1200 x 800, disc | 6.1 ms | 386 ms | 5.2 ms |

Point-Sun rays are 20 to 34 times the 5080's here, about the survey's 19; the
disc's 64 rays 74 to 95 times: the probe's 16 rays at every lit point, in
software. On the Mac, rays with a point Sun are usable for a still image or a
slow run; the disc and the reference are offline.

## Next, for the Mac

The handoff's open item 4: rays across the disc only where a penumbra can be,
told by the maps' depth pyramid (as AMD's Hybrid Shadows tells it by its
cascades), one ray to the Sun's centre elsewhere -- the probe's 16 rays at
every lit point are most of the disc's cost on either machine.

## The main pass without its ray code

Later the same day the user found the cascades slower than the maps, and the
UI app slow; measuring it, everything was. Dimorphos from 68 m at 3234 x 1774
drew at 106 frames a second with the maps, where it drew at 206 on 9 October,
and at 273 without shadows at all. The ray code compiled into the main pass
(`//@rt`, kept on any device with ray queries) slowed every pixel of it on the
M1 Pro, whether a ray was traced or not -- the branch never taken, its
registers still reserved. With ray queries off on the device, 195 and 685.

The main pass is now built with that code only while it traces -- `shadows.
method` "rays" or a reference image -- and rebuilt when that changes, as it
was already for the reference (`gpu::shader_for_traced`, `Passes::traced`,
`rays_wanted` in `window.rs`); the penumbra pass's pipelines never take it. The
device keeps ray queries, which cost nothing by themselves: 201 and 690 with
them on. The four ray-tracing tests pass, the rays' image built as before.

Frames a second after it, M1 Pro (`scratchpad` benchmark, GPU-bound):

| | maps | none | cascades, 3 x 4096, PCF 2 | cascades, 3 x 1024, PCF 1 | `"quick"` | maps, 2048, PCF 0 |
|---|---|---|---|---|---|---|
| the Didymos pair from 1 km, 1600 x 1000 | 685 | 1421 | 479 | 595 | 901 | 868 |
| Dimorphos from 68 m, 3234 x 1774 | 201 | 686 | 127 | 168 | 179 | 331 |

Up close the maps' 3.5 ms is mostly their lookups, 3.1 ms of PCF over 5.7
million pixels, the shadow pass 1.15. Cascades pay the same lookups and draw
every body into each cascade (3.2 ms at 1024, 6.3 at 4096): no faster than the
maps anywhere. Hard edges at 2048 are: 868 and 331.
