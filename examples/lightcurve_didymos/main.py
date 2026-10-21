#!/usr/bin/env python
"""Didymos and Dimorphos as an observatory on Earth saw them one night: their
light curve, and the mutual events in it -- one body shadowing or hiding the
other -- from DART's SPICE kernels and shape models and Buratti et al.
(2024)'s Hapke law.

    python main.py [meshes | ellipsoids] [--plot]

Each epoch, the two bodies are placed and turned as DART's kernels have them
when their light left for the site, and `kalast.lightcurve.flux` sums the
light of every facet: what of it the Sun lights and the site sees, exactly,
whatever the other body shadows or hides. Each body drawn alone too, the
drop the events make is the ratio.

- `meshes`: DART's shape models of the two, 49,152 facets each.
- `ellipsoids`: Scheirich et al. (2024)'s bodies, which predict the
  system's events: oblate spheroids of DART's volume and axis ratios,
  `res/ico5.obj` stretched, under the same law, so that the shapes alone
  differ. Turning about its axis, a spheroid has no rotation light curve:
  what is left is the events.

Both by default, side by side. The kernels and the shape models: README.md.
"""

import os
import sys
from concurrent.futures import ThreadPoolExecutor

import numpy
import spiceypy as spice

import kalast.mesh
from kalast.lightcurve import flux
from kalast.scattering import Hapke
from kalast.util import RPD

HERE = os.path.dirname(os.path.abspath(__file__))
# Where these are kept here: set them to yours.
KERNELS = os.path.join(HERE, "kernels.tm")  # and its PATH_VALUES
MESHES = (
    "/Users/gregoireh/data/mesh/didymos/didymos_g_9309mm_spc_obj_0000n00000_v003.obj",
    "/Users/gregoireh/data/mesh/dimorphos/dimorphos_g_1940mm_spc_obj_0000n00000_v004.obj",
)

# TRAPPIST-North, Oukaimeden, Morocco (MPC Z53): longitude, latitude (deg),
# altitude (km); a night of it, every 2 minutes, UTC.
SITE = (-7.8664, 31.20622, 2.751)
START, END, STEP = "2022-11-18T00:30", "2022-11-18T06:30", 120.0
# Buratti et al. (2024), from DART's images and the system's phase curve.
LAW = Hapke(w=0.126, b=0.36, c=1.0, b0=0.97, h=0.020, theta_bar=18.0 * RPD, k=1.0)
SHAPES = [s for s in ("meshes", "ellipsoids") if s in sys.argv] or ["meshes", "ellipsoids"]

spice.furnsh(KERNELS)
IMPACT = spice.str2et("2022-09-26T23:14:24.184")
radii = spice.bodvrd("EARTH", "RADII", 3)[1]
station = spice.georec(SITE[0] * RPD, SITE[1] * RPD, SITE[2], radii[0], (radii[0] - radii[2]) / radii[0])


def bodies(shapes, after_impact):
    """Didymos's and Dimorphos's vertices (km, in their own frames) and
    facets."""
    if shapes == "meshes":
        return [(m.positions, m.indices) for m in (kalast.mesh.Mesh(path=p) for p in MESHES)]
    # Scheirich et al. (2024): Didymos of DART's volume, 0.730 km across, its
    # equator 1.369 times its pole; Dimorphos 0.223 of it and 1.504 before
    # the impact, 0.202 and a sphere after.
    ico = kalast.mesh.Mesh(path=os.path.join(HERE, "..", "..", "res", "ico5.obj"))

    def spheroid(diameter, ratio):
        a = 0.5 * diameter * ratio ** (1 / 3)
        return ico.positions * [a, a, a / ratio], ico.indices

    return [spheroid(0.730, 1.369), spheroid(0.730 * 0.202, 1.0) if after_impact else spheroid(0.730 * 0.223, 1.504)]


