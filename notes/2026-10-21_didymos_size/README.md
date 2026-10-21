# Didymos's size: the example's spheroid, and how well it is known

21 October 2026. The user, editing `examples/lightcurve_didymos/scene.py`,
on its spheroid, `a = 0.5 * 0.730 * 1.369 ** (1 / 3)`: "i see didymos
extents are 851 × 849 × 620 so it should be half of it to resize ellispoid
per radius no?"; then "why trusting Scheirich et al. (which year?) and not
Daly et al. 2023 Table 1? maybe the shape model is wrong so maybe volume
0.2038 km3 is wrong and 0.2345 km3 is more correct."

Two things said first in the chat were wrong, and are corrected here: 0.730
km is not Scheirich et al.'s, and 1.369 does not come from 849/620.

## The spheroid in `main.py` and `scene.py`

- `res/ico5.obj` has radius 1: the stretch is the semi-axes.
- An oblate spheroid, semi-axes a, a and c = a / q, holding the volume of a
  sphere D across: a · a · (a / q) = (D / 2)³, so a = (D / 2) · q^(1/3).
- D = 0.730 km: the volume of DART's v003 model of Didymos, measured below.
- q = 1.369: Scheirich et al. 2024's, from the best-fit ellipsoid Barnouin
  et al. give for v003, 819 × 796 × 590 ± 14 m: "Using those values, we
  derived (A1B1)^1/2/C1 = 1.369 as the nominal axes ratio of the oblate
  primary model". Daly et al.'s 849/620 gives 1.369 too, by coincidence.
- Dimorphos after the impact: a sphere 0.202 of Didymos by volume (Scheirich
  et al.; before it, 0.223 and 1.504 equator to pole).

Didymos 811 × 811 × 592 m (a 405.3 m, c 296.0 m), Dimorphos 147.5 m.
Scheirich et al. work in ratios -- Dimorphos's size to Didymos's, the orbit
in Didymos's equatorial diameters -- and give no size in km.

## The models, measured

Loaded by kalast, the volume by the divergence theorem over the facets, the
extents in each model's own frame:

| | facets | extents (m) | volume (km3) | by volume (m) |
|---|---|---|---|---|
| Didymos v003, 9.3 m | 49,152 | 818.2 × 801.5 × 604.5 | 0.20336 | 729.6 |
| Didymos v003, 1.165 m | 3,145,728 | 823.7 × 801.2 × 606.9 | 0.20381 | 730.1 |
| Dimorphos v004, 1.94 m | 49,152 | 177.7 × 169.0 × 114.5 | 0.00176 | 149.8 |
| Dimorphos v004, 0.243 m | 3,145,728 | 179.5 × 169.4 × 115.2 | 0.00176 | 149.8 |
| the spheroid, `ico5` | 20,480 | 810.6 × 810.6 × 592.1 | 0.20358 | 729.9 |
| an ellipsoid 851 × 849 × 620 | 20,480 | 851 × 849 × 620 | 0.23442 | 765.0 |

v003 runs from z = -340 to +266 m, lopsided north to south. Seen from its
pole it is 786 to 828 m wide depending on the direction, never 851. DART's
page for v003, delivered 30 May 2023 (in
`/Users/gregoireh/data/mesh/didymos/notes.txt`): best-fit ellipsoid
819 × 796 × 590 ± 14 m, extents along its axes 819 ± 1, 801 ± 4 and
607 ± 1 m.

## Which ellipsoid, at 730 m

The user settled on 730 m by volume "for now", and asked which of three
ellipsoids that is:

| | what it is | volume (km3) | by volume (m) |
|---|---|---|---|
| 824 × 801 × 607 | v003's extents at 1.165 m: the box around it | 0.2097 | 737.1 |
| 819 × 796 × 590 | DART's best-fit ellipsoid to v003, ± 14 m | 0.2014 | 727.2 |
| 811 × 811 × 592 | the example's spheroid | 0.2037 | 730.0 |

- The box holds 3 % more than the model it is around.
- The spheroid is the best fit made round in its equator (√(819 × 796) =
  807 m) and scaled by 0.4 % to v003's volume: Scheirich et al.'s shape.
  It has no rotation of its own, so the night shows the events alone.
- For Didymos's rotation in the curve, the best fit scaled to 730 m:
  822.1 × 799.0 × 592.2 m. Its three numbers do not say where its long
  axis points in Didymos's frame, so it cannot be turned with Didymos yet.

The radii taken, in km:

| | a | b | c |
|---|---|---|---|
| Didymos | 0.4053 | 0.4053 | 0.2960 |
| Dimorphos after the impact: a sphere, 0.202 of Didymos | 0.0737 | 0.0737 | 0.0737 |
| Dimorphos before it: 0.223 of Didymos, 1.504 equator to pole | 0.0933 | 0.0933 | 0.0620 |

The impact: 2022-09-26 23:14:24 UTC. Dimorphos before it is 162.8 m by
volume, bigger than DART's v004 at 149.8 m: 0.223 is the campaign
photometry's ratio, not DART's images'.

## Didymos's size, as published

