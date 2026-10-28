#!/usr/bin/env python
"""What AFC saw: Didymos and Dimorphos through the Asteroid Framing Camera at
the times of its images, beside the images themselves -- or, without any, from
START to END every DT.

Each image is drawn in the rows and columns of AFC's level-1B products, each
pixel the I/F of the ground it sees over the pixel (kalast's reference image),
written as FITS to OUT beside a PNG of the frame. Didymos is drawn where the
image has it: SPICE's pointing is turned onto it, and the turn is the
pointing's error. Then the two bodies' drawings are fitted onto the image:
Didymos's size and signal against its drawing, and Dimorphos's place beyond
it, the error of its orbit given as time along it. afc.csv and afc.png set
them out."""
import glob
import os
import subprocess
import sys

import matplotlib
import numpy
import spiceypy as spice
from astropy.io import fits
from scipy import ndimage, optimize

from kalast.app import App, Hud
from kalast.scattering import Hapke
from kalast.util import DPR, RPD

matplotlib.use("Agg")  # no plot window beside kalast's: drawn to a file
import matplotlib.pyplot as plt  # noqa: E402

# AFC's level-1B images, in any folders below; AFC-1's from SINCE, when
# Didymos is resolved. Without any, every DT from START to END.
IMAGES = "/Users/gregoireh/projects/edds_decoder/out/afc"
SINCE = "2026-10-23"
START, END, DT = "2026-11-05 00:00:00 UTC", "2027-04-30 00:00:00 UTC", 15 * 60.0

KERNELS = "/Users/gregoireh/data/spice/hera/kernels/mk"
DIDYMOS = "/Users/gregoireh/data/mesh/didymos/g_01165mm_spc_didy_v003_100k.obj"
DIMORPHOS = "/Users/gregoireh/data/mesh/dimorphos/g_00243mm_spc_dimo_v004_100k.obj"
OUT = "out/hera_didymos/afc"
# Didymos turned as its PCK has it this much later: the PCK leaves the prime
# meridian undefined. Dimorphos where the ephemeris has it this much later
# (tiri.py). afc.csv gives what is left. 0 for the kernels as they are.
DIDYMOS_SHIFT = 5990.0  # s
DIMORPHOS_SHIFT = 13689.0  # s

# Buratti et al. (2024), from DART's images and the system's phase curve.
LAW = Hapke(w=0.126, b=0.36, c=1.0, b0=0.97, h=0.020, theta_bar=18.0 * RPD, k=1.0)
INSTR = "HERA_AFC-1"
R = 40  # px, half the side of the window fitted, Dimorphos inside it

spice.kclear()
spice.furnsh(f"{KERNELS}/hera_plan.tm")  # Didymos and Dimorphos
spice.furnsh(f"{KERNELS}/hera_ops.tm")  # Hera's measured attitude, over the plan's
_, _, _, _, bounds = spice.getfov(spice.bodn2c(INSTR), 4)
fovy = 2.0 * numpy.arctan2(abs(bounds[0][1]), bounds[0][2])
pixels = int(spice.gipool(f"INS{spice.bodn2c(INSTR)}_PIXEL_SAMPLES", 0, 1)[0])
focal = pixels / 2.0 / numpy.tan(fovy / 2.0)  # px
middle = (pixels - 1) / 2.0

images = []
for path in glob.glob(f"{IMAGES}/**/*.fits", recursive=True):
    h = fits.getheader(path)
    if h.get("INSTRUME") == "AFC1" and h.get("DATE-OBS", "") >= SINCE:
        images.append((spice.str2et(h["DATE-OBS"]), path))
images.sort()
times = [t for t, _ in images] or numpy.arange(spice.str2et(START), spice.str2et(END), DT)
print(f"{len(times)} times, {spice.et2utc(times[0], 'ISOC', 0)} to {spice.et2utc(times[-1], 'ISOC', 0)}")

