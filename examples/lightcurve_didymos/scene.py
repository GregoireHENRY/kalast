#!/usr/bin/env python
"""Didymos and Dimorphos as TRAPPIST-North saw them on the night of
2022-11-18, drawn by kalast: the scene whose light main.py sums. DART's
SPICE kernels place and turn the two bodies, the Sun lights them, and the
camera looks from the observatory, north up.

    python scene.py               # DART's shape models
    python scene.py ellipsoids    # Scheirich et al.'s spheroids

The night plays in a loop, 30 s of it a frame at 30 frames a second: 24 s
on screen. P pauses it.
"""

import os
import sys

import numpy
import spiceypy as spice

import kalast.app
from kalast.app import App
from kalast.scattering import Hapke
from kalast.util import RPD

HERE = os.path.dirname(os.path.abspath(__file__))
# Where these are kept here: set them to yours.
KERNELS = os.path.join(HERE, "didymos.tm")  # and its PATH_VALUES
DIDYMOS = "/Users/gregoireh/data/mesh/didymos/didymos_g_9309mm_spc_obj_0000n00000_v003.obj"
DIMORPHOS = "/Users/gregoireh/data/mesh/dimorphos/dimorphos_g_1940mm_spc_obj_0000n00000_v004.obj"

SHAPES = "ellipsoids" if "ellipsoids" in sys.argv else "meshes"
SITE = (-7.8664, 31.20622, 2.751)  # TRAPPIST-North: longitude, latitude (deg), altitude (km)
START, END = "2022-11-18T00:30", "2022-11-18T06:30"  # UTC
STEP = 30.0  # seconds of the night a frame
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

if SHAPES == "meshes":
    sim.load_mesh(path=DIDYMOS)
    sim.load_mesh(path=DIMORPHOS)
    stretch = [numpy.ones(3), numpy.ones(3)]
else:
    # A sphere stretched by its pose. Didymos 0.730 km across by volume, its
    # equator 1.369 times its pole; Dimorphos, after the impact, a sphere 0.202
    # of it.
    ico = os.path.join(HERE, "..", "..", "res", "ico5.obj")
    sim.load_mesh(path=ico, smooth=True)
    sim.load_mesh(path=ico, smooth=True)
    a = 0.5 * 0.730 * 1.369 ** (1 / 3)
    stretch = [numpy.array([a, a, a / 1.369]), numpy.full(3, 0.5 * 0.730 * 0.202)]
for body in sim.bodies:
    body.scattering = LAW

sim.camera.set_control_none()  # the script places it every frame
sim.camera.projection.fovy = 2 * numpy.arctan(1.2 / DISTANCE)  # 2.4 km top to bottom

spice.furnsh(KERNELS)
radii = spice.bodvrd("EARTH", "RADII", 3)[1]
station = spice.georec(SITE[0] * RPD, SITE[1] * RPD, SITE[2], radii[0], (radii[0] - radii[2]) / radii[0])
impact = spice.str2et("2022-09-26T23:14:24.184")
et0 = spice.str2et(START)
frames = int((spice.str2et(END) - et0) / STEP) + 1

while app.running:
    et = et0 + STEP * (sim.state.iteration % frames)

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
    sim.huds[0].text = f"TRAPPIST-North  {spice.et2utc(et, 'ISOC', 0)} UTC  {SHAPES}"

    if not app.step():
        break

spice.kclear()
