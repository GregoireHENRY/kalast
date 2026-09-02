# Handoff, 2 September 2026 — TIRI Mars swing-by geometry and radiance

Work laptop to home PC. Companion to `2026-09-02_HANDOFF.md`, which covers the
Didymos phase 2 work from the desktop; this one is only the Mars/Deimos TIRI
thread. They are separate subjects and neither supersedes the other.

## Getting the files

`out/` is gitignored, so none of the outputs travel with the repo. They are on
the ROB share, uploaded 2026-09-02:

    https://cloud-as.oma.be/index.php/s/2WFbxawmx6WZi7L

**Read-only** -- download only. The write-enabled link for the same folder is
deliberately not recorded here, since this repository is public and that link
grants delete as well as upload.

    hera_mars_swingby/                    <- new, all of today's Deimos work
      README.md                              what is in the bundle
      DELIVERY_2026-09-02.md                 the delivery note, read first
      2026-09-02_HANDOFF_tiri_deimos.md      this note
      tiri_deimos_fits/                      17 simulated radiance FITS
      tiri_deimos_png/                       17 composites, Deimos over Mars diffuse
      deimos_real_vs_sim.png                 observed vs simulated, all 17
      deimos_swingby.mp4                     context, 11:50-12:12 UTC
      deimos_axis_flip_diagnosis.png         the 180 deg evidence
      mars_landmark_orientation_test.png     the lat/lon reprojection test
      geometry_validated_115603.png          render vs real, 1.8 px
      photometry/roughness_sweep.csv         flux vs coverage and opening angle

    hera_didymos/                         <- three run directories added
      didymos_tpm_3orbit_v2/
      dimorphos_tpm_v2/
      phase2_mutual_heat_mutual/

**Deliberately not uploaded:** `hera_didymos/tiri_fits/` and `tiri_movie/`, the
GIS3D product. The local copies are from 31 August, before the 180 deg detector
fault was found, and `examples/hera_didymos/tiri_fits.py` builds the camera a
different way again (`dir = m @ [0,0,1]`, `up = m @ [1,0,0]`) that has never
been validated against data -- and cannot be yet, since there are no real TIRI
Didymos frames. The share still holds whatever colleagues had before; nothing
was overwritten. **Check that convention before regenerating or circulating
the GIS3D product.**

Also not in the bundle: the TPM restart at `out/hera_mars_swingby/deimos_tpm/`.
It is a spin-up product, not a deliverable -- re-run `tpm_deimos.py` to rebuild
it. Note it carries **no self-shadowing**; see the photometry section.

Superseded outputs with the wrong orientation stayed on the laptop, suffixed
`_SUPERSEDED_wrong_geometry`. They were not uploaded anywhere.

## Where things stand in one line

Two real geometry faults were found and fixed. A **third, unresolved defect**
displaces the rendered body at close range — it is the one that matters,
because the close frames are the science.

**Read this first: agreement on the distant frames proves very little.** At
7,600 km Deimos is 7 px across and unresolved, the aspect changes slowly, and
almost nothing about the model is being tested beyond a static angular
boresight. Quoting "1.4 px at 11:56" as validation, as an earlier draft of this
note did, overstates it badly. The close frames carry the resolved shape, the
limb, the fast-changing aspect and all the radiometric content. They are the
deliverable and they are the ones currently affected.

## Do not be misled by the comparison figure

`out/hera_mars_swingby/deimos_real_vs_sim.png` shows large red/green marker
gaps on the last six frames. **That gap is our renderer disagreeing with our
own projection formula, not our simulation disagreeing with reality.** Proof
below — it is measured entirely on simulated images, with no observation
involved.

## THE BUG (start here)

