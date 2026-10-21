#!/usr/bin/env python
import matplotlib.pyplot as plt
import numpy
import spiceypy as spice

import kalast.mesh
from kalast.lightcurve import flux
from kalast.scattering import Hapke
from kalast.util import RPD

KERNELS = "examples/lightcurve_didymos/kernels.tm"

# DART's shape models at 49,152 facets: 0.3 s an epoch, where the finest
# (3,145,728 facets, scene.py's) take 40 to 58 s.
DIDYMOS = "/Users/gregoireh/data/mesh/didymos/didymos_g_9309mm_spc_obj_0000n00000_v003.obj"
DIMORPHOS = "/Users/gregoireh/data/mesh/dimorphos/dimorphos_g_1940mm_spc_obj_0000n00000_v004.obj"

# Ellipsoids extents matching diameter equivalent sphere same volume as meshes above
# Dimorphos assumed sphere post impact due to tumbling not characterized.
EXTENTS = [
    [0.4053, 0.4053, 0.2960],
    [0.0737, 0.0737, 0.0737]
]

# options:
# - ellipsoids
# - meshes
SHAPES = "ellipsoids"

SITE = (-7.8664, 31.20622, 2.751)  # TRAPPIST-North: longitude, latitude (deg), altitude (km)
START, END = "2022-11-18T00:30", "2022-11-18T06:30"  # UTC

DT = 120.0  # seconds of the night between two points of the light curve

# Buratti et al. (2024), from DART's images and the system's phase curve.
LAW = Hapke(w=0.126, b=0.36, c=1.0, b0=0.97, h=0.020, theta_bar=18.0 * RPD, k=1.0)

if SHAPES == "meshes":
    meshes = [kalast.mesh.Mesh(path=DIDYMOS), kalast.mesh.Mesh(path=DIMORPHOS)]
    stretch = [numpy.ones(3), numpy.ones(3)]
else:
    ico = kalast.mesh.Mesh(path="res/ico5.obj")
    meshes = [ico, ico]
    stretch = EXTENTS

# The two bodies as one mesh for `flux`: Dimorphos's facets after Didymos's vertices.
indices = numpy.concatenate([meshes[0].indices, meshes[1].indices + len(meshes[0].positions)])

spice.furnsh(KERNELS)
radii = spice.bodvrd("EARTH", "RADII", 3)[1]
station = spice.georec(SITE[0] * RPD, SITE[1] * RPD, SITE[2], radii[0], (radii[0] - radii[2]) / radii[0])
impact = spice.str2et("2022-09-26T23:14:24.184")
et0 = spice.str2et(START)
etf = spice.str2et(END)
midnight = spice.str2et(START[:10])

ets = numpy.arange(et0, etf + 1.0, DT)
fluxes = []
for et in ets:
    # Where the system is from the observatory, and when its light left it.
    state, light_time = spice.spkcpo("DIDYMOS_BARYCENTER", et, "J2000", "OBSERVER", "CN", station, "EARTH", "IAU_EARTH")
    te = et - light_time
    to_site = -numpy.array(state[:3]) / numpy.linalg.norm(state[:3])

    # The bodies about the system's barycentre. DART's kernel turns Didymos
    # at 2.2600 h; its true period is 2.2603891 h before the impact and
    # 2.260440 h after (Durech et al. 2025).
    hours = (te - impact) / 3600
    spin = 2 * numpy.pi * hours * (1 / (2.2603891 if hours < 0 else 2.260440) - 1 / 2.26)
    turns = [
        numpy.array(spice.pxform("DIDYMOS_FIXED", "J2000", te)) @ spice.axisar([0.0, 0.0, 1.0], spin),
        numpy.array(spice.pxform("DIMORPHOS_FIXED", "J2000", te)),
    ]
    vertices = []
    for mesh, name, turn, axes in zip(meshes, ("DIDYMOS", "DIMORPHOS"), turns, stretch):
        place = spice.spkpos(name, te, "J2000", "NONE", "DIDYMOS_BARYCENTER")[0]
        vertices.append(mesh.positions @ (turn @ numpy.diag(axes)).T + place)
    to_sun = numpy.array(spice.spkpos("SUN", te, "J2000", "NONE", "DIDYMOS_BARYCENTER")[0])
    to_sun /= numpy.linalg.norm(to_sun)

    # The light the observatory gets: every facet's, as lit and as seen, with
    # what each body shadows and hides of the other.
    fluxes.append(flux(numpy.vstack(vertices), indices, to_sun, to_site, LAW).flux)

spice.kclear()

# The system's magnitude about the night's mean.
magnitude = -2.5 * numpy.log10(numpy.array(fluxes) / numpy.mean(fluxes))
utc = (ets - midnight) / 3600

plt.plot(utc, magnitude * 1000, ".-")
plt.gca().invert_yaxis()  # brighter up
plt.xlabel("UTC (h)")
plt.ylabel("mmag")
plt.title(f"Didymos and Dimorphos from TRAPPIST-North, {START[:10]}, {SHAPES}")
plt.show()
