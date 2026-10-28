#!/usr/bin/env python
"""Hera's view of Didymos and Dimorphos at the times of AFC's and TIRI's images
of them: the distances, the phase angle, the sub-spacecraft points and the
resolution of AFC and TIRI there (geometry.png), and the ground each saw
(coverage.png).

The images are those where afc.py and tiri.py found Didymos: run them first.
Ground is seen where the frames drawn from Hera have it -- each body framed in
turn, so that its facets are resolved -- and inside the instrument's field as
the image shows it pointed: SPICE's pointing moved by the error the two
scripts measured. AFC's must be lit; TIRI sees the night side too."""
import os
import subprocess
import sys

import matplotlib
import numpy
import spiceypy as spice

import kalast.tiri_alignment as alignment
from kalast.app import App
from kalast.scattering import Hapke
from kalast.util import DPR, RPD

matplotlib.use("Agg")  # no plot window beside kalast's: drawn to a file
import matplotlib.pyplot as plt  # noqa: E402

KERNELS = "/Users/gregoireh/data/spice/hera/kernels/mk"
MESHES = {
    "DIDYMOS": "/Users/gregoireh/data/mesh/didymos/g_01165mm_spc_didy_v003_100k.obj",
    "DIMORPHOS": "/Users/gregoireh/data/mesh/dimorphos/g_00243mm_spc_dimo_v004_100k.obj",
}
OUT = "out/hera_didymos/coverage"
# Each instrument's images where Didymos was found, as afc.py and tiri.py
# write them; its detector's rows; how the products' columns and rows lie in
# its frame (X, Y from x, y).
INSTRUMENTS = {
    "HERA_AFC-1": ("out/hera_didymos/afc/afc.csv", 1020, [[0.0, -1.0], [1.0, 0.0]]),
    "HERA_TIRI": ("out/hera_didymos/tiri/tiri.csv", 768, [[-1.0, 0.0], [0.0, -1.0]]),
}
ALIGN = True  # TIRI's boresight as measured in flight, kalast.tiri_alignment
# Didymos turned as its PCK has it this much later (afc.py); Dimorphos where
# the ephemeris has it this much later (tiri.py). 0 for the kernels as they are.
DIDYMOS_SHIFT = 5990.0  # s
DIMORPHOS_SHIFT = 13689.0  # s

spice.kclear()
spice.furnsh(f"{KERNELS}/hera_plan.tm")  # Didymos and Dimorphos
spice.furnsh(f"{KERNELS}/hera_ops.tm")  # Hera's measured attitude, over the plan's

# Each instrument's field, its half-angles' tangents, and a pixel's angle at
# its centre. Its images: the time, and the pointing's error there as an angle
# in its frame.
field, ifov, shots = {}, {}, []
for instr, (found, lines, axes) in INSTRUMENTS.items():
    _, _, _, _, bounds = spice.getfov(spice.bodn2c(instr), 4)
    field[instr] = (abs(bounds[0][0] / bounds[0][2]), abs(bounds[0][1] / bounds[0][2]))
    ifov[instr] = 2.0 * field[instr][1] / lines
    for row in numpy.atleast_1d(numpy.genfromtxt(found, delimiter=",", names=True, dtype=None, encoding=None)):
        shots.append((spice.str2et(row["utc"]), instr, numpy.array(axes) @ [row["pointing_x"], row["pointing_y"]] * ifov[instr]))
