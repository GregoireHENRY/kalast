#!/usr/bin/env python
import os
import subprocess
import sys

import matplotlib
import numpy
import spiceypy as spice

import kalast.mesh
import kalast.plot
import kalast.telescope as telescope
from kalast.lightcurve import flux
from kalast.scattering import Hapke
from kalast.util import RPD

# Drawn to a file and opened: a plot window cannot open beside kalast's.
matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402

kalast.plot.style.load("lightcurve")  # as Scheirich et al. (2024) draw theirs

KERNELS = "examples/lightcurve_didymos/kernels.tm"

# DART's shape models at 49,152 facets, as lightcurve.py has them.
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

DT = 120.0  # seconds from one exposure's start to the next
EXPOSURE = 90.0  # seconds each

# Buratti et al. (2024), from DART's images and the system's phase curve.
LAW = Hapke(w=0.126, b=0.36, c=1.0, b0=0.97, h=0.020, theta_bar=18.0 * RPD, k=1.0)

# The telescope, its filter and the night: TRAPPIST-North's camera, Cousins R,
# a dark night at Oukaimeden -- 2" seeing, 20.5 mag a square arcsecond, 0.10 mag
# an airmass. What is published and what is assumed: docs/API.md.
CAMERA = telescope.Telescope()
BAND = telescope.Band("Rc")
SKY = telescope.Sky()
# Every effect on; switch any off, telescope.Effects(stars=False, photon_noise=False, ...):
# psf, trail, sky, stars, extinction, scintillation, photon_noise, dark, read_noise,
# bias, saturation.
EFFECTS = telescope.Effects()
# The light summed within 7 pixels, the sky read from 13 to 23: wide enough for the
# target's trail, 2" in 90 s.
APERTURE = telescope.Aperture(7.0, 13.0, 23.0)

FRAMES = "out/trappist_2022-11-18"  # a FITS file an exposure, 8 MB each
GAIA = "out/gaia_didymos_2022-11-18.csv"  # the field's stars, asked of Gaia once
PLOT = "out/lightcurve_didymos_telescope.png"
AU = 149597870.7  # km

if SHAPES == "meshes":
    meshes = [kalast.mesh.Mesh(path=DIDYMOS), kalast.mesh.Mesh(path=DIMORPHOS)]
    stretch = [numpy.ones(3), numpy.ones(3)]
else:
    ico = kalast.mesh.Mesh(path="res/ico5.obj")
    meshes = [ico, ico]
    stretch = EXTENTS

spice.kclear()
spice.furnsh(KERNELS)
radii = spice.bodvrd("EARTH", "RADII", 3)[1]
station = spice.georec(SITE[0] * RPD, SITE[1] * RPD, SITE[2], radii[0], (radii[0] - radii[2]) / radii[0])
zenith = spice.surfnm(radii[0], radii[1], radii[2], station)
impact = spice.str2et("2022-09-26T23:14:24.184")
et0 = spice.str2et(START)
etf = spice.str2et(END)
midnight = spice.str2et(START[:10])


def seen(et):
    """The system from the observatory: its direction, its distance, when its light left."""
    state, light_time = spice.spkcpo("DIDYMOS_BARYCENTER", et, "J2000", "OBSERVER", "CN", station, "EARTH", "IAU_EARTH")
    return numpy.array(state[:3]), et - light_time


def sky_of(v):
    """Right ascension and declination, degrees."""
    _, ra, dec = spice.recrad(v)
    return numpy.degrees(ra), numpy.degrees(dec)


# The telescope points at the track's middle all night: one field, the same
# comparison stars in every frame. Its stars from Gaia, carried to the night.
ra0, dec0 = sky_of(seen(0.5 * (et0 + etf))[0])
year = 2000.0 + et0 / 86400.0 / 365.25
stars = telescope.gaia(ra0, dec0, 0.30, g_max=19.0, band=BAND.name, year=year, cache=GAIA)

