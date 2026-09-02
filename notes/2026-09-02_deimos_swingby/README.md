# 2 September — the Deimos TIRI product, and where it stopped

An urgent request: simulated TIRI radiance for Deimos at the Mars swing-by, to
sit beside the real frames for radiometric comparison. It was delivered, and
then a geometry check found something that should be settled before anyone
uses the radiance.

Read `2026-09-02_deimos_roughness/` alongside this: the roughness result has
its own note.

---

## Delivered

**17 simulated FITS**, `out/hera_mars_swingby/tiri_deimos_fits/`, one per
observed frame in `tiri_images_mars_swing-by_deimos.csv`, at that frame's epoch
and filter. Plus a 120-frame context movie, a contact sheet, and a frame-by-
frame comparison against the calibrated product.

Three scripts, all new:

- `tpm_deimos.py` -- spin-up, 371,833 steps in 13 s on the GPU.
- `tiri_deimos_fits.py` -- two Deimos rotations of thermal history with
  self-shadowing, then a capture at each image epoch.
- `tiri_deimos_movie.py`, `tiri_deimos_compare.py`, `tiri_deimos_frame.py`.

### Body-specific traps, both silent if got wrong

- **The seasonal wave is Mars's 687-day year**, not Deimos's 30.31 h
  `orbit_period`, which is its tidally locked diurnal period. Using the latter
  builds a column millimetres deep.
- **Three orbits is not enough.** Unlike Didymos. The surface settles in two,
  but the deep reservoir is 4.32 K out at three and 1.52 K at four. The ceiling
  is the ephemeris: Deimos reaches back 4.78 Mars years and then `spkpos` fails
  with SPKINSUFFDATA mid-run. 4.7 is the default, ~0.5-1 K short.

### Measured, not assumed

Deimos is **never eclipsed** by Mars at any of the 17 epochs, and **neither
Mars nor Phobos occults it** -- Mars is behind it throughout. The mutual
shadowing originally asked for does not exist here. Both are evaluated per
frame and written to the header. Self-shadowing is modelled.

Deimos does **transit Mars's disc** in seven frames, separation falling to
1.06 deg against an 8 deg angular radius, so those frames carry a warm
background the product does not model. `MARSBACK` flags them.

---

## The radiance gap, and roughness

Full account in `2026-09-02_deimos_roughness/`. In short: the model came out
**34 % low** at the best-resolved frame, thermal inertia moves the wrong way
and cannot explain it, and the Kuehrt crater roughness closes it at an RMS
slope of ~46 degrees -- which is the value Giese and Kuehrt published for
Deimos in 1990, from the paper that introduced the model. Nothing was tuned.

The correction was ported to Rust and validated against that paper's own
schematic, the exact limits, and node convergence.

---

## Where it stopped: the geometry does not match

**Deimos sits 0.7 degrees from where the kernels put it.** On the three frames
where Mars is out of shot and the detection is unambiguous:

| UTC | predicted - observed | offset |
|---|---|---|
| 11:56:03 | (+44.9, -37.4) px | 0.76 deg |
| 11:58:43 | (+39.5, -38.9) px | 0.72 deg |
| 12:01:23 | (+35.7, -40.1) px | 0.70 deg |

Everything checkable on this side has been checked and none of it accounts for
the offset:

| hypothesis | test | result |
|---|---|---|
| FOV or boresight | `getfov(-91200)` | boresight `[0,0,1]`, half-angles 6.65 x 5.00 against 6.667 x 5.0 assumed -- 2.6 px at the frame edge |
| axis convention | all 8 flips and swaps | none fits; best 128 px with 227 px scatter |
| render vs projection | rendered centroid | agree to **0.3 px** |
| epochs | CSV against filename | **0.01 s** |
| timing | dt needed to close it | 1286-2617 s, inconsistent, and the offset does not lie along the motion (dot +0.63, +0.51, **-0.90**) |
| aberration | all 9 SPICE corrections | agree to 0.3 px; stellar aberration 0.13 px |
| observer | instrument vs spacecraft | identical |

Mars cannot arbitrate: at 12:08:36 its predicted angular radius is 613 px
against a 384 px frame half-height, so the disc overflows on three sides and a
circle fit returns nonsense -- median residual 43-75 px, which is thermal
structure, not a limb.

A single frame rotation reconciles all three frames to half a pixel, but that
is **not usable**: the three observed positions span 6 px, so they constrain
one direction, and three free angles fitted to one direction are degenerate.
The minimal rotation, 0.7 deg, is well determined; its axis is not.

### It is the instrument alignment, and the kernel says so

Deimos is **not faintly present** at the predicted position and brighter
elsewhere. There is nothing there at all: within 25 px of the prediction the
strongest pixel is 2.2 sigma and **no pixel exceeds 5 sigma**, while the source
at the observed position is **519 sigma**. The body is simply 0.7 degrees from
where the kernels put it.

`hera_v16.tf`, line 1245, ahead of the TIRI frame definition:

> *"**Nominally**, the TIRI frames are co-aligned with the s/c frame"*