| | across (m) | by volume | volume (km3) |
|---|---|---|---|
| Daly et al. 2023, Table 1 | 849 × 851 × 620 ± 15 | 761 ± 26 m | 0.231 |
| v003, the model Barnouin et al. 2024 use | 819 × 796 × 590 ± 14, best-fit ellipsoid | 730 m | 0.204 |
| DART's PCK `didymos_system_15.tpc`, 21 September 2023, beside orbit solution s542 | 783 × 783 × 584 | 710.3 m | 0.1876 |
| the radar model from before DART | | about 760 m | about 0.23 |

- The PCK: Didymos's radii 0.39167, 0.39167 and 0.29195 km, "using the
  best-fit dimensions from" Naidu et al., then in preparation; Dimorphos
  145.6 m by volume, radii 0.09078, 0.06932 and 0.06131 km. The example
  loads this PCK for the poles and the rotation, not for the sizes.
- The radar model: v003 is "11% smaller in volume relative to the prior
  radar model, just within the 12% uncertainty (1-σ) associated with the
  radar model" (Barnouin et al. 2024); the 760 m is worked back from that.
- 0.2345 km3 is no one's measurement: it is the ellipsoid through Daly et
  al.'s extents. Their own volume, 761 m across, is 0.231 km3, and their
  table says it was "computed from the volumes of shape models, not from the
  volumes of triaxial ellipsoids with the extents listed above".
- Daly et al.'s 761 ± 26 m and v003's 730 m are 1.2 of Daly et al.'s sigmas
  apart. Didymos's size is known to a few per cent.

## Could v003 be wrong

In part, yes:

- Barnouin et al. 2024: "About 40% of the total surface area of Didymos was
  observed with both cameras at a similar pixel scale of 5 m/pixel". The
  model, stereophotoclinometry on the images of DRACO and of LICIACube's
  LUKE, "does not include limb data, and thus lacks the triangular outline
  seen in the images".
- Eli's campaign: the primary eclipses before the impact are deeper than
  v003's flatter north allows and shallower than a spheroid's
  (`2026-10-16_didymos_every_day/`, `2026-10-19_didymos_questions/`). Seen
  from 27-43 deg south in August and September, v003's amplitude comes out
  1.7-2 times the observed.

## What the size changes: the night of 2022-11-18

`main.py`'s night, every minute, the spheroids at each size, Dimorphos a
sphere 0.202 of Didymos; an event where the drop passes 1 mmag:

| Didymos | Dimorphos's shadow on Didymos | Dimorphos hidden | level, against 730 m |
|---|---|---|---|
| 710 m | 00:39-01:45, 108.7 mmag at 00:53 | 04:37-05:57, 49.2 | 0.059 mag fainter |
| 730 m | 00:37-01:46, 108.8 at 00:52 | 04:36-05:58, 49.2 | |
| 761 m | 00:35-01:47, 108.4 at 00:51 | 04:34-06:01, 49.2 | 0.089 brighter |
| 851 × 849 × 620 | 00:35-01:47, 108.5 at 00:50 | 04:34-06:01, 49.3 | 0.100 brighter |

- The depths stay: they follow Dimorphos's size against Didymos's.
- The durations do not: 3 to 5 minutes longer for 30 m more, with the orbit
  in km from the kernels. Eli's events can measure the size, against that
  orbit.
- The level moves 0.09 mag for 30 m, but it trades against the albedo:
  H = 18.16 asks for a geometric albedo of 0.19 at 710 m, 0.18 at 730 and
  0.17 at 761, where Buratti et al.'s Hapke parameters give 0.095. The
  0.7 mag gap (`2026-10-15_didymos_lightcurves/`, *The level*) stays at any
  of them.

## Kept

The example keeps 730 m: its spheroid then holds the volume of the v003
mesh it is set against, and `main.py`'s two curves differ by shape alone.
At 761 m the mesh would have to grow 4 % too.

## Open

- Didymos's size from Eli's campaign: the events' durations against the
  kernels' orbit, beside the north polar shape from the primary eclipses.

## Sources

- Daly et al. 2023, Table 1:
  <https://pmc.ncbi.nlm.nih.gov/articles/PMC10115643/table/Tab1>
- Scheirich et al. 2024, *Dimorphos orbit determination from mutual events
  photometry*, The Planetary Science Journal:
  <https://arxiv.org/html/2403.02804v1>
- Barnouin et al. 2024, *The geology and evolution of the Near-Earth binary
  asteroid system (65803) Didymos*, Nature Communications 15, 6202:
  <https://pmc.ncbi.nlm.nih.gov/articles/PMC11289119/>
- DART's page for v003, from `notes.txt` (it did not answer from here):
  <https://dart.jhuapl.edu/SOC/project-data/derived_products/flight/shape_model/Palmer_didymos-v003/Didymos_SPC_v003.html>
- `didymos_system_15.tpc`, NAIF's DART archive:
  <https://naif.jpl.nasa.gov/pub/naif/pds/pds4/dart/dart_spice/spice_kernels/pck/didymos_system_15.tpc>
