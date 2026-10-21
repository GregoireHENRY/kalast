#!/usr/bin/env python
import os
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

import matplotlib
import numpy
import spiceypy as spice

import kalast.app
import kalast.mesh
from kalast.app import App
from kalast.lightcurve import flux
from kalast.scattering import Hapke
from kalast.util import RPD

# No plot window beside kalast's: drawn to a file, opened at the night's end.
matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402

KERNELS = "examples/lightcurve_didymos/kernels.tm"

# Drawn at their finest.
DIDYMOS = "/Users/gregoireh/data/mesh/didymos/g_01165mm_spc_didy_v003.obj"
DIMORPHOS = "/Users/gregoireh/data/mesh/dimorphos/g_00243mm_spc_dimo_v004.obj"
# Their light from the same models at 49,152 facets: 0.5 s a frame, where the
# finest take 40 to 58 s.
DIDYMOS_LIGHT = "/Users/gregoireh/data/mesh/didymos/didymos_g_9309mm_spc_obj_0000n00000_v003.obj"
DIMORPHOS_LIGHT = "/Users/gregoireh/data/mesh/dimorphos/dimorphos_g_1940mm_spc_obj_0000n00000_v004.obj"

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

DT = 30.0  # seconds of the night a frame, and between two points of the light curve
DISTANCE = 20.0  # km: the camera on the line to the observatory, this far out

# Buratti et al. (2024), from DART's images and the system's phase curve.
LAW = Hapke(w=0.126, b=0.36, c=1.0, b0=0.97, h=0.020, theta_bar=18.0 * RPD, k=1.0)

PLOT = "out/lightcurve_didymos.png"

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
    meshes = [kalast.mesh.Mesh(path=DIDYMOS_LIGHT), kalast.mesh.Mesh(path=DIMORPHOS_LIGHT)]
else:
    ico = "res/ico5.obj"
    sim.load_mesh(path=ico, smooth=True)
    sim.load_mesh(path=ico, smooth=True)
    stretch[:] = EXTENTS
    meshes = [kalast.mesh.Mesh(path=ico), kalast.mesh.Mesh(path=ico)]

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
midnight = spice.str2et(START[:10])

# The light curve, made as the night is drawn: each frame's light on a thread
# of its own, `flux` letting go of Python's lock, so the drawing keeps its pace.
pool = ThreadPoolExecutor()
times, points = [], []
plotted = False
et = et0

while app.running:
    # The night drawn and all its light summed: the light curve, once.
    if not plotted and et > etf and all(p.done() for p in points):
        fluxes = numpy.array([p.result().flux for p in points])
        magnitude = -2.5 * numpy.log10(fluxes / fluxes.mean())  # about the night's mean
        plt.plot((numpy.array(times) - midnight) / 3600, magnitude * 1000, ".-")
        plt.gca().invert_yaxis()  # brighter up
        plt.xlabel("UTC (h)")
        plt.ylabel("mmag")
        plt.title(f"Didymos and Dimorphos from TRAPPIST-North, {START[:10]}, {SHAPES}")
        os.makedirs(os.path.dirname(PLOT), exist_ok=True)
        plt.savefig(PLOT, dpi=120)
        print(f"wrote {PLOT}")
        if sys.platform == "win32":
            os.startfile(PLOT)
        else:
            subprocess.Popen(["open" if sys.platform == "darwin" else "xdg-open", PLOT])
        plotted = True

    if app.simulation.state.is_paused:
        app.step()
        continue

    et = et0 + DT * sim.state.iteration
    date = spice.et2utc(et, 'ISOC', 0)

    if et > etf:
        app.simulation.state.toggle_pause()
        app.step()
        continue

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
    for body, mesh, name, turn, axes in zip(sim.bodies, meshes, ("DIDYMOS", "DIMORPHOS"), turns, stretch):
        pose = turn @ numpy.diag(axes)
        place = spice.spkpos(name, te, "J2000", "NONE", "DIDYMOS_BARYCENTER")[0]
        body.mat[:3, :3] = pose
        body.mat[:3, 3] = place
        vertices.append(mesh.positions @ pose.T + place)
    to_sun = numpy.array(spice.spkpos("SUN", te, "J2000", "NONE", "DIDYMOS_BARYCENTER")[0])
    sim.sun.pos = to_sun.tolist()

    # The light the observatory gets: every facet of the two, as lit and as
    # seen, each body shadowing and hiding the other.
    times.append(et)
    points.append(pool.submit(flux, vertices, [mesh.indices for mesh in meshes], to_sun, to_site, LAW))

    # The camera on the line to the observatory, celestial north up.
    sim.camera.pos = (to_site * DISTANCE).tolist()
    sim.camera.dir = (-to_site).tolist()
    sim.camera.up = [0.0, 0.0, 1.0]
    sim.huds[0].text = f"TRAPPIST-North  {date} UTC  {SHAPES}"

    app.step()

pool.shutdown(cancel_futures=True)
spice.kclear()
