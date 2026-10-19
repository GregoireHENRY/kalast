# Didymos's light curves for Eli: what the plots show, and where DART's shape falls short

19 October 2026: questions on the plots of `out/didymos_eli/`
(`2026-10-15_didymos_lightcurves/`, `2026-10-16_didymos_every_day/`),
answered with numbers. One answer corrects the earlier notes: Dimorphos's
shadow falls near Didymos's north pole in 2022, not on its southern cap.

## The plots' labels

- **LM, TS, TN, geo**: Les Makes (La Reunion), the five nights simulated
  before; TRAPPIST-South (Chile); TRAPPIST-North (Morocco); from Earth's
  centre, the requested nights with no data.
- **chi**, in the events plots: the rms of (data - model) / stated error
  over the points inside the predicted events, the rotation fitted out.
  **(none)** is the same against no event at all, a flat line; a model
  that lowers chi below it finds the event in the data.
- **Lommel-Seeliger**: the scattering law of dark, single-scattering
  surfaces, brightness proportional to cos i / (cos i + cos e) -- Scheirich
  et al.'s, with their spheroids. kalast's DART-shape curves use Hapke with
  Buratti et al. (2024)'s parameters. Swapping the law moves an event by
  1-2 mmag.
- **The green curve**, "rotation x0.44 (fitted)": kalast's rotational curve
  with its amplitude scaled to the night's data by least squares. A
  diagnostic only, and a confusing one: where kalast's curve is out of step
  with the data the best scale shrinks toward zero and the curve goes flat
  (2022-08-26, x0.14); where it is in step it adds nothing (09-12, x0.80).

## Why DART's primary eclipses are small, before the impact

In August and September 2022 the Sun was 11-12 deg and Earth 27-43 deg
south of Didymos's equator. Dimorphos orbits in that plane, so its shadow
falls about 0.2 km north of the equator, near Didymos's north pole: ground
lit at a grazing angle and seen from Earth near the limb (`shadow.py`, the
shadowed facets of the system against Didymos alone):

| primary eclipse | Sun, Earth latitude | shadow's ground: DART, spheroid | cos incidence | cos emission | light it takes |
|---|---|---|---|---|---|
| 2022-08-16 20:18, LM | -12.3, -26.6 deg | z +208 m, +214 m | 0.27, 0.34 | 0.15, 0.23 | 0.5 %, 1.8 % |
| 2022-09-04 04:50, TS | -11.3, -36.9 deg | z +201 m, +195 m | 0.29, 0.43 | 0.11, 0.18 | 0.3 %, 1.3 % |
| 2022-09-12 03:18, TS | -10.6, -42.8 deg | z +188 m, +172 m | 0.39, 0.53 | 0.09, 0.16 | 0.3 %, 1.1 % |

DART's Didymos is flatter there -- its north pole at z = +0.267 km, the
spheroid's at +0.296 -- so its polar ground faces the Sun and Earth less,
and the shadow on it takes three to four times less light. The 2022
campaign's 16 primary eclipses (`2026-10-16_didymos_every_day/`) are 1.48
times DART's depth and 0.74 the spheroids': the real north pole lies
between. A grazing eclipse is as sensitive to the orbit's tilt: a degree
moves the shadow 20 m on Didymos.

The secondary eclipses are another matter: Dimorphos wholly in Didymos's
shadow, their depth is Dimorphos's share of the light, and the two models
agree to 15 %. The spheroids' match there is partly by construction --
Scheirich et al. fitted D2/D1 = 0.223 to those eclipses.

## The rotation's timing, night by night

kalast's rotation shifted in time against each night's data, outside the
events, an amplitude and a level fitted at each shift (`shifts.py`):

| nights | best shift | amplitude the data want |
|---|---|---|
| 2022-09-21 TS, 10-04 TN, 10-21 TN -- around the encounter | 0 +- 1 min | 0.7-1.0 |
| 2022-09-12 and 09-14 TS | kalast 10-12 min ahead, 27-32 deg of rotation | 0.5-0.6 |
| 2022-09-15 TS | 17 min ahead | 0.8 |
| 2023-01-14, 01-17 TN | 10-13 min behind | 0.55-0.64 |
| November-December TN | best fits inverted: the curve's shape differs, not only its timing | |

Where the shift is small the drawn features match; on 09-12 and 09-14
kalast's drops come 10-12 minutes before the data's, and its maximum near
8 h on 09-14 is both early and too bright. DART's shape model was built from
the encounter's images; from Earth in August-September Didymos was seen from
27-43 deg south, where its features rest on less, and the amplitude comes
out 1.7-2 times the observed. Part of a shift may also be the rotation's
phase, set from the campaign's runs (`2026-10-15_didymos_lightcurves/`,
*Didymos's rotation*).

## 2022-09-04, TRAPPIST-South: a dip that is not the eclipse

From 3.5 to 5.2 h the data are 0.06-0.09 mag fainter than at the same
rotation phase one turn earlier or later (`night0904.py`, 10-minute bins)
-- 1.7 hours where the predicted primary eclipse lasts 4.4-5.3 h, three to
four times deeper than Dimorphos's whole shadow can make it. Something in
the photometry overlaps the eclipse that night: a field star in a comparison
star's aperture, the comparison stars' colour against the sky. Eli's
reduction would tell.

## After the impact, 2022-12-09

Neither model is the reference. Scheirich et al.'s post-impact model is a
spherical Dimorphos (D2/D1 = 0.202) beside a spheroid Didymos; DART's shapes
are the pre-impact Dimorphos -- there is no post-impact shape -- beside
Didymos's own. The secondary eclipse's plateau differs by 10 % (0.042
against 0.047): Dimorphos's share of the light, lower with DART's shapes.
Over 225 clusters after the impact, DART's shapes give Scheirich et al.'s
table's depths at the median (primary eclipses 0.94, secondary 0.94-0.95).
The data decide: on Eli's TRAPPIST-North nights of 12-18 and 12-19 both find
the events, the spheroids a little better (chi 0.11-0.12, DART's 0.15, none
0.26-0.29).

## Open

- The green curve replaced by kalast's rotation shifted and scaled to the
  night, the timing it shows.
- 2022-09-04's comparison stars, on Eli's side.
- Didymos's north polar region, and the orbit's tilt, from the primary
  eclipses; the light curves' features for the rest of the shape.

The scripts (`shadow.py`, `shifts.py`, `night0904.py`) are in
`out/didymos_eli/scripts/questions/`, beside what made the curves.