`FRAME_HERA_TIRI = -91210`, class 4, relative to `HERA_SPACECRAFT`, with
`TKFRAME_ANGLES = (0.0, 0.0, 0.0)`. That is a **pre-flight nominal alignment
that has never been calibrated in flight**, and a real boresight misalignment
would be invisible to SPICE and appear as exactly this.

The attitude itself is `hera_sc_meas_*`, reconstructed, and the trajectory is
trusted. Neither is in question. What is missing is the instrument-to-
spacecraft alignment.

**Measured from the three clean frames:**

| | value |
|---|---|
| total misalignment | **0.7297 deg**, spread 0.0254 |
| about X | **+0.507 deg**, very stable (+0.489, +0.508, +0.524) |
| about Y | +0.52 deg mean, but drifting +0.586 to +0.467 over five minutes |

The X component is steady to 0.014 deg across the sequence. The Y component
drifts by 0.12 deg over five minutes, which is more than the scatter and is not
yet explained -- either a residual attitude effect or something in the geometry
that has not been isolated. Three frames spanning five minutes is a thin basis
and this should be redone across the whole swing-by, and against a body other
than Deimos, before it is offered as a calibration.

The kernel reissue is **not** the cause: v180 to v182 did not touch Deimos.

The July validation compared kalast against **ShapeViewer**, both driven by the
same kernels. It establishes that the two renderers agree, which is worth
having, but it cannot detect a common misalignment -- both would be wrong
together. That is why this did not surface then.

### What this does and does not invalidate

A **pointing** offset moves where bodies land on the detector. It does not move
Deimos relative to the Sun or the observer, so it does not change what Deimos
emits: the disc-averaged radiance comparison, and the roughness result, stand.
A **pixel-wise** comparison does not, and is blocked until this is resolved.

---

## To do, in the order that unblocks the most

1. **Confirm the alignment offset against a second target.** Deimos alone,
   over five minutes, is a thin basis for a 0.73 deg calibration, and the Y
   component drifts. Mars's limb across the whole swing-by would settle it, as
   would any other body TIRI imaged in cruise. The four-way comparison at 12:08:36 -- filter g,
   Deimos crossing Mars's limb, both bodies constraining the geometry at once.
   kalast's half is rendered (`out/hera_mars_swingby/frame_120836/`, FITS
   included); ShapeViewer and Cosmographia remain. The HERA Cosmographia
   package is at `data/spice/hera/misc/cosmo`, and `load_hera_ops_001.json`
   loads the operational kernels; its scenarios target Dimorphos, so a
   Mars-swingby view has to be set up.
   The kernel version is not it: v180 to v182 did not touch Deimos.
2. **Compare against Mueller's model radiances** for this same swing-by, CSV
   in the old `hera_mars_swingby` work. Like-for-like on the exact
   observation, and the most direct external check available.
3. **Pixel-wise radiance**, once the geometry is settled. Disc-averaged was a
   first cut, not the validation.
4. **Re-do the July validation presentation** with the current code.

### Roughness, still open

5. **Davidsson and Rickman 2014**, **Davidsson et al. 2015** -- the reference
   1D-conduction-plus-3D-correction treatment. Partial code in
   `examples/old/roughness_davidsson`.
6. **Rozitis et al. 2024** figure 2(a) -- roughness in the **upper boundary
   condition** rather than as a geometric correction, section 8.3's route
   rather than 8.2's. Partial code in
   `examples/old/didymos_comparison_rozitis2024`.
7. **The emission bound.** The roughness result is sensitive to it: 75 degrees
   against 85 moves Deimos's disc average by a factor of three. Until that is
   understood the 46 degrees is a consistency check, not a retrieval.
8. **Regenerate the roughness LUT** from the Rust port, which takes 13 s for
   78,292 geometries, and wire it into the radiance path. A GPU port is not
   worth it -- this is a once-per-band cost.

### Carried over, still not done

9. **Phase 2 for Didymos** with heating, on the desktop.
10. **Re-run the GIS3D product** with heating: Dimorphos self and mutual,
    Didymos self only.
11. **Checkpoint resume is only half tested.** It restores and prints
    correctly, but the test resumed at the final step, so "resume and
    continue" has never run. That is the half phase 2 depends on.
12. The paper, `2026-08-26_TIMELINE.md`.

---

## Fixed along the way

- **An occluded window stopped the simulation dead**, and separately the
  redraw chain broke on a single dropped event, leaving a run frozen until
  clicked. Both fixed; `CLAUDE.md`'s "keep the window frontmost" rule retired.
- **Temperature means are area-weighted now**, not per facet. Facet areas span
  226-541x on these meshes; Dimorphos's surface mean moves 237.8 -> 250.6 K.
- **The restart fingerprint used Python's `hash()`**, which is randomised per
  process, so it never matched across runs. Now sha256.
- **The comparison overlay was mirrored in Y**, which invalidated the first
  geometry figure. Caught because Mars sits far off-axis where 8 px at the
  boresight becomes obvious.
