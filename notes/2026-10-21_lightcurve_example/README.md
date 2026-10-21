# The light-curve example: Didymos and Dimorphos from an observatory

21 October 2026. The user, reading `examples/lightcurve`: "Maybe the
lightcurve example could be on Didymos/Dimorphos at a given date from a
given observer point on Earth"; then "Can be good to be able to run
ellipsoid or real mesh to see difference".

It drew one triaxial ellipsoid -- `res/ico3.obj` stretched 1.6 x 1.1 x 1.0
-- under a fixed Sun and observer, through `lightcurve` and `Spin`, and
needed no data. Now `examples/lightcurve/main.py`:

- **The night**: TRAPPIST-North, 2022-11-18, 00:30 to 06:30 UTC every 2
  minutes, phase 60.6 deg -- a night of Eli's campaign
  (`2026-10-15_didymos_lightcurves/`).
- **The geometry** as Eli's `lc.py` places it: DART's kernels (s205, s542,
  DE430), the site by `spkcpo` with converged light time, each body at the
  time its light left. Didymos at its true period from the kernel's phase
  at the impact; not the campaign's rotation offsets (8 to 50 deg,
  `phase_fix`), which come from Eli's photometry, not from DART.
- **The light**: `flux` for the system and for each body apart, Buratti et
  al.'s Hapke law; SPICE on one thread, then `flux` an epoch a thread: 30 s
  for both shape models on the M1 Pro, 8 s for the spheroids alone.
- **The shapes**: DART's PDS models (Didymos v003 at 9.3 m, Dimorphos v004
  at 1.94 m, 49,152 facets each; 0.28 s an epoch for the system, 0.23 s
  for the two apart), or Scheirich et al. (2024)'s spheroids from
  `res/ico5.obj` under the same law -- their sizes, and how well
  Didymos's is known, in `2026-10-21_didymos_size/`. The files used are
  the copies in `/Users/gregoireh/data/mesh` and
  `/Users/gregoireh/data/spice/dart`; the
  example's README links the public ones -- NAIF's DART archive, the PDS
  Small Bodies Node -- checked, of the same sizes.