app = App()
sim = app.simulation
sim.config.image.width = sim.config.image.height = pixels
sim.config.axes.style = "off"
sim.config.wireframe.mode = 0
sim.config.shadows.method = "reference"  # each pixel over its area
sim.config.shading.srgb_mode = "linear"
sim.config.light.exposure = 20.0  # shown as 20 x I/F
sim.config.export.dir = OUT
sim.huds = [Hud("", size=16)]

# The scene in AFC's frame: the camera at its origin along its boresight, +X
# up, as the products have it: columns along +Y, rows along -X.
sim.camera.pos = [0.0, 0.0, 0.0]
sim.camera.dir = [0.0, 0.0, 1.0]
sim.camera.up = [1.0, 0.0, 0.0]
sim.camera.projection.fovy = fovy

sim.load_mesh(path=DIDYMOS)
sim.load_mesh(path=DIMORPHOS)
for body in sim.bodies:
    body.scattering = LAW
dimorphos_radius = numpy.linalg.norm(sim.bodies[1].mesh.positions, axis=1).max()  # km


def pixel(v):
    """Where a direction in AFC's frame lands: x along the columns, y the rows."""
    return numpy.array([middle + focal * v[1] / v[2], middle - focal * v[0] / v[2]])


def pose(et, turn=numpy.eye(3)):
    """Bodies and Sun in AFC's frame turned by `turn`; the bodies' places."""
    to_afc = turn @ spice.pxform("J2000", INSTR, et)
    didymos = to_afc @ spice.spkpos("DIDYMOS", et, "J2000", "LT+S", "HERA")[0]
    dimorphos = didymos + to_afc @ spice.spkpos("DIMORPHOS", et + DIMORPHOS_SHIFT, "J2000", "NONE", "DIDYMOS")[0]
    for body, p, name, s in zip(sim.bodies, (didymos, dimorphos), ("DIDYMOS", "DIMORPHOS"), (DIDYMOS_SHIFT, DIMORPHOS_SHIFT)):
        body.mat[:3, :3] = to_afc @ spice.pxform(f"{name}_FIXED", "J2000", et + s)
        body.mat[:3, 3] = p
    sim.sun.pos = to_afc @ spice.spkpos("SUN", et, "J2000", "LT+S", "HERA")[0]
    return didymos, dimorphos


def beside(et, shift):
    """Where Dimorphos is in the image against Didymos (px)."""
    to_afc = spice.pxform("J2000", INSTR, et)
    d = to_afc @ spice.spkpos("DIDYMOS", et, "J2000", "LT+S", "HERA")[0]
    return pixel(d + to_afc @ spice.spkpos("DIMORPHOS", et + shift, "J2000", "NONE", "DIDYMOS")[0]) - pixel(d)


def measured(et):
    """Whether Hera's measured attitude covers et; after it, SPICE has the plan's."""
    try:
        spice.pxform("J2000", "HERA_SPACECRAFT_MEAS", et)
        return True
    except spice.exceptions.SpiceyError:
        return False


def target(img):
    """The brightest feature of a frame, 3x3-median smoothed: its centroid
    (x, y) above the sky, and its peak in noise units."""
    smooth = ndimage.median_filter(img, 3)
    y, x = numpy.unravel_index(numpy.argmax(smooth), img.shape)
    noise = 1.4826 * numpy.median(numpy.abs(numpy.diff(img, axis=1))) / numpy.sqrt(2)
    sky = numpy.median(img[max(y - 30, 0):y + 31, max(x - 30, 0):x + 31])
    ys, xs = numpy.mgrid[max(y - 10, 0):min(y + 11, img.shape[0]), max(x - 10, 0):min(x + 11, img.shape[1])]
    w = numpy.clip(img[ys, xs] - sky, 0.0, None)
    return (w * xs).sum() / w.sum(), (w * ys).sum() / w.sum(), (smooth[y, x] - sky) / max(noise, 1e-30)