The rendered body is **translated** relative to where its own mesh and pose put
it, by an amount that grows as range falls. Measured by projecting the actual
5,040-facet mesh through the exact pose the render used
(`pxform(IAU_DEIMOS, HERA_TIRI)` plus the aligned position) and comparing
against the silhouette of non-zero pixels in our own simulated FITS. No
observation is involved, so this is purely our renderer against our geometry:

    epoch      range    mesh-projected span     rendered span    lo err  hi err
    11:56:03  7596.4   [ 473.8,  480.7]        [ 474,  480]        0.2    -0.7
    11:58:43  6181.6   [ 475.3,  483.6]        [ 475,  483]       -0.3    -0.6
    12:01:23  4768.1   [ 475.1,  485.9]        [ 475,  485]       -0.1    -0.9
    12:04:03  3357.0   [ 482.5,  497.6]        [ 483,  497]        0.5    -0.6
    12:07:06  1751.3   [ 713.3,  740.9]        [ 714,  741]        0.7     0.1
    12:07:17  1655.6   [ 705.6,  734.6]        [ 709,  737]        3.4     2.4
    12:07:27  1568.7   [ 693.9,  724.4]        [ 699,  728]        5.1     3.6
    12:07:38  1473.3   [ 675.3,  707.4]        [ 685,  715]        9.7     7.6
    12:07:48  1386.9   [ 651.1,  685.3]        [ 664,  696]       12.9    10.7
    12:07:59  1292.1   [ 615.4,  652.2]        [ 636,  670]       20.6    17.8
    12:08:10  1197.6   [ 568.7,  608.4]        [ 600,  637]       31.3    28.6
    12:08:36   976.7   [ 395.9,  445.1]        [ 422,  469]       26.1    23.9
    12:08:47   884.6   [ 283.2,  337.8]        [ 328,  379]       44.8    41.2
    12:08:57   801.9   [ 150.2,  211.1]        [ 208,  265]       57.8    53.9
    12:09:08   712.5   [ -41.3,   28.6]        [  53,  117]       94.3    88.4

**Both edges move by the same amount**, so the silhouette is the right width
and the right shape — it is in the wrong place. Sub-pixel agreement holds out
to 1,751 km, then the error switches on and grows to ~90 px.

### Leading hypothesis: capture-time offset

`tiri_deimos_fits.py` steps the TPM at `DT_FINE = 10 s` and, per its own
comment at the capture site, takes **"the step nearest each image epoch"**,
while the FITS header records the *image* epoch. At closest approach Deimos
crosses the field at up to 20 px/s, so a nearest-step error of a few seconds is
tens of pixels. Dividing the measured shift by dx/dt gives:

    12:07:17  -3.28 s     12:08:10  -6.39 s
    12:07:27  -3.32 s     12:08:36  -2.87 s
    12:07:38  -4.41 s     12:08:47  -3.77 s
    12:07:48  -4.39 s     12:08:57  -3.80 s
    12:07:59  -5.35 s     12:09:08  -4.67 s

Consistently -3 to -6 s across a rate range of 50x, and `DT_FINE/2 = 5 s` is
exactly the worst case that "nearest step" allows. That is a strong match, but
**it is not proven**: the four distant frames give -5.9, -14.8, +19.1, -0.1 s,
where dx/dt is near zero and the quotient is meaningless, so they neither
confirm nor refute it.

**Next step:** make the capture land exactly on the image epoch — step to the
image epoch directly instead of snapping to the grid, or record the true
captured epoch in the header and compare against that. If the residual
collapses, this was it. This is cheap to test and should be done first.

### Ruled out

- Not the camera: `camera.pos = [0,0,0]`, `dir = +Z`, fixed `fovy`, set every
  frame.
- Not the body's shape or a clipping effect: the mesh projection above already
  accounts for the true 7.8 x 6.0 x 5.1 km silhouette, and both edges move
  together.
- Not near-plane clipping, which would narrow the silhouette asymmetrically
  rather than translate it.

### Retracted from an earlier draft of this note

An earlier version claimed the rendered body was "83% too narrow" and that a
centroid sat "further from the centre than the body's radius, which is
geometrically impossible". Both were artefacts of comparing a markedly triaxial
body against a **volume-equivalent sphere** of radius 6.203 km; 0.82 x 6.203 =
5.09 km is simply Deimos's smallest semi-axis. Against the real mesh the width
is correct. Ignore any width-based reasoning in older text.

