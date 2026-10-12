# Phobos in Mars Express HRSC SRC images: shadows and light

12 October 2026, M1 Pro.

A colleague asked for four HRSC SRC images of Phobos to be reproduced with
kalast, and compared reduced to lit or not -- "0 dans l'ombre versus 1 pas
dans l'ombre" -- as his own code does, which scored his h8870 at IoU 0.72
with Phobos's rotation from PCK00011 (`examples/phobos_shadows_fit/
sample_h8870_0006_sr2__PCK00011.png`). The user: find which of his
meta-kernel's kernels the images need, the shape model behind its DSK, and
whether a better one is wanted; compare the shadows lit or not, then with
the light: Lambert with an albedo map, then Hapke; and kalast's shadow
methods against each other, the maps, the rays and the reference.

`examples/phobos_shadows_fit/`: `main.py` draws the four images,
`compare.py` scores them and makes the figures, `hrsc.py` reads an image,
`phobos_hrsc.tm` loads the kernels.

## The kernels: 11 of 350

`rot_shdw.TM` loads 350 kernels -- 264 of them a month of Mars Express's
orbit each, and solar-array attitudes, ASPERA, ground stations. The four
images need eleven, all in ESA's Mars Express set (`phobos_hrsc.tm`):

| kernel | for |
|---|---|
| `lsk/NAIF0012.TLS` | UTC to TDB |
| `sclk/MEX_250326_STEP.TSC` | the spacecraft's clock, for the attitude |
| `fk/MEX_V16.TF` | the spacecraft's and HRSC's frames |
| `ik/MEX_HRSC_V09.TI` | the SRC: focal length, pixels, field of view |
| `pck/PCK00010.TPC` | Phobos's shape constants and rotation |
| `spk/DE405.BSP` | the Sun and Mars |
| `spk/MAR097_030101_300101_V0001.BSP` | Phobos about Mars |
| `spk/ORMM_T19_101201000000_00755.BSP`, `..._110101000000_00758`, `..._130601000000_00985` | Mars Express in December 2010, January 2011, June 2013 |
| `ck/ATNM_T6_P060401000000_01346.BC` | where the spacecraft pointed |

`rot_shdw.TM` names a later SCLK (`MEX_250915_STEP.TSC`); the clock's
correlation for 2010-2013 is the same in both. Positions are taken in J2000
and turned into the camera's frame: asked relative to `MEX_HRSC_SRC` itself
with light time, SPICE wants the camera's own ephemeris
(`MEX_STRUCT_V01.BSP`), which these images do not otherwise need.

## The shape models

`rot_shdw.TM`'s DSK, `PHOBOS_M157_GAS_V01.BDS`, is Gaskell's 2011 model
from Viking Orbiter and Phobos 2 images: 3,145,728 plates, 1,579,014
vertices, from 456 landmarks (`VO1-SA-VISA/VISB-5-PHOBOSSHAPE-V1.0`).
ESA ships it as an OBJ beside the DSK, `PHOBOS_M157_GAS_V01.OBJ` --
byte for byte `PHOBOS_M003_GAS_V01.OBJ`, and the same vertices and facets
as `~/data/mesh/phobos/phobos_m003_gas_v01.obj`, which was on this laptop
already. Its plates are fine, about 40 m; its relief is not -- 456
landmarks -- and craters come out shallow (below).

Two others to hand: DLR's `PHOBOS_K275_DLR_V02` from HRSC images (Willner
et al. 2014), 274,874 facets, in the same ESA set. And one better than both,
not to hand: Ernst et al. (2023), stereophotoclinometry from 2,382 images,
the SRC's among them -- 18 m facets, 12 million of them, positions good to
36 m, and an albedo map with the highest-resolution model. It is served by
the Small Body Mapping Tool (sbmt.jhuapl.edu) and not yet in the PDS; its
data server wants an account.

## The camera

The SRC: focal length 984.76 mm, 9 µm pixels, 9.139 µrad a pixel, a 1024
x 1024 reference CCD centred on (512.5, 512.5). A level-2 product is 1008
samples by 1018 lines read from (13, 3) on it, so its centre is 4 samples
and 1 line off the boresight: the camera looks through the window's
centre. The images are stored turned half a turn from the IK's diagram --
a sample along +X, a line along +Y -- which the first renders showed:
upside down and mirrored, all four. Phobos's position carries light time
and stellar aberration, the latter alone about 10 SRC pixels; its rotation
is taken at the time the light left it.

The images are not absolutely calibrated (`ABSOLUTE_FLUX_CALIB_FLAG =
'N'`, the radiance scaling `-9.99e+31`): brightness is compared up to one
scale an image.

| image | UTC | range | pixel | phase |
|---|---|---|---|---|
| h8870_0006 | 2010-12-10 06:44:31 | 3,642 km | 33 m | 90° |
| h8951_0005 | 2011-01-02 21:16:29 | 1,205 km | 11 m | 70° |
| h8963_0005 | 2011-01-06 09:21:16 | 2,394 km | 22 m | 58° |
| hc069_0004 | 2013-06-29 06:02:43 | 875 km | 8 m | 85° |

Mars is 60-120° off the boresight in all four, and Phobos is out of its
shadow; Mars does light Phobos's night side, which h8870 shows (60 DN
against the sky's 13-39 and the day side's 1000).

## Lit or not

A pixel of an image is lit above a fifth of the image's brightest (its
99.5th percentile): the shadows read 60-160 DN, the lit ground 1000-1600 at
the median. kalast's `mask` runs are lit wherever the Sun reaches -- the
geometric answer, grazing slopes included; its Hapke runs are cut at the
same fifth as the images, the like for like. Scored by the intersection
over union of the lit pixels, as SPICE points the camera, and moved to
where the two overlap best.

## Results

Every run: `maps`, `rays` and `reference`, lit-or-not, Hapke and Lambert,
Gaskell's model and DLR's, PCK00010 and, with the rays, PCK00011 -- 23 runs
of four images, `table.md` beside this. IoU as SPICE points the camera /
moved to where the two overlap best.

Hapke, cut at a fifth like the images -- the like for like:

| model | method | h8870 | h8951 | h8963 | hc069 |
|---|---|---|---|---|---|
| gaskell | maps | 0.791 / 0.883 | 0.901 / 0.907 | 0.913 / 0.942 | 0.742 / 0.749 |
| gaskell | rays | 0.791 / 0.879 | 0.901 / 0.907 | 0.913 / 0.942 | 0.742 / 0.749 |
| gaskell | reference | 0.791 / 0.882 | 0.901 / 0.908 | 0.914 / 0.942 | 0.743 / 0.750 |
| dlr | maps | 0.765 / 0.879 | 0.898 / 0.918 | 0.895 / 0.933 | 0.716 / 0.725 |
| dlr | rays | 0.765 / 0.874 | 0.897 / 0.918 | 0.895 / 0.933 | 0.717 / 0.726 |
| dlr | reference | 0.768 / 0.883 | 0.898 / 0.919 | 0.895 / 0.934 | 0.716 / 0.725 |
| gaskell, PCK00011 | rays | 0.789 / 0.888 | 0.899 / 0.908 | 0.907 / 0.943 | 0.757 / 0.758 |
| dlr, PCK00011 | rays | 0.761 / 0.890 | 0.884 / 0.918 | 0.890 / 0.939 | 0.734 / 0.742 |

Lit wherever the Sun reaches, grazing slopes included:

| model | method | h8870 | h8951 | h8963 | hc069 |
|---|---|---|---|---|---|
| gaskell | maps | 0.747 / 0.828 | 0.857 / 0.884 | 0.879 / 0.904 | 0.778 / 0.783 |
| gaskell | rays | 0.749 / 0.824 | 0.859 / 0.888 | 0.881 / 0.907 | 0.778 / 0.783 |
| gaskell | reference | 0.748 / 0.828 | 0.857 / 0.886 | 0.878 / 0.905 | 0.778 / 0.782 |
| dlr | rays | 0.739 / 0.834 | 0.851 / 0.897 | 0.863 / 0.893 | 0.756 / 0.759 |
| gaskell, PCK00011 | rays | 0.746 / 0.831 | 0.856 / 0.891 | 0.875 / 0.908 | 0.789 / 0.793 |

Brightness where both are lit, with the rays: the correlation r, and the
image over kalast at the 16th and 84th percentiles:

| model | law | h8870 | h8951 | h8963 | hc069 |
|---|---|---|---|---|---|
| gaskell | Hapke | 0.80, 0.84-1.25 | 0.79, 0.86-1.18 | 0.89, 0.88-1.17 | 0.48, 0.75-1.32 |
| gaskell | Lambert | 0.77, 0.83-1.32 | 0.74, 0.84-1.26 | 0.82, 0.86-1.26 | 0.52, 0.77-1.32 |
| dlr | Hapke | 0.79, 0.82-1.23 | 0.79, 0.85-1.20 | 0.88, 0.88-1.17 | 0.63, 0.77-1.23 |
| dlr | Lambert | 0.76, 0.81-1.29 | 0.75, 0.83-1.26 | 0.82, 0.84-1.24 | 0.65, 0.79-1.23 |
| gaskell, PCK00011 | Hapke | 0.81, 0.84-1.24 | 0.80, 0.86-1.17 | 0.89, 0.88-1.16 | 0.56, 0.77-1.25 |

![h8870, Hapke cut like the image](h8870_shadows_hapke.png)

![h8870, lit wherever the Sun reaches](h8870_shadows_lit_at_all.png)

What the tables say:

- **The shadow methods agree.** The maps, the rays and the reference are
  within 0.01 of each other in every IoU and 0.04 in every r. At the SRC's
  8-33 m a pixel the maps resolve what the rays do; the reference's
  integrated pixels change edges by a fraction of one. Which method draws
  the shadows does not decide this comparison; the shape and the pointing
  do.
- **The pointing is off by 10-20 pixels.** The best overlaps are
  (+15..+20, -2..-10) pixels off for h8870 and (+11..+16, 0..-13) for h8963
  over all runs, 110-180 µrad; h8951's lies along one axis (0, -8..-35),
  little limb in its frame to pin it, and hc069's is not determined at all
  (3-124 pixels), Phobos filling the frame. `ATNM_T6_P...` is the
  attitude as commanded -- "predicted attitude as commanded", ESA's CK
  readme -- and the colleague's own figure shows the same rim along the
  limb. ESA has the attitude measured on board too, a kernel a year, not in
  `rot_shdw.TM`: `ATNM_MEASURED_100101_110101_V03.BC` (241 MB),
  `ATNM_MEASURED_2011_V04.BC` (217 MB) and `ATNM_MEASURED_2013_V04.BC` (226
  MB) cover these images. Moved, h8870 goes from 0.79 to 0.88.
- **Like for like scores higher than lit at all.** Cut at the same fifth as
  the image, h8870 is 0.79 as pointed, against 0.75 lit wherever the Sun
  reaches, and the colleague's 0.72: a grazing slope dim in the image is lit
  in a geometric mask.
- **Neither shape model is better throughout.** DLR's fits h8951 better
  moved (0.918 against 0.907), Gaskell's h8963 (0.942 against 0.933) and
  hc069 (0.749 against 0.725). Both are too coarse for hc069 at 8 m a
  pixel: a crater with a clear shadow in the image is a smooth lit bowl in
  both, and the large crater's shadow edge is off, at 0.75 whatever is
  moved. Ernst et al.'s 18 m model would be the one for it.
- **PCK00011 fits a little better.** Moved, it is ahead or level on every
  image with both models, by up to 0.016, and hc069's brightness correlates at 0.56
  against 0.48 with Gaskell's model; as pointed the pointing error
  dominates and it is mixed. Phobos's orientation differs by 0.56-0.84°
  between the two kernels at these dates.
- **Hapke over Lambert.** The brightness correlates at 0.79-0.89 with Hapke
  against 0.74-0.82 with Lambert on the three images the models resolve, and
  spreads less (h8963: 0.88-1.17 against 0.86-1.26). The lit-or-not cut is
  nearly the same with either, being relative to each image. What is left,
  ±15-20 %, shows as broad patches in the ratios -- Phobos's lighter and
  darker units among them, the albedo being uniform here -- with the shape's
  missing relief and the light the model has not got: Mars-shine and light
  from the surface around.

![h8963, image over kalast with Hapke](h8963_brightness_hapke.png)

![h8963, image over kalast with Lambert](h8963_brightness_lambert.png)

![hc069, Hapke cut like the image](hc069_shadows_hapke.png)

![hc069, image over kalast with Hapke](hc069_brightness_hapke.png)

## On the way

- **Lambert's frames clipped.** At an exposure of 3 a Lambert surface of
  colour 1 saturated the 8-bit export wherever cos i passed a third, and its
  brightness correlated at 0.41-0.64 with the maps and the rays against
  0.74-0.82 with the reference, whose floats are never clipped. 0.8 now;
  Hapke's 12 leaves its frames at 44-78 of 255, quantised but no worse: the
  reference's floats correlate the same.
- **The reference's floats unclamped.** The lit-or-not cut was half the
  frame's brightest, which for the reference's floats at an exposure of
  10,000 was hundreds: IoU 0.55. Clamped to 1 first now.
- **A reference stops at its error, which a lit-or-not frame never
  reaches**: every pixel on an edge is a coin toss, which would need 65,000
  samples. `reference.max_samples = 1024` puts such a pixel within about
  1.5 %.
- **Speckles along the limb** in Gaskell's model, the lit-or-not frames
  dotted with unlit pixels where the surface is seen edge on: its facets'
  own roughness, some facing away from the Sun. Not the level of detail --
  the same with `shading.lod` off -- and the reference averages them away.

## Not done

- **The measured attitude.** The three `ATNM_MEASURED` kernels above, 684
  MB, in place of the commanded attitude: the 10-20 pixels the images are
  moved by should shrink, and with them the part of the misfit the moving
  hides.
- **Mars-shine.** h8870's night side reads about 60 DN against the sky's
  13-39 and its day side's 1000, lit by Mars, which is nearly full from
  Phobos at these geometries. kalast's night is black. Mars as a body in the
  scene, its colour its albedo, and the reference's bounces would light it.
- **The Ernst et al. model and its albedo map**, for hc069 and for the
  brightness: through SBMT, with an account.
- **The pointing from the limb alone**, so the shadows are compared with
  nothing moved for them; the limb against the sky is plain in h8870 and
  h8963, against Mars-shine on the night side less so.
- **Facet by facet**, as the colleague suggests: kalast's lit fraction of
  each facet (`sim.facet_shadow`) against the image's brightness over the
  pixels showing it (`sim.facet_id_map()`).
- **Phobos's rotation fitted** to the shadows -- the colleague's aim:
  the IoU moved as the objective, over the rotation's angles.

## Running it

From the repository root, after setting the paths in `phobos_hrsc.tm`
and `main.py`:

    python examples/phobos_shadows_fit/main.py rays hapke gaskell
    python examples/phobos_shadows_fit/main.py rays hapke gaskell pck00011
    python examples/phobos_shadows_fit/compare.py

The frames go to `out/phobos_hrsc/<run>/`, the figures and `table.md` to
`out/phobos_hrsc/figures/`. `main.py` reproduces the runs here pixel for
pixel. On the M1 Pro a run of four images takes about a minute with the
maps or the rays, and 1-3 minutes with the reference.