def fit(observed, parts, x, y):
    """The two bodies' drawings spread, Didymos's zoomed, moved and scaled onto
    the image about (x, y): each one's scale, Didymos's move (px) and zoom,
    Dimorphos's move beyond it (px), the spread (a Gaussian's sigma, px), the
    residual's rms."""
    x, y = int(round(x)), int(round(y))
    window = numpy.pad(observed, R, constant_values=numpy.median(observed))[y:y + 2 * R + 1, x:x + 2 * R + 1]
    cuts = [numpy.pad(p, R)[y:y + 2 * R + 1, x:x + 2 * R + 1] for p in parts]
    c = numpy.array(ndimage.center_of_mass(cuts[0]))

    def model(q):
        a, b, sky, sx, sy, zoom, tx, ty, s = q
        d = ndimage.affine_transform(cuts[0], numpy.eye(2) / zoom, offset=c - (c + (sy, sx)) / zoom, order=1)
        m = ndimage.shift(cuts[1], (sy + ty, sx + tx), order=1)
        return ndimage.gaussian_filter(a * d + b * m, s) + sky

    def solve(tx, ty):
        return optimize.least_squares(lambda q: (model(q) - window).ravel(), [scale, scale, sky, 0.0, 0.0, 1.0, tx, ty, 1.0],
                                      bounds=([0, 0, -numpy.inf, -9, -9, 0.7, -20, -20, 0.3], [numpy.inf] * 3 + [9, 9, 1.4, 20, 20, 4]))

    sky = numpy.median(window)
    scale = (window - sky).clip(0).sum() / max(cuts[0].sum() + cuts[1].sum(), 1e-12)
    # Dimorphos where it is drawn; then where Didymos alone leaves most light.
    # The better of the two.
    first = solve(0.0, 0.0)
    a, _, sky, sx, sy, zoom, _, _, s = first.x
    rest = ndimage.gaussian_filter(window - model([a, 0.0, sky, sx, sy, zoom, 0.0, 0.0, s]), 1.0)
    yy, xx = numpy.mgrid[:2 * R + 1, :2 * R + 1]
    rest[numpy.hypot(xx - c[1] - sx, yy - c[0] - sy) < zoom * numpy.sqrt((cuts[0] > 0).sum() / numpy.pi) + 2.0] = -numpy.inf
    my, mx = numpy.unravel_index(numpy.argmax(rest), rest.shape)
    if cuts[1].sum() > 0:
        cy, cx = ndimage.center_of_mass(cuts[1])
        first = min(first, solve(mx - sx - cx, my - sy - cy), key=lambda f: f.cost)
    q = first.x
    return q[0], q[1], q[3], q[4], q[5], q[6], q[7], q[8], numpy.sqrt(numpy.mean(first.fun ** 2))


def cut(img, x, y):
    """64 x 64 pixels about (x, y), zero past the frame's edges."""
    x, y = int(round(x)), int(round(y))
    return numpy.pad(img, 32)[y:y + 64, x:x + 64]


