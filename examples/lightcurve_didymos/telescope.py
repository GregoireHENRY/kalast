#!/usr/bin/env python
import os
import subprocess
import sys

import matplotlib
import numpy
import spiceypy as spice

import kalast.app
import kalast.plot
import kalast.telescope as telescope
from kalast.app import App
from kalast.scattering import Hapke
from kalast.util import RPD

# No plot window beside kalast's: drawn to a file, opened at the night's end.
matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402

kalast.plot.style.load("lightcurve")  # as Scheirich et al. (2024) draw theirs

KERNELS = "examples/lightcurve_didymos/kernels.tm"

# Drawn and measured at their finest.
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

DT = 120.0  # seconds of the night from one exposure's start to the next, a frame drawn each
EXPOSURE = 90.0  # seconds each
DISTANCE = 20.0  # km: the camera on the line to the observatory, this far out

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

app = App()
sim = app.simulation
sim.config.wireframe.mode = 0
sim.config.shading.srgb_mode = "linear"  # a pixel is exposure x I/F x 255
sim.config.light.exposure = 20.0
sim.config.huds = [kalast.app.Hud("")]
# Each frame's light, from the frame itself: the telescope's target.
sim.config.photometry.enabled = True

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

# The night drawn, an exposure a frame: its frame made on the GPU from that
# frame's own light, written as FITS a frame or two later.
times, paths, rendered = {}, {}, {}
plotted = False
et = et0

while app.running:
    for iteration, _, _, mag in sim.exposed():
        rendered[iteration] = mag

    # The night's frames all written: reduced from the files as a real night's
    # are -- the target where its ephemeris puts it, the frame's zero point
    # from its comparison stars -- and plotted beside the rendered light.
    if not plotted and et > etf and sim.exposures_pending == 0:
        reduced = [(i, telescope.reduce(telescope.read(paths[i]), stars, APERTURE)) for i in sorted(paths)]
        reduced = [(i, red) for i, red in reduced if red is not None]  # a frame without its target or stars, left out
        utc = numpy.array([times[i] for i, _ in reduced]) - midnight
        measured = numpy.array([red.mag for _, red in reduced])
        errors = numpy.array([red.error for _, red in reduced])
        model = numpy.array([rendered[i] for i, _ in reduced])
        print(f"{len(reduced)} of {len(paths)} frames reduced, {numpy.median(errors) * 1000:.1f} mmag a frame, "
              f"{numpy.median([red.comparisons for _, red in reduced]):.0f} comparison stars; measured less rendered: "
              f"{numpy.mean(measured - model) * 1000:+.1f} mmag, {numpy.std(measured - model) * 1000:.1f} rms")
        plt.errorbar(utc / 3600, measured, errors, fmt="o", elinewidth=0.6, label="measured, the frames")
        plt.plot(utc / 3600, model, "k-", lw=1.5, label="rendered")
        plt.gca().invert_yaxis()  # brighter up
        plt.xlabel("UTC (h)")
        plt.ylabel(f"{BAND.name} (mag)")
        plt.title(f"Didymos and Dimorphos through TRAPPIST-North, {START[:10]}, {SHAPES}")
        plt.legend(loc="lower center")
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

    i = sim.state.iteration
    et = et0 + DT * i  # the exposure's start
    if et > etf:
        app.simulation.state.toggle_pause()
        app.step()
        continue

    # Mid-exposure: where the system is from the observatory, and when its
    # light left it.
    mid = et + 0.5 * EXPOSURE
    toward, te = seen(mid)
    delta = numpy.linalg.norm(toward)
    to_site = -toward / delta

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
    to_sun = numpy.array(spice.spkpos("SUN", te, "J2000", "NONE", "DIDYMOS_BARYCENTER")[0])
    sim.sun.pos = to_sun.tolist()

    # The camera on the line to the observatory, celestial north up.
    sim.camera.pos = (to_site * DISTANCE).tolist()
    sim.camera.dir = (-to_site).tolist()
    sim.camera.up = [0.0, 0.0, 1.0]

    # Where it is on the sky and how it moves against the stars, how high it stands.
    ra, dec = sky_of(toward)
    ahead, behind = sky_of(seen(mid + 30.0)[0]), sky_of(seen(mid - 30.0)[0])
    rate = ((ahead[0] - behind[0]) * numpy.cos(numpy.radians(dec)) * 60.0, (ahead[1] - behind[1]) * 60.0)  # arcsec/s
    up = numpy.array(spice.pxform("IAU_EARTH", "J2000", mid)) @ zenith
    altitude = 90.0 - numpy.degrees(spice.vsep(up, toward))

    # The exposure of this frame's light.
    exposure = telescope.Exposure(spice.et2utc(et, "ISOC", 3), EXPOSURE, ra0, dec0, telescope.airmass(altitude), seed=i)
    paths[i] = f"{FRAMES}/{i:04d}.fits"
    times[i] = mid
    sim.expose(paths[i], exposure, (ra, dec), rate, numpy.linalg.norm(to_sun) / AU, delta, stars,
               CAMERA, BAND, SKY, EFFECTS, "(65803) Didymos")
    sim.huds[0].text = f"TRAPPIST-North  {exposure.start[:19]} UTC  {SHAPES}"

    app.step()

spice.kclear()
