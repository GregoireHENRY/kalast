#!/usr/bin/env python
import os
import sys

import numpy
import spiceypy as spice

import kalast.app
from kalast.app import App
from kalast.scattering import Hapke
from kalast.util import RPD

KERNELS = "examples/lightcurve_didymos/kernels.tm"

DIDYMOS = "/Users/gregoireh/data/mesh/didymos/g_01165mm_spc_didy_v003.obj"
DIMORPHOS = "/Users/gregoireh/data/mesh/dimorphos/g_00243mm_spc_dimo_v004.obj"

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

DT = 30.0  # seconds of the night a frame
DISTANCE = 20.0  # km: the camera on the line to the observatory, this far out

# Buratti et al. (2024), from DART's images and the system's phase curve.
LAW = Hapke(w=0.126, b=0.36, c=1.0, b0=0.97, h=0.020, theta_bar=18.0 * RPD, k=1.0)

app = App()
sim = app.simulation
sim.config.wireframe.mode = 0
sim.config.shading.srgb_mode = "linear"  # a pixel is exposure x I/F x 255
sim.config.light.exposure = 20.0
sim.config.huds = [kalast.app.Hud("")]
sim.state.rate_limited = True
sim.state.rate_limit = 30.0  # frames a second

stretch = [numpy.ones(3), numpy.ones(3)]

if SHAPES == "meshes":
    sim.load_mesh(path=DIDYMOS)
    sim.load_mesh(path=DIMORPHOS)
else:
    ico = "res/ico5.obj"
    sim.load_mesh(path=ico, smooth=True)
    sim.load_mesh(path=ico, smooth=True)
    stretch[:] = EXTENTS

for body in sim.bodies:
    body.scattering = LAW

sim.camera.projection.set_orthographic()
sim.camera.projection.side = 1.2

spice.furnsh(KERNELS)
radii = spice.bodvrd("EARTH", "RADII", 3)[1]
station = spice.georec(SITE[0] * RPD, SITE[1] * RPD, SITE[2], radii[0], (radii[0] - radii[2]) / radii[0])
impact = spice.str2et("2022-09-26T23:14:24.184")
et0 = spice.str2et(START)
etf = spice.str2et(END)

while app.running:
    if app.simulation.state.is_paused:
        app.step()
        continue

    et = et0 + DT * sim.state.iteration
    date = spice.et2utc(et, 'ISOC', 0)

    if et > etf:
        app.simulation.state.toggle_pause()

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
    for body, name, turn, axes in zip(sim.bodies, ("DIDYMOS", "DIMORPHOS"), turns, stretch):
        body.mat[:3, :3] = turn @ numpy.diag(axes)
        body.mat[:3, 3] = spice.spkpos(name, te, "J2000", "NONE", "DIDYMOS_BARYCENTER")[0]
    sim.sun.pos = spice.spkpos("SUN", te, "J2000", "NONE", "DIDYMOS_BARYCENTER")[0].tolist()

    # The camera on the line to the observatory, celestial north up.
    sim.camera.pos = (to_site * DISTANCE).tolist()
    sim.camera.dir = (-to_site).tolist()
    sim.camera.up = [0.0, 0.0, 1.0]
    sim.huds[0].text = f"TRAPPIST-North  {date} UTC  {SHAPES}"

    app.step()

spice.kclear()
