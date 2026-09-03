# MSAA on the main render pass

`Config::msaa` — 1 (off), 2, 4 or 8, **default 4**. Exposed to Python as
`app.config.msaa`, read when the window is created, so it must be set before
`App.start()`.

## Why

Every silhouette this renderer draws is a science measurement: a limb, a
terminator, a body's apparent diameter. At one sample per pixel each of those
is quantised to whole pixels, which biases anything fitted from an exported
frame — the Mars limb fits in `notes/2026-09-03_tiri_timing/` are exactly that
kind of measurement, and the Deimos centroids there are quoted to a few tenths
of a pixel against a hard-edged disc.

## What is and is not multisampled

**Only the main pass.** The shadow map, the facet-id pass and the hemicube pass
stay single-sampled deliberately: they carry ids and depths, not colour, and
averaging an id across samples produces an id that belongs to no facet. The
facet-id buffer is what `tiri_deimos_fits.py` uses to attach a temperature to a
pixel, so this is not a detail — multisampling it would silently corrupt the
radiance products.

The light cube is drawn *inside* the main pass, so its pipeline takes the same
sample count; wgpu rejects a pipeline whose count disagrees with the
attachments in flight.

## Exports are unchanged

The pass draws into a multisampled colour buffer and **resolves into
`render_texture`** on store — the same single-sample target that has always
been blitted and exported. Nothing downstream changed: the blit, the HUD
overlay and the frame exporter all read what they always read, and exported
PNGs keep their shape and size. The multisampled depth buffer is discarded at
the end of the pass and never read back.

## Fallbacks and one caveat

`resolve_samples` asks the adapter what it supports and falls back to 4, then
to 1. Requesting an unsupported count would otherwise panic inside wgpu at
pipeline creation, which is a poor way to find out a machine only does 4x. 4 is
guaranteed for every renderable format.

**Caveat:** `debug_depth_show` mirrors the main pass's depth only at `msaa = 1`.
Above that the pass writes its own multisampled depth buffer, and the debug
view is not it.

## Status

Compiles and was built `--release` today. **No before/after comparison was
recorded** — the visual improvement is assumed, not measured, and the effect on
a fitted limb radius or centroid has not been quantified. Worth doing before
any number from an exported frame is restated: MSAA changes edge pixel values,
so a centroid or radius measured from a 1-sample export and one measured from a
4-sample export are not the same measurement.