Older sphere-based table, kept only so the retraction is checkable:

    epoch       pred cx  R px     pred span      rendered span   mid err
    11:56:03      477.5   3.6   [ 474,  481]     [ 474,  480]      -0.5   ok
    11:58:43      479.8   4.4   [ 475,  484]     [ 475,  483]      -0.8   ok
    12:01:23      481.0   5.7   [ 475,  487]     [ 475,  485]      -1.0   ok
    12:04:03      490.8   8.1   [ 483,  499]     [ 483,  497]      -0.8   ok
    12:07:06      728.8  15.6   [ 713,  744]     [ 714,  741]      -1.3   ok
    12:07:17      721.9  16.5   [ 705,  738]     [ 709,  737]      +1.1   ok
    12:07:27      711.1  17.4   [ 694,  729]     [ 699,  728]      +2.4   ok
    12:07:38      693.5  18.6   [ 675,  712]     [ 685,  715]      +6.5   ok
    12:07:48      670.4  19.7   [ 651,  690]     [ 664,  696]      +9.6   ok
    12:07:59      636.0  21.2   [ 615,  657]     [ 636,  670]     +17.0   OFF
    12:08:10      590.8  22.8   [ 568,  614]     [ 600,  637]     +27.7   OFF
    12:08:36      422.4  28.0   [ 394,  450]     [ 422,  469]     +23.1   OFF
    12:08:47      312.1  30.9   [ 281,  343]     [ 328,  379]     +41.4   OFF
    12:08:57      181.9  34.1   [ 148,  216]     [ 208,  265]     +54.6   OFF
    12:09:08       -5.7  38.4   [ -44,   33]     [  53,  117]     +90.7   OFF

Two facts to work from:

1. **The rendered body is also too narrow, consistently.** Rendered width over
   predicted width is ~0.83 at every OFF epoch (e.g. 12:09:08: 64 px rendered
   against 77 predicted; 12:08:47: 51 against 62). So it is a scale *and* a
   shift, not a pure displacement.
2. **It is not a pure scale about a fixed point either.** Solving
   `err = (s-1)(cx - c0)` with s = 0.83 gives c0 = 736 at one epoch and 558 at
   another, so a single scaling centre does not fit.

Reproduce in seconds, no rendering needed — the simulated FITS are already on
disk. The script above lives in the transcript; the essential part is: project
the body centre with the same formula `tiri_deimos_compare.project` uses, and
compare against `numpy.nonzero((img > 0).any(axis=0))`.

### Hypotheses, untested

- **Near-plane clipping.** Most promising. Deimos is 712 km away and 12 km
  across at the worst epoch, while Mars sits ~20,000 km out in the same scene.
  If the camera near/far planes are derived from `scene_bounds()` — which the
  hemicube far plane now is, changed earlier this week in
  `Fit the hemicube far plane to the scene, not to the requesting body` — then
  Mars could be pushing the near plane out far enough to clip the near face of
  Deimos. Clipping the near cap would both narrow the silhouette and shift it,
  which matches both facts above. **Check `src/app/window.rs` and the camera
  projection for how near/far are chosen, and whether Mars's presence changes
  them.** Note the movie/FITS scripts push the unused body to `z = -1e9`, which
  may itself be distorting the bounds.
- Perspective divergence: at 712 km the body subtends 1 deg, so the difference
  between the true perspective silhouette and a small-angle gnomonic point is
  real but should be ~1% of R, not 240%.
- A vertex-precision or mesh-scale issue at close range (mesh is in km).

Rule out the near plane first; it explains both the narrowing and the shift.

## What IS validated and safe to send

Two faults were found and fixed today, both real:

1. **The detector was rendered 180 degrees rotated** (`camera.up` was `-Y`,
   now `+Y`). Established three independent ways — Deimos's traverse direction,
   Mars's sub-solar hot spot, and lat/lon reprojection consistency across
   epochs (correlation 0.74 against 0.20 / 0.04 / 0.01).
2. **A 0.60 deg instrument alignment the FK does not carry.** Fitted on four
   frames, then confirmed on Mars *without refitting*: +0.59 px against
   +40.4 px uncorrected. `kalast/tiri_alignment.py`.

Resulting agreement with the real frames:

    11:56:03   1.4 px   0.02 deg
    11:58:43   2.1 px   0.03 deg
    12:01:23   3.5 px   0.05 deg
    12:04:03   7.1 px   0.09 deg
    12:07:06  10.6 px   0.14 deg

Scale is right too: Mars's limb gives r_obs/r_pred = 1.058, 1.052, 1.059,
1.015 with an unbiased estimator.

What this does and does not establish: the 180 deg convention and the 0.60 deg
alignment are solid, and Mars's limb independently confirms the plate scale.
But this agreement is measured where Deimos is small and near the field centre,
so it does not certify the close frames. **Do not treat "hold the last six
frames" as the answer** — an earlier draft of this note said that, and it was
wrong. The close frames are the scientific product; the defect above is
bounded, understood in its scaling, and has a concrete first thing to try.