os.makedirs(OUT, exist_ok=True)
rows, crops = [], []
for k, et in enumerate(times):
    # Didymos where the image has it: SPICE's pointing turned onto it.
    observed, found, turn, aim = None, False, numpy.eye(3), numpy.zeros(2)
    if images:
        observed = numpy.nan_to_num(fits.getdata(images[k][1]).astype(float))
        ox, oy, snr = target(observed)
        found = snr >= 100.0
        if found:
            before = pose(et)[0]
            seen = numpy.array([(middle - oy) / focal, (ox - middle) / focal, 1.0])
            turn = spice.axisar(numpy.cross(before, seen), spice.vsep(before, seen))
            aim = numpy.array([ox, oy]) - pixel(before)
    didymos, dimorphos = pose(et, turn)
    distance = numpy.linalg.norm(didymos), numpy.linalg.norm(dimorphos)
    phase = spice.vsep(sim.sun.pos - didymos, -didymos) * DPR
    sim.huds[0].text = (f"{spice.et2utc(et, 'ISOC', 1)}  Didymos {distance[0]:.1f} km  Dimorphos {distance[1]:.1f} km  "
                        f"phase {phase:.1f}°  {distance[0] / focal * 1e3:.0f} m/px")
    app.step()  # the scene seen: the sum begins again
    while app.running and not sim.reference_done:
        app.step()
    if not app.running:
        break
    sim.export_once()
    app.step()
    if observed is None:
        continue

    # Each body's part of the drawing: Dimorphos's pixels about its centre.
    drawn = numpy.asarray(sim.reference_image())[..., 0] / sim.config.light.exposure
    yy, xx = numpy.mgrid[:pixels, :pixels]
    mx, my = pixel(dimorphos)
    near = numpy.hypot(xx - mx, yy - my) < dimorphos_radius / distance[1] * focal + 3.0
    parts = [numpy.where(near, 0.0, drawn), numpy.where(near, drawn, 0.0)]
    if drawn.any():
        name = os.path.basename(images[k][1]).replace("afc_par_sc", "kalast")
        header = fits.Header([("DATE-OBS", spice.et2utc(et, "ISOC", 3)), ("BUNIT", "I/F"),
                              ("AIMX", aim[0], "[px] SPICE's pointing turned, columns"), ("AIMY", aim[1], "[px] rows"),
                              ("DIDSHIFT", DIDYMOS_SHIFT, "[s] Didymos's rotation"),
                              ("DIMSHIFT", DIMORPHOS_SHIFT, "[s] Dimorphos along its orbit")])
        fits.writeto(f"{OUT}/{name}", drawn.astype(numpy.float32), header, overwrite=True)
    if found and parts[0].any():
        a, b, sx, sy, zoom, tx, ty, spread, rms = fit(observed, parts, ox, oy)
        v = (beside(et, DIMORPHOS_SHIFT + 60.0) - beside(et, DIMORPHOS_SHIFT - 60.0)) / 120.0
        lag = numpy.array([tx, ty]) @ v / (v @ v)
        rows.append((spice.et2utc(et, "ISOC", 3), aim[0] + sx, aim[1] + sy, zoom, a, tx, ty, lag, b, spread, rms, snr,
                     parts[0].sum(), parts[1].sum(), fits.getheader(images[k][1])["EXPOSURE"],
                     "measured" if measured(et) else "planned"))
        print("%s  pointing %+.1f %+.1f px  Didymos x%.3f, %.3g per I/F  Dimorphos %+.1f %+.1f px, %+.0f s, %.3g per I/F  "
              "PSF %.2f px  rms %.2g" % rows[-1][:11] + f"  {rows[-1][-1]} attitude")
        crops.append((rows[-1][0], cut(observed, ox, oy), cut(drawn, ox - sx, oy - sy)))
    else:
        print(f"{spice.et2utc(et, 'ISOC', 3)}  Didymos not in the image")