paths, model, times = [], [], []
for i, start in enumerate(numpy.arange(et0, etf + 1.0, DT)):
    et = start + 0.5 * EXPOSURE  # mid-exposure
    toward, te = seen(et)
    delta = numpy.linalg.norm(toward)
    to_site = -toward / delta

    # The bodies about the system's barycentre, as lightcurve.py places them.
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
    r = numpy.linalg.norm(to_sun)

    # Its magnitude above the air: its light, at its distances.
    light = flux(vertices, [mesh.indices for mesh in meshes], to_sun / r, to_site, LAW).flux
    mag = BAND.magnitude(light, r / AU, delta)

    # Where it is and how it moves against the stars, how high it stands.
    ra, dec = sky_of(toward)
    ahead = sky_of(seen(et + 30.0)[0])
    behind = sky_of(seen(et - 30.0)[0])
    rate = ((ahead[0] - behind[0]) * numpy.cos(numpy.radians(dec)) * 60.0, (ahead[1] - behind[1]) * 60.0)  # arcsec/s
    up = numpy.array(spice.pxform("IAU_EARTH", "J2000", et)) @ zenith
    altitude = 90.0 - numpy.degrees(spice.vsep(up, toward))

    # The exposure, as the camera records it.
    exposure = telescope.Exposure(spice.et2utc(start, "ISOC", 3), EXPOSURE, ra0, dec0, telescope.airmass(altitude), seed=i)
    frame = telescope.expose(CAMERA, BAND, SKY, EFFECTS, exposure, telescope.Source(ra, dec, mag, rate), stars, "(65803) Didymos")
    paths.append(f"{FRAMES}/{i:04d}.fits")
    frame.write(paths[-1])
    model.append(mag)
    times.append(et)
    print(f"\r{paths[-1]}  {exposure.start}  Rc {mag:.3f}", end="", flush=True)
print()
spice.kclear()

# The night reduced as a real one is, frame by frame from the files: the target
# where its ephemeris puts it, the frame's zero point from its comparison stars.
reduced = [(k, telescope.reduce(telescope.read(path), stars, APERTURE)) for k, path in enumerate(paths)]
reduced = [(k, red) for k, red in reduced if red is not None]  # a frame without its target or stars, left out
kept = [k for k, _ in reduced]
measured = numpy.array([red.mag for _, red in reduced])
errors = numpy.array([red.error for _, red in reduced])
model = numpy.array(model)
utc = (numpy.array(times) - midnight) / 3600
print(f"{len(kept)} of {len(paths)} frames reduced, {numpy.median(errors) * 1000:.1f} mmag a frame, "
      f"{numpy.median([red.comparisons for _, red in reduced]):.0f} comparison stars; "
      f"measured less model: {numpy.mean(measured - model[kept]) * 1000:+.1f} mmag, "
      f"{numpy.std(measured - model[kept]) * 1000:.1f} rms")

# Each about its own mean, as Pravec and Scheirich plot a night.
plt.errorbar(utc[kept], (measured - measured.mean()) * 1000, errors * 1000, fmt="o", elinewidth=0.6, label="measured, the frames")
plt.plot(utc, (model - model.mean()) * 1000, "k-", lw=1.5, label="model")
plt.gca().invert_yaxis()  # brighter up
plt.xlabel("UTC (h)")
plt.ylabel("Relative magnitude (mmag)")
plt.title(f"Didymos and Dimorphos through TRAPPIST-North, {START[:10]}, {SHAPES}, {BAND.name}")
plt.legend(loc="lower center")
os.makedirs(os.path.dirname(PLOT), exist_ok=True)
plt.savefig(PLOT, dpi=120)
print(f"wrote {PLOT}")
if sys.platform == "win32":
    os.startfile(PLOT)
else:
    subprocess.Popen(["open" if sys.platform == "darwin" else "xdg-open", PLOT])
