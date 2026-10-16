# A binary's primary events in `lightcurve.flux`: past the piece cap, now quartered

## What was wrong

`shadowing::lit_area` takes a facet's lit (or visible) area by subtracting
every facet in front of it, one at a time, from what is left of it, kept as
convex pieces -- at most 256 (`MAX_PIECES`). A facet under many smaller
facets shatters. In Didymos and Dimorphos at DART's 3,072 facets each,
Didymos's facets are about 23 m and Dimorphos's 5 m: a Didymos facet under
Dimorphos's shadow, or behind Dimorphos as seen, met 30 to 60 of them --
both Dimorphos's near and far sides, in the order of the projection grid's
buckets. The system's flux overflowed at every epoch of a primary eclipse
or occultation; each body alone never did, nor any secondary event, where
Didymos's large facets fall on Dimorphos's small ones.

The overflow path was wrong as well. It stopped part-way through the
pieces, kept those it had cut and dropped the others, so the area it
returned was neither the upper bound its documentation promised nor a
lower one.

Found by Eli's geocentric days set against the TRAPPIST-North nights
(`2026-10-15_didymos_lightcurves/`): at identical epochs the two agreed to
0.0005 mag, but the event's drop jumped by 3 mmag between epochs 20 s apart.

## The fix

- **Quartering.** A region of a facet with more than 8 occluders in front of
  it is cut into four at its edges' midpoints, and each quarter clipped
  against only the occluders that reach it; down to 4^6 = 4,096 of a facet.
  The quarters' areas add up to the facet's exactly, so nothing is
  approximated. Which of two facets is in front is decided on the quarter's
  share of their overlap, the same answer for facets that do not cut
  through each other.
- **Overflow**, now only past six quarterings, stops before the occluder
  that would pass the cap: what is left before it is an upper bound, as
  documented.
- **Only what the flux reads.** `flux_at` clipped every facet along both
  directions and used the lit fraction of the facets facing the Sun, the
  visible fraction of those facing both. `shadowing::lit_fractions_where`
  clips only those; the night side, under the whole day side, was the
  costliest of all and thrown away.
- **Python's lock.** `flux`, `lightcurve` and `lit_fractions` release it
  (`py.detach`), so a script's threads compute epochs at once.
- **Test.** `a_facet_under_many_small_occluders_is_exact`: a facet under a
  slab of 1,024 triangles -- its near and far sides, 16 x 16 each, in a
  scrambled order -- exact to 1e-9 without overflow. It overflows with the
  quartering off.

## Measured

From Earth's centre, the system and each body apart at one epoch, both
models: the event's drop (mag), and the three fluxes' time.

| epoch | DART's shapes | Scheirich's spheroids | time, DART | time, spheroids |
|---|---|---|---|---|
| 2022-08-16 08:24, primary eclipse | 0.0071 -> 0.0072 | 0.0292 -> 0.0294 | 0.63 -> 0.12 s | 0.42 -> 0.20 s |
| 2022-08-16 02:32, secondary eclipse | 0.0429 | 0.0470 | 0.53 -> 0.05 s | 0.37 -> 0.04 s |
| 2022-11-18 00:52, primary eclipse | 0.0918 -> 0.0943 | 0.0988 -> 0.1024 | 0.49 -> 0.30 s | 0.45 -> 0.28 s |
| 2022-11-18 05:10, Dimorphos occulted | 0.0421 | 0.0478 | 0.58 -> 0.04 s | 0.73 -> 0.05 s |
| 2022-11-18 03:00, no event | 0.0000 | 0.0000 | 0.57 -> 0.12 s | 0.16 -> 0.03 s |

Scheirich et al. (2024)'s table gives the 2022-11-18 primary eclipse
0.104: the spheroids, their model, now 0.1024.

The same eclipse's partial phase from TRAPPIST-North, 01:15 to 01:23 UTC
every 20 s: before, between 0.0608 and 0.0438, up and down by up to 3 mmag
from one epoch to the next; after, 0.0676 falling smoothly to 0.0570 --
before was 0.008 to 0.013 mag shallow, 13 to 23 %.

## What it changes

- **The Didymos work of 2026-10-15** (`2026-10-15_didymos_lightcurves/`),
  re-run. Eli's 45 nights, the drop's largest change at an epoch where an
  event is on:

  | | DART's shapes | spheroids |
  |---|---|---|
  | 15 nights before the impact | 0.003 mag | 0.001 |
  | 29 nights after | 0.026 mag (2022-11-22), up to 42 % of the drop | 0.006, 14 % |

  After the impact the phase angle was 50 to 75 deg and Dimorphos's shadow
  long across Didymos: more of its facets under it, each under more of
  Dimorphos's. The secondary events and Didymos's rotation are as they
  were. Chi in the events moved by 0.01 to 0.3 (`events.csv`); what the
  notes read off it stands. The campaign's comparison, before the impact,
  moved by the true period it had been run without, hardly by this.
- **Nothing else.** The clipping serves `lightcurve` and
  `kalast._rs.shadowing.lit_fractions` only; the thermophysical model's
  shadows are the GPU's.
