# 2026-10-09 — the TPM's shadows are the image's, PCF included; the shadow cache

Asked: "can we make TPM use PCF? i dont understand why if pcf is ON it
wouldnt be included. ... TPM should follow whatever the shadow computes", and
the shadow maps kept from frame to frame "as an option, not the default".

## The per-facet query, one for every Sun and filter

`sim.facet_shadow` came from `facet_shadow.wgsl` for a point Sun: one texel
per point, lit or not, blind to `shadows.pcf` on purpose (`notes/2026-09-17_pcf_erosion.md`,
`tests/test_facet_shadow.py` asserted it). Now `cs_facets` in
`mesh_shadow.wgsl` answers for every Sun: each of a facet's corners and centre
through the image's own `sun_at` and `sun_lookup`, `shadows.pcf`'s kernel
included (worked out once per facet, a flat facet's four points sharing it),
and with the disc the pyramid and walk as before. `facet_shadow.wgsl` and
`app/facet_shadow.rs` are no longer used (left in place, for the user to say).

- `tests/test_facet_shadow.py` now asserts the opposite: at pcf 4 and 8, 102
  and 134 of the crater's 2,048 facets differ from pcf 0, the edges' blur.
  Against ray tracing over 15 Sun angles at pcf 0, the same to the facet as
  the old pass: 48 and 52 of 14,724 wrongly lit and dark.
- Cost, the Didymos pair, every facet every step, 1600 x 1000: 88 steps a
  second with the old point pass, 79.6 through the image's lookups at pcf 0,
  69 at the new default pcf 2.
- On the way, the receiver's plane gradient, three projections, left `SunAt`:
  only a walk uses it (`sun_grad`, where a walk is queued or made), and every
  lookup worked it out.

## Rays that graze the ground

In `2026-10-09_disc_gaps_hidden_relief/`.

## The shadow cache (`shadows.cache`, `shadows.cache_degrees`)

Off by default. With it, each layer is fitted to its whole body (as when the
maps are read per facet), there is no near layer, and per layer the window
keeps (`LayerCache`): the matrix and bias it was drawn with, its body's pose,
the Sun's way in the body's frame, and the bodies drawn into it with their
poses about it. A frame keeps a layer when the same bodies are in it, the Sun
has moved no more than `cache_degrees` in the body's frame, and each of the
others has turned and moved about it by no more than that (moved: that angle
times its distance); a change of resolution, slices, disc, bias settings or
meshes (`mesh_epoch`, counted in `sync_meshes`) drops them all (`CacheUnder`).

A kept layer is not drawn, nor its other bodies' slice nor its second depth
layer, nor the pyramid when all are kept; its matrix is moved with its body,
`view_proj x mat_then x mat_now^-1`, so its own shadows turn with it -- and
where the body has not moved, the matrix as drawn, to the bit (through the
product, one facet of 5,012 flipped on an edge).

`tests/test_shadow_cache.py`: a plate with a wall, a floating box:

- nothing moving, the shadow pass 0.000 ms against 1.57 drawn every frame;
- the camera moved and back, kept, the image the same to the bit;
- the plate turned 0.5 deg, at 0 drawn again, the image as with the cache
  off; at 1 kept, 0.38 % of the pixels off a layer drawn that frame;
- the per-facet query from kept layers the same as from drawn ones.

The Didymos pair paused, iteration 702, 1600 x 1000 (GPU span, medians):
point Sun 1.87 to 1.34 ms, the shadow pass 1.25 to 0; disc 6.74 to 2.55, the
shadow pass 3.42 to 0.

## Open

- A near layer with the cache: drawn every frame over kept body layers.
- The self-shadows and the others' kept apart, the others redrawn as they move
  and the body's own kept (as Unreal keeps static and dynamic depth apart).
