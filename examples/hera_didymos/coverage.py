#!/usr/bin/env python
"""Hera's view of Didymos and Dimorphos at the times of the TIRI images decoded
by edds_decoder: the distances, the phase angle, the sub-spacecraft points and
the resolution of AFC and TIRI there (geometry.png), and the ground each saw
(coverage.png).

Ground is seen where the frames drawn from Hera have it -- each body framed in
turn, so that its facets are resolved -- and inside the instrument's field as
it was pointed. AFC's must be lit; TIRI sees the night side too."""
import glob
import os
import subprocess
import sys

import matplotlib
import numpy
import spiceypy as spice
from astropy.io import fits

import kalast.tiri_alignment as alignment
from kalast.app import App
from kalast.scattering import Hapke
from kalast.util import DPR, RPD

matplotlib.use("Agg")  # no plot window beside kalast's: drawn to a file
import matplotlib.pyplot as plt  # noqa: E402

IMAGES = "/Users/gregoireh/projects/edds_decoder/out/data_par/asteroid"
KERNELS = "/Users/gregoireh/data/spice/hera/kernels/mk"
MESHES = {
    "DIDYMOS": "/Users/gregoireh/data/mesh/didymos/g_01165mm_spc_didy_v003_100k.obj",
    "DIMORPHOS": "/Users/gregoireh/data/mesh/dimorphos/g_00243mm_spc_dimo_v004_100k.obj",
}
OUT = "out/hera_didymos/coverage"
INSTRUMENTS = {"HERA_AFC-1": 1020, "HERA_TIRI": 768}  # and their detectors' rows
ALIGN = True  # TIRI's boresight as measured in flight, kalast.tiri_alignment

spice.kclear()
spice.furnsh(f"{KERNELS}/hera_plan.tm")  # Didymos and Dimorphos
spice.furnsh(f"{KERNELS}/hera_ops.tm")  # Hera's measured attitude, over the plan's

# Each instrument's field, its half-angles' tangents, and a pixel's angle at its centre.
field, ifov = {}, {}
for instr, lines in INSTRUMENTS.items():
    _, _, _, _, bounds = spice.getfov(spice.bodn2c(instr), 4)
    field[instr] = (abs(bounds[0][0] / bounds[0][2]), abs(bounds[0][1] / bounds[0][2]))
    ifov[instr] = 2.0 * field[instr][1] / lines
turn = {"HERA_AFC-1": numpy.eye(3), "HERA_TIRI": alignment.rotation() if ALIGN else numpy.eye(3)}

headers = [fits.getheader(p) for p in glob.glob(f"{IMAGES}/**/tiri_par_*.fits", recursive=True)]
times = sorted({spice.str2et(h.get("DATE-OBS", h["SPI_TIME"])) for h in headers if h["FW_NUM"] != "CLOSE"})
print(f"{len(times)} times, {spice.et2utc(times[0], 'ISOC', 0)} to {spice.et2utc(times[-1], 'ISOC', 0)}")

app = App()
sim = app.simulation
sim.config.image.width = sim.config.image.height = 1024
sim.config.axes.style = "off"
sim.config.wireframe.mode = 0
sim.config.shadows.access_shadow_map = True  # what is lit, both bodies
for path in MESHES.values():
    sim.load_mesh(path=path)
for body in sim.bodies:
    body.scattering = Hapke(w=0.126, b=0.36, c=1.0, b0=0.97, h=0.020, theta_bar=18.0 * RPD, k=1.0)

# The facets in each body's own frame: centres, and where they lie on it.
centres = [numpy.array([f.pos for f in body.mesh.facets]) for body in sim.bodies]
places = [numpy.array([spice.reclat(c)[1:] for c in cs]) * DPR for cs in centres]
seen = {(instr, b): numpy.zeros(len(centres[b]), int) for instr in INSTRUMENTS for b in range(len(sim.bodies))}