## Radiance

Disc-integrated flux, computed by summing over facets rather than rendered
pixels (a rendered frame samples ~35 of ~2,600 visible facets and carries
5-11% sampling noise). Model is 5-12% dim:

    11:56:03  obs/model 1.120     12:01:23  1.093
    11:58:43  obs/model 1.104     12:04:03  1.051

Best-fit roughness is hemispherical craters, coverage 0.80, RMS slope 43.9 deg
— **do not quote it.** The boost roughness supplies *grows* with phase angle
(1.087 at 2.57 deg, 1.204 at 4.18 deg) while the boost the data needs
*shrinks* (1.120 -> 1.051). Opposite trends. All four epochs are within
1.6 deg of opposition where beaming is weakest, so this dataset cannot
constrain roughness. Needs a larger-phase dataset.

## Also fixed today

- **Pause now works.** `P` gated only `Simulation::update`, which increments a
  counter, so it never stopped the Python callbacks where all TPM work happens.
  It now gates `before_render`/`after_render` while the frame still renders and
  presents, so the window stays responsive. `src/app/mod.rs`. Not a regression
  from this week — `git log -S is_paused -- src/app/mod.rs` is empty.
- **The preroll was 187x more expensive than needed.** `nonuniform_max_dt` is
  1876 s; the scripts used `DT_FINE = 10 s`. Verified converged: 750 s vs 47 s
  changes disc-integrated flux by 0.44%. `tiri_deimos_photometry.py` uses the
  stability limit and runs in seconds.
- **`tiri_deimos_compare.detect()` was broken by Mars.** It thresholded at
  median + 20 MAD over the whole frame and kept the largest cluster, so Mars
  won on size at 12:07:06 (656 px error) and suppressed detection entirely from
  12:07:48. Replaced with a high-pass peak. **Caveat: the high-pass finds the
  limb once the body is resolved**, so it is fine for unresolved Deimos and
  should not be trusted for the near frames.

## Retracted today, do not resurrect

- The 0.73 deg "boresight misalignment" in the old `tiri_alignment.py`. It was
  fitting the residual of the 180 deg flip. The tell: the apparent offset grew
  from 0.66 to 1.05 deg as Mars closed in, which no rigid rotation can do.
- A "+20.7 s timestamp offset". It fits the X residual across a 100x range of
  image-plane rate, which is genuinely striking, **but it is computed from the
  positions the render bug corrupts.** Re-derive only after the bug is fixed;
  if the residual survives, it becomes interesting again.
- A "43% radiance gap" — that was rendered-pixel undersampling, not physics.
  True gap is 5-12%.

## Files touched, uncommitted

    src/app/mod.rs                                   pause gates the callbacks
    kalast/tiri_alignment.py                         rewritten, carries retraction
    examples/hera_mars_swingby/tiri_deimos_frame.py  camera.up, ALIGN=True
    examples/hera_mars_swingby/tiri_deimos_fits.py   camera.up, alignment, provenance keys
    examples/hera_mars_swingby/tiri_deimos_movie.py  camera.up, alignment
    examples/hera_mars_swingby/tiri_deimos_compare.py  projection, alignment, Mars diffuse, detect, legend
    examples/hera_mars_swingby/tiri_deimos_png.py    new
    examples/hera_mars_swingby/tiri_deimos_photometry.py  new
    notes/2026-09-02_tiri_geometry/                  new
    out/hera_mars_swingby/DELIVERY_2026-09-02.md     delivery note

Nothing committed. Superseded outputs are suffixed
`_SUPERSEDED_wrong_geometry`.

## Still queued

- T. Mueller comparison — his returned CSV is not on either machine; only the
  geometry we sent him, `out/old/old2/geometry.csv`. Ask for the file.
- Pixel-wise radiance comparison — needs the instrument PSF. The real source is
  12.2 px against a 7.2 px true disc at 11:56, so the model must be convolved
  before any per-pixel comparison. Total flux is unaffected.
- `examples/hera_didymos/tiri_fits.py` builds `up` differently (`m @ [1,0,0]`)
  and needs its own check before the GIS3D product is trusted.