if rows:
    header = ("utc,pointing_x,pointing_y,didymos_size,didymos_signal,dimorphos_x,dimorphos_y,dimorphos_s,dimorphos_signal,"
              "psf_px,rms,peak_sigma,didymos_drawn,dimorphos_drawn,exposure_s,attitude")
    numpy.savetxt(f"{OUT}/afc.csv", numpy.array(rows, dtype=object), fmt="%s", delimiter=",", header=header, comments="")
    r = {name: numpy.array([row[i] for row in rows]) for i, name in enumerate(header.split(","))}
    hours = numpy.array([(spice.str2et(t) - spice.str2et(rows[0][0])) / 3600.0 for t in r["utc"]])
    held = r["attitude"] == "measured"
    found = numpy.hypot(r["dimorphos_x"], r["dimorphos_y"]) < 4.0  # Dimorphos found beside its drawing
    lit = r["dimorphos_drawn"] > 0.8 * numpy.median(r["dimorphos_drawn"])  # and out of Didymos's shadow there
    if held.any():
        print(f"measured attitude, {held.sum()} images: pointing, observed - SPICE: x {numpy.median(r['pointing_x'][held]):+.1f} px, "
              f"y {numpy.median(r['pointing_y'][held]):+.1f} px; Didymos {numpy.median(r['didymos_size'][held]):.3f} its drawing's size, "
              f"{numpy.median(r['didymos_signal'][held]):.3g} per I/F")
    if (held & found).any():
        lag = numpy.median(r["dimorphos_s"][held & found])
        print(f"Dimorphos, observed - drawn along its orbit: {lag:+.0f} s (median of {(held & found).sum()} images): "
              f"DIMORPHOS_SHIFT = {DIMORPHOS_SHIFT + lag:.0f} would match them")

    fig = plt.figure(figsize=(15, 11.5))
    grid = fig.add_gridspec(4, 8, height_ratios=(1.0, 1.0, 1.0, 1.3))
    ax = fig.add_subplot(grid[0, :6])
    for name, mark, color in (("pointing_x", "o", "C0"), ("pointing_y", "s", "C1")):
        for use, fill, att in ((held, color, "measured"), (~held, "none", "planned")):
            if use.any():
                ax.plot(hours[use], r[name][use], mark, color=color, mfc=fill, ms=4, label=f"Didymos, {name[-1]}, {att} attitude")
    ax.set_ylabel("observed - SPICE (px)")
    ax.legend(fontsize=8, ncol=2)
    ax.set_title(f"AFC-1, {rows[0][0][:10]} to {rows[-1][0][:10]}: the bodies where the images have them, against kalast's drawings")
    ax = fig.add_subplot(grid[1, :6], sharex=ax)
    scale = numpy.median(r["didymos_signal"][held] if held.any() else r["didymos_signal"])
    ax.plot(hours, r["didymos_size"], "o", ms=4, label="Didymos's size")
    ax.plot(hours, r["didymos_signal"] / scale, "s", ms=4, label="Didymos's signal, to its median")
    ax.plot(hours[found & lit], r["dimorphos_signal"][found & lit] / scale, "^", ms=4,
            label="Dimorphos's signal, to Didymos's median, lit in the drawing")
    ax.set_ylim(0.0, 2.0)
    ax.set_ylabel("observed / drawn")
    ax.legend(fontsize=8, ncol=3)
    ax = fig.add_subplot(grid[2, :6], sharex=ax)
    ax.plot(hours[found], r["dimorphos_x"][found], "x", label="x")
    ax.plot(hours[found], r["dimorphos_y"][found], "+", label="y")
    ax.set_xlabel(f"hours from {rows[0][0][:19]} UTC")
    ax.set_ylabel("Dimorphos beyond Didymos,\nobserved - drawn (px)")
    ax.legend(fontsize=8)
    ax = fig.add_subplot(grid[:3, 6:])
    track = numpy.array([beside(spice.str2et(t), DIMORPHOS_SHIFT) for t in r["utc"]])
    ax.plot(track[:, 0], track[:, 1], "k.", ms=3, label="drawn")
    ax.plot(track[found, 0] + r["dimorphos_x"][found], track[found, 1] + r["dimorphos_y"][found], "o", ms=4, mfc="none",
            label="observed")
    ax.plot(0, 0, "o", color="0.6", ms=14)
    ax.invert_yaxis()  # rows down, as the images
    ax.set_aspect("equal")
    ax.set_title("Dimorphos beside Didymos (px)", fontsize=10)
    ax.legend(fontsize=8)
    for j, c in enumerate(numpy.linspace(0, len(crops) - 1, min(8, len(crops))).astype(int)):
        time, seen, drawn = crops[c]
        ax = fig.add_subplot(grid[3, j])
        ax.imshow(numpy.vstack([seen / max(seen.max(), 1e-12), drawn / max(drawn.max(), 1e-12)]), cmap="gray", vmin=0.0, vmax=1.0)
        ax.set_title(f"{time[5:10]} {time[11:19]}", fontsize=8)
        ax.set_xticks([])
        ax.set_yticks([32, 96], ["AFC", "kalast"] if j == 0 else ["", ""], fontsize=8)
    fig.tight_layout()
    fig.savefig(f"{OUT}/afc.png", dpi=110)
    print(f"wrote {OUT}/")
    if sys.platform == "win32":
        os.startfile(f"{OUT}/afc.png")
    else:
        subprocess.Popen(["open" if sys.platform == "darwin" else "xdg-open", f"{OUT}/afc.png"])

sim.state.toggle_pause()  # the last frame kept on screen
while app.running and sim.state.is_paused:
    app.step()

spice.kclear()
