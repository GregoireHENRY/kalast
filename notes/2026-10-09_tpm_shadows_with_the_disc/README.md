# 2026-10-09 — the thermophysical model's shadows with the Sun a disc; the body panel's law and horizon map

Asked: "yes go, TPM needs to use the selected shadow method" -- the per-facet
shadow query, which the thermophysical model reads, was a point Sun's whatever
`light.sun_as_point` said. And: "i dont see horizon map on UI app. I also
dont see how to toggle and config hapke law per body on UI app".

## The query with the disc

`sim.facet_shadow(body)` came from `facet_shadow.wgsl`: each facet's three
corners and centre, a single texel each, lit or not, a quarter each. With
the Sun a disc it is now `cs_facets` in `mesh_shadow.wgsl`, beside the
image's own penumbra code, so the model sees what the image shows at
`shadows.pcf = 0`:

- the same four points, each through `sun_at` and the pyramid
  (`sun_reaches`); with nothing in reach the hardware's single comparison
  for its layer and the other bodies' slice; in the umbra 0; between, the
  walk;
- with a horizon map, the disc's light over the facet's horizon from its
  centre (`horizon_seen`), as the image takes it;
- `1.0` hidden where the facet faces away from the Sun;
- the body's own layer, not its near layer: the layers are fitted whole while
  facets are queried (`shadow_whole`), so the answer does not depend on the
  camera.

Group 5 is the body's own bind group (its pose, flags, layer, horizons), now
visible to compute, as are the shadow group's comparison sampler; group 6 the
query's.

## Making it affordable

The Didymos pair at iteration 702, Dimorphos's shadow across Didymos, every
facet of both queried every step, 1600 x 1000, steps per second (median of
five blocks of 30):

| | point Sun | disc | disc, `second_depth` off |
|---|---|---|---|
| no query | 514 | | 195 |
| the walks in each point's own thread | 88 | 27.5 | 30.5 |
| walks queued, `cs_walk` | | 33.7 | 39.1 |
| and the pyramid once per facet | | 38.8 | 45.8 |

- **The walks.** The first version walked each point's 32 directions in its
  own thread, a SIMD group held by its longest walk, as the image's
  fragments once were: 19 ms of a step (without them, 70 steps a second).
  Now a point that needs one queues it (`walks`, 60,241 on Didymos there, of
  room for 262,144), the image's `cs_walk` takes them a lane per direction,
  and `cs_facets_walked` adds each walked point's share to its facet,
  in fixed point.
- **The pyramid.** Looked round for each of four points of 6.3 million
  facets, 8 ms. Now once per facet, about its centre, as far again as its
  corners are and as far from the Sun as the farthest (`sun_reach_about`):
  where nothing can reach any point of the facet, its points take the
  comparison; only facets near a penumbra look round per point.
- **Not it**: the PCF kernel's vectors, worked out by `sun_at` for every
  lookup, now apart (`SunKernel`) and only for the image's PCF -- no change
  in step rate.

What the disc costs a step there, all told: 26 ms against 11 with a point
Sun, of which 3 ms is the image's penumbra pass and the rest the query.

## Against rays

`tests/test_facet_shadow_disc.py`: a plate of 80,000 facets 0.2 apart and a
wall on it, 4 high, one body, the Sun 25 deg up with an angular radius of
0.02, the shadow's edge a penumbra about 0.9 wide. Per facet, against the
fraction of the limb-darkened disc that rays from its corners and centre do
not reach past the wall -- the points the query averages:

- 1,716 facets across the edge, 780 in the penumbra: rms 0.92 %, mean
  -0.54 %, worst 2.4 %;
- 116 different values over the penumbra's facets, not quarter steps;
- wholly hidden deep in the shadow, wholly lit far from it.

The same to the digit before and after the queue and the per-facet pyramid.

## The body panel

The simulation panel's section for each body has, after the mesh:

- **scattering**: the law, Lambert, Lommel-Seeliger/Lambert or Hapke, and
  its numbers -- Hapke's `w`, `b`, `c`, `b0`, `h`, `theta_bar` in degrees
  (radians in a script) and `k`. A law chosen again comes back as it was;
  an edit the law refuses (`Hapke::check`) is not taken.
- **shadows**: `horizon_map`, on and off.

Both write the fields a script sets (`body.scattering`, `body.horizon_map`),
read each frame. Compiled and the suite run; not looked at on screen.

## Not done

- `light.sun_as_point` stays `True` by default. The disc costs a step of the
  Didymos pair with the query 2.3 times what a point does, and an image
  2.6 times (195 frames a second against 514 at 1600 x 1000), against a
  target of 240 and more.