def placed(et):
    """At `et`, as the site saw it: Didymos's and Dimorphos's turns and places
    about the system's barycentre (ecliptic J2000, km), and the directions
    of the Sun and of the site -- when the light left."""
    state, lt = spice.spkcpo("DIDYMOS_BARYCENTER", et, "ECLIPJ2000", "OBSERVER", "CN", station, "EARTH", "IAU_EARTH")
    te = et - lt
    # DART's kernel turns Didymos at 2.2600 h. Its sidereal period is
    # 2.2603891 h before the impact and 2.260440 h after (Durech et al. 2025):
    # the kernel's phase kept at the impact, the true period from there.
    hours = (te - IMPACT) / 3600
    spin = 2 * numpy.pi * hours * (1 / (2.2603891 if hours < 0 else 2.260440) - 1 / 2.26)
    turns = [
        numpy.array(spice.pxform("DIDYMOS_FIXED", "ECLIPJ2000", te)) @ spice.axisar([0.0, 0.0, 1.0], spin),
        numpy.array(spice.pxform("DIMORPHOS_FIXED", "ECLIPJ2000", te)),
    ]
    places = [spice.spkpos(b, te, "ECLIPJ2000", "NONE", "DIDYMOS_BARYCENTER")[0] for b in ("DIDYMOS", "DIMORPHOS")]
    sun = numpy.array(spice.spkpos("SUN", te, "ECLIPJ2000", "NONE", "DIDYMOS_BARYCENTER")[0])
    return turns, places, sun / numpy.linalg.norm(sun), -numpy.array(state[:3]) / numpy.linalg.norm(state[:3])


def light(two, turns, places, sun, site):
    """The system's flux, then Didymos's and Dimorphos's alone."""
    v = [p @ r.T + c for (p, _), r, c in zip(two, turns, places)]
    facets = [f for _, f in two]
    both = flux(numpy.vstack(v), numpy.concatenate([facets[0], facets[1] + len(v[0])]), sun, site, LAW).flux
    return [both] + [flux(v[k], facets[k], sun, site, LAW).flux for k in (0, 1)]


# The geometry first, on one thread: SPICE is not thread-safe. Then the
# light, an epoch a thread: `flux` lets go of Python's lock.
ets = numpy.arange(spice.str2et(START), spice.str2et(END) + 1.0, STEP)
geometry = [placed(et) for et in ets]
utc = [spice.et2utc(et, "ISOC", 0)[11:16] for et in ets]
hours = (ets - spice.str2et(START[:10])) / 3600  # UTC
curves = {}
for shapes in SHAPES:
    two = bodies(shapes, ets[0] > IMPACT)
    with ThreadPoolExecutor() as pool:
        f = numpy.array(list(pool.map(lambda g: light(two, *g), geometry)))
    # The system's magnitude about its mean, and what the events take.
    curves[shapes] = (-2.5 * numpy.log10(f[:, 0] / f[:, 0].mean()), 2.5 * numpy.log10((f[:, 1] + f[:, 2]) / f[:, 0]))
spice.kclear()

alpha = numpy.degrees(numpy.arccos(geometry[0][2] @ geometry[0][3]))
print(f"Didymos and Dimorphos from TRAPPIST-North, {START[:10]}, phase {alpha:.1f} deg: magnitude about the night's mean, and the events' drop (mmag)")
print(" UTC   " + "".join(f"{s:>22}" for s in curves))
for k in range(0, len(ets), 5):
    print(f"{utc[k]} " + "".join(f"{m[k] * 1000:+12.1f}{d[k] * 1000:10.1f}" for m, d in curves.values()))
for s, (m, d) in curves.items():
    k = numpy.argmax(d)
    print(f"{s}: amplitude {(m.max() - m.min()) * 1000:.0f} mmag; deepest event {d[k] * 1000:.0f} mmag at {utc[k]}")

if "--plot" in sys.argv:
    import matplotlib.pyplot as plt

    for (s, (m, d)), colour in zip(curves.items(), ("C0", "C1")):
        plt.plot(hours, m * 1000, colour, label=f"{s}: the system")
        plt.plot(hours, d * 1000, colour, linestyle="--", label=f"{s}: the events' drop")
    plt.gca().invert_yaxis()  # brighter up
    plt.xlabel("UTC (h)")
    plt.ylabel("mmag")
    plt.title(f"Didymos and Dimorphos from TRAPPIST-North, {START[:10]}")
    plt.legend()
    plt.show()
