# TIRI geometry: the "0.7 deg offset" was a 180 deg detector flip

2026-09-02. Closes the blocker that stopped the Deimos radiance validation.

## The claim

kalast rendered TIRI **180 degrees rotated**. `camera.up` was `-Y` and should
be `+Y`. Targets were mispredicted by up to 3.5 deg, and for two days that was
being read as a ~0.7 deg boresight misalignment and patched with a fitted
rotation.

After the fix a genuine 0.60 deg alignment remains, measured on Deimos and
confirmed on Mars without refitting.

| | before | after flip | after flip + alignment |
|---|---|---|---|
| Deimos vs real radiances | 270 px (3.51 deg) | 17 px (0.22 deg) | 3.5 px |
| Mars limb centre, Y | +40.4 px | +40.4 px | +0.59 px (sd 3.20) |

## Why the old reading was wrong

The retracted `tiri_alignment.py` fitted 0.7297 deg to three frames spanning
five minutes. It reproduced those frames and nothing else.

The tell that was missed: measuring the offset on Mars across the approach, it
**grew from 0.656 deg at 06:10 to 1.053 deg at 11:31** as the range fell from
212,000 to 43,800 km. No rigid rotation of a boresight can do that. Two
hypotheses that could -- a fixed attitude lag, and a fixed linear position
error -- both fail too (84% and 40% scatter in the implied constant). The
growth is what an axis flip looks like when the target is drifting off-centre.

## What settled it

None of these rely on the renderer being right.

1.  **Deimos traverses the field.** During the flyby its predicted column dips
    to 272 and climbs to 2071; its *measured* column peaks at 742 and falls to
    10. They run in opposite directions, so no translation can reconcile them.
    Fitting a fixed linear part and solving only for translation:

        identity   rms 32.96 px    flip Y   rms 32.97 px
        flip X     rms 10.36 px    180 deg  rms 10.28 px

2.  **Mars's thermal peak vs the sub-solar point.** Unflipped they are 150 px
    apart; flipped, 41 px (the remainder is the afternoon lag, in the right
    direction). Phase angle was 9.7 deg, so Mars is 99.3% lit and there is no
    terminator to confuse this.

3.  **Reprojection to lat/lon.** Mars rotates 19.5 deg between 09:10 and
    10:11, so only the correct convention reprojects both epochs to the *same*
    map. Correlation of the two maps:

        as-implemented  +0.20      flip X only  +0.04
        flip Y only     +0.01      180 deg      +0.74

    This needs no Mars map, no landmark identification and no renderer.

X is settled by data (1 and 2); Y is degenerate in this dataset -- Mars's pole
and the sub-solar direction both lie within 9 deg of the TIRI X axis at every
epoch, so nothing in the flyby moves in Y. Y comes from the IK, which states
pixel (0,0) is at the lower left with +Y **down**, i.e. the line index runs
against +Y. Test 3 then confirms the combination. A 180 deg rotation is also
the only one of the four that is physically realisable: a pure mirror cannot
come from a rigid mount.

## Size: the plate scale is fine

A first pass had the measured Mars disc 15% small and consistent across a 5x
range of disc sizes, which looks exactly like a plate-scale error. It was not.

- A steepest-gradient limb fit is biased **inward** on Mars because the thermal
  limb is soft (emission-angle cooling plus CO2), falling over ~130 px.
- Measuring instead by extrapolating the limb falloff to the space background,
  about the measured centre, gives r_meas/r_pred = 1.058, 1.052, 1.059, 1.015.

So the IK's 13.3 x 10.0 deg FOV is right and bodies come out the correct size
in pixels. The 3-6% overshoot is the atmospheric limb sitting above the solid
surface, plus PSF.

Two estimator traps caught in passing, both mine:

- A 50%-of-peak isophote is **not** the limb on a limb-darkened disc, and its
  centre is pulled toward the bright hemisphere -- worth ~10 px on Mars, which
  is why Mars's X is not used in the alignment fit.
- The gradient search window `arange(r-90, r+90)` goes negative for the far
  frames where Mars is only ~70 px, sampling straight through the disc. Those
  were the epochs driving the false "growth with range" trend.

## The residual alignment, 0.60 deg

    AXIS = [0.867385, 0.497636, -0.001393]
    ANGLE_DEG = 0.600215

Boresight moves by (+0.299, -0.521) deg in TIRI X, Y. Roll about the boresight
is -0.0008 deg: the minimal rotation is used, since the fitting frames are
nearly collinear and do not constrain roll. A Kabsch fit on the same frames
absorbs a spurious 5.96 deg roll -- do not use it.

Fitted on the four frames where Mars is out of shot (11:56:03 to 12:04:03, SNR
198 to 97). **Cross-validated on Mars, which took no part in the fit**: a
600-700 px disc observed three hours earlier lands +0.59 px from the corrected
prediction, against +40.4 px uncorrected. Two targets ~100x apart in apparent
size agreeing to under a pixel is what distinguishes this from a fudge.

The FK carries `TKFRAME_ANGLES = (0, 0, 0)` for `HERA_TIRI`, a pre-flight value
never calibrated in flight, so an alignment of this size is expected.

`ALIGN = True` is now the default in `tiri_deimos_frame.py`.

## Open

- **Is the flip ours or the data product's?** The IK's apparent-FOV diagram
  says +X maps to increasing sample, which is what kalast did. The data says
  otherwise. Either the delivered FITS are mirrored relative to the IK layout,
  or the diagram is idealised. Worth raising with the TIRI team; it does not
  change anything here, since the convention is now pinned to the data.
- **Didymos TIRI is likely affected.** `examples/hera_didymos/tiri_fits.py`
  builds `up` differently (`m @ [1,0,0]`), so it needs its own check before the
  GIS3D product is trusted. Not yet done.
- The 12:07:xx Deimos centroids are contaminated -- Mars exceeds 700 px radius
  by then and fills the frame. Only the four clean frames were used.

- **Mars renders zero pixels in the 12:08:36 diagnostic frame.** Noticed
  2026-09-02 after the fact, not investigated. `kalast_mars_diffuse.fits` is
  identically zero in both the 16:13 (`frame_115603_fixed`, 1024x768) and the
  16:56 (`frame_115603_super4`, 3024x1844) runs -- so the script's own
  `Mars N px` line printed 0 both times, on the one frame whose point is
  Deimos crossing Mars's limb. Deimos itself renders fine (35 and 215 px, in
  proportion to the frame areas), so it is Mars specifically. Suspect the
  facet-id mask `(ids > lo) & (ids <= lo + n_mars)` in `after_render`, or Mars
  not being drawn at ~24,000 km. Both directory names say 115603 and are
  stale; both are the 120836 epoch.

## Files

- `kalast/tiri_alignment.py` -- rewritten; carries the retraction.
- `examples/hera_mars_swingby/tiri_deimos_{frame,fits,movie}.py` -- `camera.up`.
- `examples/hera_mars_swingby/tiri_deimos_compare.py` -- `project()` negates both axes.
- `out/hera_mars_swingby/deimos_axis_flip_diagnosis.png`
- `out/hera_mars_swingby/mars_landmark_orientation_test.png`