rows, k = [], 0
while app.running and k < 2 * len(times):
    et, b = times[k // 2], k % 2  # each time, each body framed in turn
    hera = spice.spkpos("HERA", et, "J2000", "LT+S", "DIDYMOS")[0]
    for body, name in zip(sim.bodies, MESHES):
        body.mat[:3, :3] = spice.pxform(f"{name}_FIXED", "J2000", et)
        body.mat[:3, 3] = spice.spkpos(name, et, "J2000", "LT+S", "DIDYMOS")[0]
    sim.sun.pos = spice.spkpos("SUN", et, "J2000", "LT+S", "DIDYMOS")[0]
    toward = sim.bodies[b].mat[:3, 3] - hera
    sim.camera.pos = hera
    sim.camera.dir = toward
    sim.camera.up = [0.0, 0.0, 1.0]
    sim.camera.projection.fovy = 2.6 * max(numpy.abs(centres[b]).max(axis=0)) / numpy.linalg.norm(toward)
    sim.request_facet_id()
    if not app.step():
        break

    # Drawn from Hera, lit, and inside each instrument's field as pointed.
    ids, offsets = sim.facet_id_map()
    n = len(centres[b])
    drawn = numpy.zeros(n, bool)
    drawn[ids[(ids > offsets[b]) & (ids <= offsets[b] + n)] - offsets[b] - 1] = True
    light = sim.facet_illumination(b)
    lit = (sim.facet_incidence(b) if light is None else light) > 0.0
    world = centres[b] @ sim.bodies[b].mat[:3, :3].T + sim.bodies[b].mat[:3, 3] - hera
    for instr in INSTRUMENTS:
        v = world @ (turn[instr] @ spice.pxform("J2000", instr, et)).T
        inside = (v[:, 2] > 0) & (abs(v[:, 0]) <= field[instr][0] * v[:, 2]) & (abs(v[:, 1]) <= field[instr][1] * v[:, 2])
        seen[instr, b] += drawn & inside & (lit if instr == "HERA_AFC-1" else True)

    if b == 1:
        # The geometry: Hera to each body, the phase angle, the points under Hera.
        row = [et]
        for name in MESHES:
            p = spice.spkpos(name, et, "J2000", "LT+S", "HERA")[0]
            point = spice.subpnt("INTERCEPT/ELLIPSOID", name, et, f"{name}_FIXED", "LT+S", "HERA")[0]
            _, lon, lat = spice.reclat(point)
            row += [numpy.linalg.norm(p), lat * DPR, lon * DPR % 360.0]
        sun = spice.spkpos("SUN", et, "J2000", "LT+S", "DIDYMOS")[0]
        row.append(spice.vsep(sun, hera) * DPR)
        rows.append(row)
    k += 1

os.makedirs(OUT, exist_ok=True)
g = numpy.array(rows)
hours = (g[:, 0] - g[0, 0]) / 3600.0
start = spice.et2utc(g[0, 0], "ISOC", 0)
header = "utc,didymos_km,didymos_lat,didymos_lon,dimorphos_km,dimorphos_lat,dimorphos_lon,phase_deg"
numpy.savetxt(f"{OUT}/geometry.csv", numpy.column_stack([[spice.et2utc(t, "ISOC", 3) for t in g[:, 0]], g[:, 1:]]),
              fmt="%s", delimiter=",", header=header, comments="")

fig, ax = plt.subplots(4, 1, figsize=(10, 10), sharex=True)
ax[0].plot(hours, g[:, 1], "o-", ms=3, label="Didymos")
ax[0].plot(hours, g[:, 4], "s-", ms=3, label="Dimorphos")
ax[0].set_ylabel("Hera's distance (km)")
ax[0].legend()
ax[1].plot(hours, g[:, 7], "o-", ms=3, color="k")
ax[1].set_ylabel("phase angle (°)")
for col, mark, name in ((2, "o", "Didymos"), (5, "s", "Dimorphos")):
    ax[2].plot(hours, g[:, col], mark, ms=3, label=f"{name}, latitude")
    ax[2].plot(hours, g[:, col + 1], mark, ms=3, mfc="none", label=f"{name}, longitude (east)")
ax[2].set_ylabel("sub-spacecraft point (°)")
ax[2].legend(fontsize=8, ncol=2)
for instr in INSTRUMENTS:
    ax[3].plot(hours, g[:, 1] * ifov[instr] * 1e3, "o-", ms=3, label=f"{instr[5:]}, {ifov[instr] * 1e6:.1f} µrad")
ax[3].set_ylabel("resolution at Didymos (m/px)")
ax[3].set_xlabel(f"hours from {start} UTC")
ax[3].legend()
ax[0].set_title(f"Hera at Didymos, the {len(times)} TIRI images' times")
fig.tight_layout()
fig.savefig(f"{OUT}/geometry.png", dpi=110)

fig, ax = plt.subplots(2, 2, figsize=(13, 7.5), sharex=True, sharey=True)
for i, instr in enumerate(INSTRUMENTS):
    for b, name in enumerate(MESHES):
        area = numpy.array([f.area for f in sim.bodies[b].mesh.facets])
        count = seen[instr, b]
        a = ax[b, i]
        art = a.scatter(places[b][:, 0] % 360.0, places[b][:, 1], c=numpy.where(count > 0, count, numpy.nan),
                        s=0.3, cmap="viridis", vmin=1, rasterized=True)
        a.set_facecolor("0.85")
        a.set_title(f"{name.capitalize()} by {instr[5:]}: {100 * area[count > 0].sum() / area.sum():.0f}% of the surface",
                    fontsize=10)
        fig.colorbar(art, ax=a, label=f"seen at how many of the {len(times)} image times")
        a.plot(g[:, 3 * b + 3], g[:, 3 * b + 2], "+", color="r", ms=7, mew=1.2, label="sub-spacecraft point")
ax[0, 0].legend(loc="upper right", fontsize=8)
for a in ax[:, 0]:
    a.set_ylabel("latitude (°)")
for a in ax[1]:
    a.set_xlabel("east longitude (°)")
ax[0, 0].set_xlim(0, 360)
ax[0, 0].set_ylim(-90, 90)
fig.suptitle(f"Ground seen, {start} to {spice.et2utc(times[-1], 'ISOC', 0)} UTC: in the field, facing Hera, lit for AFC")
fig.tight_layout()
fig.savefig(f"{OUT}/coverage.png", dpi=110)
print(f"wrote {OUT}/")
for path in (f"{OUT}/geometry.png", f"{OUT}/coverage.png"):
    if sys.platform == "win32":
        os.startfile(path)
    else:
        subprocess.Popen(["open" if sys.platform == "darwin" else "xdg-open", path])

spice.kclear()
