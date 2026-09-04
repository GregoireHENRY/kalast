# Reorganising `examples/hera_mars_swingby` — started, not finished

## The principle

**An example script says what to simulate; kalast works out how.** Examples are
the first thing a new user runs, so they have to be short enough to read in one
go. Deep-dive and validation work belongs in an `analysis/` subfolder, and
superseded scripts in `old/`.

The advanced path stays open — anyone who needs to drive render passes and
frusta by hand still can. It just stops being the ordinary way to get a picture.

## What was done

- `analysis/` created; `tiri_deimos_compare.py` moved there.
- `old/` now holds the superseded scripts: `afc.py`, `rad.py`, `rad_sum.py`,
  `tiri_data.py`, `tiri_data_deimos_only.py`, `tiri_diffuse_lighting.py`,
  `tpm.py`, `tpm_show.py`, `mesh_equator_meridian0.py`.
- `cosmographia/` added — `view_tiri.py`, `view_afc.py`, `tiri.py`, `sensors/`.
- **`diffuse_lighting_one_image.py`** written: one epoch, geometry and diffuse
  lighting, no TPM and no radiance. `res/`-free but it does need the kernels
  and three meshes. This is the "quick look" entry point.
- 47 output PNGs deleted from `examples/` — outputs do not belong beside the
  scripts.

## What is left

**The FITS and TPM/radiance side is not reorganised yet.**

## The engine change this argues for

`tiri_deimos_frame.py` and `tiri_deimos_movie.py` each hand-roll a two-pass
render: one pass per body, each with the shadow frustum fitted to that body
alone, then composited. They fake per-body bounds by parking the other body at
`z = -1e9` scaled by `1e-6`, and carry a cross-frame `st` state machine to
cache the first pass.

That is engine work sitting in user code, and it exists only because
`Eye::fit_projection` fits **once to the whole scene**
(`src/app/window.rs`). With Mars at 3,396 km in the scene, 6 km Deimos gets
almost no shadow-map texels.

**Done the same evening**, 22:50. `config.shadow_per_body` — on by default —
gives each body its own shadow layer, and the Sun no longer needs aiming: each
layer derives its direction from `sun.pos` and the body it targets, so
`sun.dir` and `sun.anchor` are no longer consulted for shadowing. Measured on
Deimos beside Mars, the shared map found 49 shadowed pixels where the per-body
map finds 249 — it was **missing** most of the shadow, not adding to it. Mutual
shadowing survives, since each layer spans the whole scene in depth even though
it is sized to one body.

`tiri_deimos_frame.py` and `tiri_deimos_movie.py` still carry the hand-rolled
version. Simplifying them onto the new path should reproduce their output, and
would be the test that it covers what they were doing by hand.

Note what the single shared frustum does and does not cost: Lambert `cos i` is
per-facet geometry and never touches the shadow map, so the terminator is
unaffected. Only cast and self shadows on the small bodies degrade. For a
diffuse quick-look that is the right trade, which is why
`diffuse_lighting_one_image.py` correctly does not carry the two-pass
machinery.

## The sun anchor, since it confused

**Superseded the same evening** — the Sun is aimed per body by the renderer
now, `sun.pos` alone determines the lighting, and `sun.look_anchor()` is gone
from the quick-look script. Kept below because it explains what the old scripts
in `old/` are doing, and why the frames they produced are still sound.


`sim.sun` is an `Eye`, the same struct as the camera. `look_anchor()` is
`dir = normalize(anchor - pos)` and nothing else, and it reads **`sun.anchor`**
— a different field from `camera.anchor`, so setting the camera's does not feed
the sun.

`diffuse_lighting_one_image.py` never sets `sun.anchor`, so it is the default
`(0,0,0)`, which in that frame is Hera itself (every `spkpos` uses `HERA` as
observer). The sun therefore looks at the spacecraft rather than at each body,
and at 1.659 AU the difference is 0.0030 deg for Mars, 0.0010 for Deimos,
0.0013 for Phobos — against a 1.3 deg Mars angular radius. **Correct for a
quick look; state it before using such a frame for anything measured.**
`sun.set_target(p)` sets the anchor and looks at it in one call.

Note also that the `sun.projection.side/near/far` pins in the `old/` scripts
(5e4 / 1e7 / 1e9) predate the auto-fit. They are now *pins that beat the fit*
(`self.near.unwrap_or(fit.near)`), so carrying them into new scripts makes
things worse, not safer. Dropping them is correct.

## Found while surveying, not yet fixed

1. **The canonical FITS directory holds the pre-timing-correction set.**
   `out/hera_mars_swingby/tiri_deimos_fits/` is the 2026-09-02 set: no
   `TIMEOFFS`, `DATE-OBS` at the label epoch. The corrected set exists only in
   `out/hera_mars_swingby/2026-09-03_timing_update/tiri_deimos_fits/`, made on
   the other machine from the reconstructed image list, so its files are named
   `tiri_rad_*` while this machine's real CSV yields `tiri_raw_*`.
   **Consequence:** `tiri_deimos_png.py` and `analysis/tiri_deimos_compare.py`
   run here today read the stale FITS, and pointing them at the corrected
   directory matches zero pairs. Re-running `tiri_deimos_fits.py` here fixes
   both — it already has `tiri_timing.ENABLED = True`.

2. **`analysis/tiri_deimos_compare.py` mixes two epochs.** It takes the green
   prediction marker and the blue Mars limb from the CSV *label* epoch, while
   the simulated panel beside it was rendered at label − 24.89 s. Against
   corrected FITS this figure shows a large offset that is purely its own bug.
   `tiri_deimos_png.py` does it right, reading `ET` from the header.

3. **Its docstring is superseded** — it still concludes "a 0.7 degree pointing
   offset", which `notes/2026-09-03_tiri_timing/` retracts.

4. **`tiri_deimos_frame.py` hardcodes the label epoch** with no timing
   correction, so its like-for-like overlay on the observed 12:08:36 frame is
   ~30 px off. Left as is, deliberately, for now.

5. **Phobos is drawn 10x oversized in `diffuse_lighting_one_image.py`**, as a
   visibility hack. Kept, and now commented in the script.

## Unrelated, and open

**Cosmographia: Mars goes almost fully dark when the Hera catalog is loaded.**
Cause unknown. Work on the Cosmographia scenarios stopped there.