shots.sort(key=lambda shot: shot[0])
turn = {"HERA_AFC-1": numpy.eye(3), "HERA_TIRI": alignment.rotation() if ALIGN else numpy.eye(3)}
taken = {instr: sum(shot[1] == instr for shot in shots) for instr in INSTRUMENTS}
times = [shot[0] for shot in shots]
print(f"{taken} images, {spice.et2utc(times[0], 'ISOC', 0)} to {spice.et2utc(times[-1], 'ISOC', 0)}")

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
while app.running and k < 2 * len(shots):
    (et, instr, off), b = shots[k // 2], k % 2  # each image, each body framed in turn
    hera = spice.spkpos("HERA", et, "J2000", "LT+S", "DIDYMOS")[0]
    for body, name, s, t in zip(sim.bodies, MESHES, (DIDYMOS_SHIFT, DIMORPHOS_SHIFT), (0.0, DIMORPHOS_SHIFT)):
        body.mat[:3, :3] = spice.pxform(f"{name}_FIXED", "J2000", et + s)
        body.mat[:3, 3] = spice.spkpos(name, et + t, "J2000", "NONE", "DIDYMOS")[0]
    sim.sun.pos = spice.spkpos("SUN", et, "J2000", "LT+S", "DIDYMOS")[0]
    toward = sim.bodies[b].mat[:3, 3] - hera
    sim.camera.pos = hera
    sim.camera.dir = toward
    sim.camera.up = [0.0, 0.0, 1.0]
    sim.camera.projection.fovy = 2.6 * max(numpy.abs(centres[b]).max(axis=0)) / numpy.linalg.norm(toward)
    sim.request_facet_id()
    if not app.step():
        break

    # Drawn from Hera, lit, and inside the instrument's field as the image
    # shows it pointed.
    ids, offsets = sim.facet_id_map()
    n = len(centres[b])
    drawn = numpy.zeros(n, bool)
    drawn[ids[(ids > offsets[b]) & (ids <= offsets[b] + n)] - offsets[b] - 1] = True
    light = sim.facet_illumination(b)
    lit = (sim.facet_incidence(b) if light is None else light) > 0.0
    world = centres[b] @ sim.bodies[b].mat[:3, :3].T + sim.bodies[b].mat[:3, 3] - hera
    v = world @ (turn[instr] @ spice.pxform("J2000", instr, et)).T
    x, y = v[:, 0] / v[:, 2] + off[0], v[:, 1] / v[:, 2] + off[1]
    inside = (v[:, 2] > 0) & (abs(x) <= field[instr][0]) & (abs(y) <= field[instr][1])
    seen[instr, b] += drawn & inside & (lit if instr == "HERA_AFC-1" else True)

    if b == 1:
        # Each body's geometry: Hera's distance, the point under Hera
        # (planetocentric, where the line to the centre meets the surface),
        # the phase angle.
        row = [et]
        sun = spice.spkpos("SUN", et, "J2000", "LT+S", "DIDYMOS")[0]
        for body, name, s in zip(sim.bodies, MESHES, (DIDYMOS_SHIFT, DIMORPHOS_SHIFT)):
            seen_from = hera - body.mat[:3, 3]
            _, lon, lat = spice.reclat(spice.pxform("J2000", f"{name}_FIXED", et + s) @ seen_from)
            row += [numpy.linalg.norm(seen_from), lat * DPR, lon * DPR % 360.0, spice.vsep(sun - body.mat[:3, 3], seen_from) * DPR]
        rows.append(row)
    k += 1

os.makedirs(OUT, exist_ok=True)
g = numpy.array(rows)
hours = (g[:, 0] - g[0, 0]) / 3600.0
start = spice.et2utc(g[0, 0], "ISOC", 0)
mine = {instr: numpy.array([shot[1] == instr for shot in shots[:len(g)]]) for instr in INSTRUMENTS}
header = "utc,instrument,didymos_km,didymos_lat,didymos_lon,didymos_phase,dimorphos_km,dimorphos_lat,dimorphos_lon,dimorphos_phase"
numpy.savetxt(f"{OUT}/geometry.csv", numpy.column_stack([[spice.et2utc(t, "ISOC", 3) for t in g[:, 0]],
                                                         [shot[1] for shot in shots[:len(g)]], g[:, 1:]]),
              fmt="%s", delimiter=",", header=header, comments="")

fig, ax = plt.subplots(4, 1, figsize=(10, 10), sharex=True)
for b, (mark, name) in enumerate((("o", "Didymos"), ("s", "Dimorphos"))):
    ax[0].plot(hours, g[:, 1 + 4 * b], mark + "-", ms=3, label=name)
    ax[1].plot(hours, g[:, 4 + 4 * b], mark + "-", ms=3, label=name)
    ax[2].plot(hours, g[:, 2 + 4 * b], mark, ms=3, label=f"{name}, latitude")
    ax[2].plot(hours, g[:, 3 + 4 * b], mark, ms=3, mfc="none", label=f"{name}, longitude (east)")
    for instr in INSTRUMENTS:
        ax[3].plot(hours, g[:, 1 + 4 * b] * ifov[instr] * 1e3, mark + ("-" if b == 0 else "--"), ms=3,
                   label=f"{name}, {instr[5:]} ({ifov[instr] * 1e6:.1f} µrad)")
ax[0].set_ylabel("Hera's distance (km)")
ax[0].legend()
ax[1].set_ylabel("phase angle (°)")
ax[1].legend()
ax[2].set_ylabel("sub-spacecraft point (°)")
ax[2].legend(fontsize=8, ncol=2)
ax[3].set_ylabel("resolution under Hera (m/px)")
ax[3].set_xlabel(f"hours from {start} UTC")
ax[3].legend()
ax[0].set_title(f"Hera at Didymos, the times of {taken['HERA_AFC-1']} AFC and {taken['HERA_TIRI']} TIRI images")
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
        fig.colorbar(art, ax=a, label=f"seen in how many of the {taken[instr]} images")
        a.plot(g[mine[instr], 3 + 4 * b], g[mine[instr], 2 + 4 * b], "+", color="r", ms=7, mew=1.2, label="sub-spacecraft point")
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