![The night with DART's shapes and with spheroids](didymos_2022-11-18.png)

DART's shapes add Didymos's rotation, about 0.1 mag either way at this
phase; the spheroids have none, and show the events and the phase angle's
slow change alone.

## Checked

- **The events**, each pass switched off in turn (`shadowing=False` leaves
  the occultations, `visibility=False` the shadows), every minute:

  | | DART's shapes | spheroids |
  |---|---|---|
  | Dimorphos's shadow on Didymos | 00:40 to 01:41, 97.4 mmag at 00:57 | 00:37 to 01:46, 108.8 at 00:52 |
  | Dimorphos hidden | 04:34 to 06:02, 46.4 at 05:10, all its light | 04:36 to 05:58, 49.2, all its light |
  | Didymos's shadow on Dimorphos | from 06:26 | from 06:20 |

- **Against 15 October**: Dimorphos hidden from 04:32 to 05:59 there, and
  from 04:33 to 05:57 in Scheirich et al.'s table, from Earth's centre.
- **Against Eli's run of the night**
  (`out/didymos_eli/nights/2022-11-18_TN.csv`): her spheroids' shadow
  0.1010 at 00:51 and 0.0991 at 00:53, the example's 0.1067 at both. Hers
  were under Lommel-Seeliger, as Scheirich et al.'s are: under it the
  example gives 0.1015 and 0.0994. Her spheroids of rings (3,968 and 1,520
  facets) in the example's geometry come within 0.6 mmag of `ico5`'s: the
  law is the difference, 8 mmag at 60 deg of phase. Her DART curve, turned
  by the campaign's offsets, has the shadow 0.103 deep: where it falls on
  DART's Didymos changes with its turning.
- **The spheroids' slow change outside the events**, 7 mmag over four
  hours: the phase angle, 60.57 to 60.39 deg, at Hapke's 0.04 mag a degree.

## The night drawn: `scene.py`

The user renamed the folder `examples/lightcurve_didymos`, and asked for
"one simple human readable/editable python example "scene.py" with the
renderer showing the scene of the night, either with ellipsoid or with
mesh".

`scene.py`, one flat script, its settings at the top: each frame the poses
from SPICE as `main.py` has them (the emission time, Didymos at its true
period), in J2000 so that the camera's up is celestial north; the camera
20 km out on the line to the observatory, 2.4 km top to bottom, its right
west -- east left, as on a sky image; the time in a HUD. The spheroids are
`res/ico5.obj` loaded smooth and stretched by their pose, which the
renderer takes with the normals turned by its inverse transpose. The night
plays in a loop on the iteration count, 30 s of it a frame, the frame rate
capped at 30 (`sim.state.rate_limited`): 24 s on screen, and `P` pauses
it -- with vsync off it would otherwise run as fast as the GPU draws. Run
in a background window: both shape sets, and stills at four moments:

![The night drawn: DART's shapes above, the spheroids below](scene_2022-11-18.png)

The events where `main.py` puts them: the shadow across Didymos at 00:52,
Dimorphos gone at 05:10, at 06:30 a crescent of it left by Didymos's shadow.

### The projection

The user asked whether `fovy = 2 * numpy.arctan(1.2 / DISTANCE)` was
TRAPPIST-North's, and expected an orthographic view. It is not the
telescope's: it frames 2.4 km at the system, ±1.2 km for Dimorphos's
orbit, from a camera put 20 km out on the line to the observatory.
TRAPPIST-North's camera has 0.60" pixels over 19.8'; that night the system
was 0.147 au away, where 1 km is 9.4 mas, and the 2.4 km framed are 22
mas -- a 27th of a pixel, under seeing of an arcsecond or more. The
telescope sees a point, whose light `main.py` sums; the scene is what a
telescope able to resolve the system would show.

From 0.147 au that view is orthographic. In perspective from 20 km, the
system's near and far sides, ±1.2 km, differ in scale by up to 6 % each
way: at 00:52 Dimorphos, nearer the camera, is drawn 4 % larger and 21
pixels (37 m) further out than in the orthographic view. kalast's camera
has both: `set_orthographic()`, and `side`, the half-height in km, in place
of `fovy`:

```python
sim.camera.projection.set_orthographic()
sim.camera.projection.side = 1.2  # km: 2.4 km top to bottom
```

`DISTANCE` then only puts the camera outside the system; the near and far
planes fit themselves, 17.4 to 22.6 km. Checked in a background window, on
the finest models:

![Perspective against orthographic, 00:52 and 05:10](scene_projection.png)

## The shape models' resolution

The user, editing `scene.py`, asked why it drew the 49,152-facet models
"instead of /Users/gregoireh/data/mesh/didymos/g_01165mm_spc_didy_v003.obj
and /Users/gregoireh/data/mesh/dimorphos/g_00243mm_spc_dimo_v004.obj" --
the same models at their finest, 3,145,728 facets each (170 MB each here,
231 MB on the PDS).

- `scene.py`: for no reason. Drawn, the finest is the one to use, and they
  load in 0.3 s. The user is setting them.
- `main.py`: for the time. On the finest, `flux` takes 40 to 58 s an epoch
  for the system and the two bodies apart, against 0.5 s: the night would
  take about two and a half hours on one thread, against 30 s.
- And a fault, not looked into yet. On the finest models at 03:00, no event
  under way, the bodies 1.17 km apart, the system comes out 6.8 mmag
  fainter than its two bodies apart: 1.421363e-3 against 1.371387e-3 +
  5.885791e-5 = 1.430245e-3, nothing overflowed, lit fractions 0.99813
  (the system), 0.99819 (Didymos) and 0.96407 (Dimorphos). At 49,152 facets
  it is 0.0. The drop at 00:52 is 75.4 mmag against 72.7, at 05:10 42.5
  against 46.4; the system is fainter than at 49,152 facets by 3.0, 9.4 and
  1.1 mmag at 00:52, 03:00 and 05:10. **Found and fixed the same day**
  (`2026-10-21_flux_double_sum/`): `flux` summed its facets in `f32`, and
  Dimorphos's, after Didymos's, were rounded away; summed in `f64`, -0.0
  mmag at 03:00, 71.2 at 00:52, 46.1 at 05:10.

## `lightcurve.py`

The user edited `scene.py` into their own: the spheroids' radii in
`EXTENTS`, the shape model in `SHAPES` at its top, the paths from kalast's
folder, the camera orthographic, the night played once. Then: "now i want
to add the part of the lightcurve creation. Keep scene.py as it is".

`lightcurve.py`, in `scene.py`'s form and with its settings: each epoch the
same SPICE block, the two bodies posed as `scene.py` poses them (`turn @
diag(EXTENTS)`, then the place), one mesh of the two, and
`kalast.lightcurve.flux` once, in a plain loop every 2 minutes; the
system's magnitude about the night's mean, plotted. With `meshes`, DART's
models at 49,152 facets rather than `scene.py`'s finest: 0.5 s an epoch
against 40 to 58 s.

Checked against `main.py`: 181 points, 118 mmag from brightest to faintest
with the spheroids and 218 with DART's shapes, as `main.py` has them; with
the spheroids the faintest point at 00:52, Dimorphos's shadow. On one
thread, 32 s for the spheroids, 90 s for DART's shapes; `main.py` does both
in 30 s, an epoch a thread. `main.py` loads `kernels.tm` now, the user's
name for `didymos.tm`.

Then, at the user's word, `main.py` removed: `lightcurve.py` in its place.
The night with both shape models side by side, above, is `main.py`'s.

The index arithmetic for the binary -- the bodies stacked, Dimorphos's
indices offset by Didymos's vertex count -- went when `flux` took the
bodies as two lists, at the user's word (`2026-10-21_flux_double_sum/`):
`flux(vertices, [mesh.indices for mesh in meshes], ...)`.

## `scene_lightcurve.py`, the two in one

The user: "is it possible to put lighcurve and scene in one script? the
lightcurve accumulate over the rendering and show the plot at the end".

`scene_lightcurve.py`: `scene.py`'s night, and each frame the bodies' light
from the same poses, `flux` submitted to a `ThreadPoolExecutor` -- it lets go
of Python's lock -- so that the drawing keeps its 30 frames a second; a
point a frame, 30 s of the night. At the night's end the scene pauses, and
once every point is summed the plot is written to
`out/lightcurve_didymos.png` and opened in the system's viewer. With
`meshes`, the finest models drawn and the 49,152-facet ones summed, posed
alike.

Not a plot window: a matplotlib window in a process where kalast's window
has opened takes it down on macOS, after `app.close()` too
(`2026-10-21_flux_double_sum/`). `lightcurve.py` writes and opens its plot
the same way, so that it shows one in the UI app too, where scripts get
`agg`.

Run in a background window, plot to scratch: 721 points, 118 mmag with the
spheroids and 218 with DART's shapes, as `lightcurve.py` has them. The night
drawn in 24.2 s and 24.6 s; the plot at 24.4 s with the spheroids, at 63.5 s
with DART's shapes -- 139 of their 721 points summed when the night ended.

## Open

- `lightcurve` and `Spin`, the single rotating body, have no example now;
  `docs/API.md` keeps the stretched `ico3` as their snippet.
- Didymos's size: `2026-10-21_didymos_size/`.
