#!/usr/bin/env python
"""What AFC saw: Didymos and Dimorphos through the Asteroid Framing Camera, at
the times of the TIRI images decoded by edds_decoder -- or, without any, from
START to END every DT. A frame a time, written to FRAMES."""
import glob

import numpy
import spiceypy as spice
from astropy.io import fits

from kalast.app import App, Hud
from kalast.scattering import Hapke
from kalast.util import DPR, RPD

# TIRI's images as edds_decoder writes them, AFC drawn at each one's DATE-OBS
# (SPI_TIME without the TC report).
IMAGES = "/Users/gregoireh/projects/edds_decoder/out/data_par/asteroid"
START, END, DT = "2026-11-05 00:00:00 UTC", "2027-04-30 00:00:00 UTC", 15 * 60.0

KERNELS = "/Users/gregoireh/data/spice/hera/kernels/mk"
DIDYMOS = "/Users/gregoireh/data/mesh/didymos/g_01165mm_spc_didy_v003_100k.obj"
DIMORPHOS = "/Users/gregoireh/data/mesh/dimorphos/g_00243mm_spc_dimo_v004_100k.obj"
FRAMES = "out/hera_didymos/afc"
# Dimorphos where TIRI's images put it: where the ephemeris has it this much
# later, 120 deg further along its orbit (tiri.py measures it). 0 for the
# ephemeris as it is.
DIMORPHOS_SHIFT = 13689.0  # s

# Buratti et al. (2024), from DART's images and the system's phase curve.
LAW = Hapke(w=0.126, b=0.36, c=1.0, b0=0.97, h=0.020, theta_bar=18.0 * RPD, k=1.0)
INSTR = "HERA_AFC-1"

spice.kclear()
spice.furnsh(f"{KERNELS}/hera_plan.tm")  # Didymos and Dimorphos
spice.furnsh(f"{KERNELS}/hera_ops.tm")  # Hera's measured attitude, over the plan's
_, _, _, _, bounds = spice.getfov(spice.bodn2c(INSTR), 4)
fovy = 2.0 * numpy.arctan2(abs(bounds[0][1]), bounds[0][2])
ifov = spice.gdpool(f"INS{spice.bodn2c(INSTR)}_IFOV", 0, 1)[0]  # rad a pixel
pixels = int(spice.gipool(f"INS{spice.bodn2c(INSTR)}_PIXEL_SAMPLES", 0, 1)[0])

headers = [fits.getheader(p) for p in glob.glob(f"{IMAGES}/**/tiri_par_*.fits", recursive=True)]
times = (sorted({spice.str2et(h.get("DATE-OBS", h["SPI_TIME"])) for h in headers if h["FW_NUM"] != "CLOSE"})
         or numpy.arange(spice.str2et(START), spice.str2et(END), DT))
print(f"{len(times)} times, {spice.et2utc(times[0], 'ISOC', 0)} to {spice.et2utc(times[-1], 'ISOC', 0)}")

app = App()
sim = app.simulation
sim.config.image.width = sim.config.image.height = pixels
sim.config.axes.style = "off"
sim.config.wireframe.mode = 0
sim.config.shading.srgb_mode = "linear"
sim.config.light.exposure = 20.0
sim.config.export.dir = FRAMES
sim.huds = [Hud("", size=16)]

# The scene in AFC's frame: the camera at its origin along its boresight, +X up
# as in AFC's own images.
sim.camera.pos = [0.0, 0.0, 0.0]
sim.camera.dir = [0.0, 0.0, 1.0]
sim.camera.up = [1.0, 0.0, 0.0]
sim.camera.projection.fovy = fovy

sim.load_mesh(path=DIDYMOS)
sim.load_mesh(path=DIMORPHOS)
for body in sim.bodies:
    body.scattering = LAW

while app.running:
    if sim.state.is_paused:
        app.step()
        continue

    i = sim.state.iteration
    et = times[min(i, len(times) - 1)]
    if i >= len(times) - 1:
        sim.state.toggle_pause()  # the last time drawn, then paused

    to_afc = spice.pxform("J2000", INSTR, et)
    didymos = spice.spkpos("DIDYMOS", et, INSTR, "LT+S", INSTR)[0]
    sim.bodies[0].mat[:3, :3] = spice.pxform("DIDYMOS_FIXED", INSTR, et)
    sim.bodies[0].mat[:3, 3] = didymos
    sim.bodies[1].mat[:3, :3] = to_afc @ spice.pxform("DIMORPHOS_FIXED", "J2000", et + DIMORPHOS_SHIFT)
    sim.bodies[1].mat[:3, 3] = didymos + to_afc @ spice.spkpos("DIMORPHOS", et + DIMORPHOS_SHIFT, "J2000", "NONE", "DIDYMOS")[0]
    sun = spice.spkpos("SUN", et, INSTR, "LT+S", INSTR)[0]
    sim.sun.pos = sun

    distance = [numpy.linalg.norm(body.mat[:3, 3]) for body in sim.bodies]
    phase = spice.vsep(sun - sim.bodies[0].mat[:3, 3], -sim.bodies[0].mat[:3, 3]) * DPR
    sim.huds[0].text = (f"{spice.et2utc(et, 'ISOC', 1)}  Didymos {distance[0]:.1f} km  "
                        f"Dimorphos {distance[1]:.1f} km  phase {phase:.1f}°  "
                        f"{distance[0] * ifov * 1e3:.0f} m/px")

    sim.export_once()
    app.step()

spice.kclear()
