#!/usr/bin/env python
"""What TIRI saw: Didymos and Dimorphos in the thermal infrared at the times of
the TIRI images decoded by edds_decoder, beside the images themselves.

The surface temperatures are kalast's thermophysical model, spun up SPINUP days
before the first image under the Sun as SPICE has it, the shadows -- each
body's on the other included -- from the frames. Each pixel is the radiance of
the ground it sees in the image's filter, averaged over SUPER x SUPER samples
and spread by TIRI's point spread (PSF), written as FITS in the products' rows
and columns.

Didymos is drawn where each image has it: SPICE's pointing is turned onto it,
and the turn is the pointing's error. tiri.csv and tiri.png set each image
beside its drawing. The two bodies' drawings are moved and scaled onto the
image: Didymos's move is what is left of the pointing's error, Dimorphos's
beyond it the error of its orbit, given as time along it, and the scales are
each body's signal against its drawing."""
import glob
import os
import subprocess
import sys

import matplotlib
import numpy
import spiceypy as spice
from astropy.io import fits
from scipy import ndimage, optimize

import kalast
import kalast.tiri_alignment as alignment
import kalast.tpm.radiance as radiance
from kalast.app import App, Hud
from kalast.tpm import core, properties
from kalast.util import AU_KM

matplotlib.use("Agg")  # no plot window beside kalast's: drawn to a file
import matplotlib.pyplot as plt  # noqa: E402

IMAGES = "/Users/gregoireh/projects/edds_decoder/out/data_par/asteroid"
KERNELS = "/Users/gregoireh/data/spice/hera/kernels/mk"
MESHES = {
    "DIDYMOS": "/Users/gregoireh/data/mesh/didymos/g_01165mm_spc_didy_v003_100k.obj",
    "DIMORPHOS": "/Users/gregoireh/data/mesh/dimorphos/g_00243mm_spc_dimo_v004_100k.obj",
}
RESPONSE = "/Users/gregoireh/data/hera/tiri/response.csv"
OUT = "out/hera_didymos/tiri"

SPINUP = 3.0  # days of the model before the first image
LAYERS_PER_SKIN = 8  # at the surface: within 0.6 K of a converged column
SUPER = 4  # samples a pixel along each side: Didymos is 5 pixels across
PSF = 1.3  # TIRI's point spread, a Gaussian's sigma (px)
ALIGN = True  # TIRI's boresight as measured in flight, kalast.tiri_alignment
BLIND = 6  # columns of the raw frame the products leave out, the first
INSTR = "HERA_TIRI"
# Didymos turned as its PCK has it this much later: the PCK leaves the prime
# meridian undefined (afc.py). Dimorphos where the ephemeris has it this much
# later; tiri.csv gives what is left. 0 for the kernels as they are.
DIDYMOS_SHIFT = 5990.0  # s
DIMORPHOS_SHIFT = 13689.0  # s

spice.kclear()
spice.furnsh(f"{KERNELS}/hera_plan.tm")  # Didymos and Dimorphos
spice.furnsh(f"{KERNELS}/hera_ops.tm")  # Hera's measured attitude, over the plan's
_, _, _, _, bounds = spice.getfov(spice.bodn2c(INSTR), 4)
fovy = 2.0 * numpy.arctan2(abs(bounds[0][1]), bounds[0][2])
focal = 384.0 / numpy.tan(fovy / 2.0)  # px
turn = alignment.rotation() if ALIGN else numpy.eye(3)

# The images with the shutter open: time (SPI_TIME without the TC report),
# filter letter, product.
images = []
for path in glob.glob(f"{IMAGES}/**/tiri_par_*.fits", recursive=True):
    h = fits.getheader(path)
    if h["FW_NUM"] != "CLOSE":
        images.append((spice.str2et(h.get("DATE-OBS", h["SPI_TIME"])), h["FW_NUM"].split()[1], path))
images.sort()
print(f"{len(images)} images, {spice.et2utc(images[0][0], 'ISOC', 0)} to {spice.et2utc(images[-1][0], 'ISOC', 0)}")

app = App()
sim = app.simulation
sim.config.image.width, sim.config.image.height = 1024 * SUPER, 768 * SUPER
sim.config.axes.style = "off"
sim.config.wireframe.mode = 0
sim.config.shadows.access_shadow_map = True  # both bodies' shadows, every frame, for the model
sim.config.shading.color_mode = "data"  # the surface temperature
sim.config.data.colormap = "inferno"
sim.config.data.value_min, sim.config.data.value_max = 150.0, 350.0
sim.huds = [Hud("", size=16 * SUPER)]

# The scene in TIRI's frame: the camera along its boresight, +Y up, the
# products' rows and columns (kalast/tiri_alignment.py).
sim.camera.pos = [0.0, 0.0, 0.0]
sim.camera.dir = [0.0, 0.0, 1.0]
sim.camera.up = [0.0, 1.0, 0.0]
sim.camera.projection.fovy = fovy

for path in MESHES.values():
    sim.load_mesh(path=path)


def place(et, shift=DIMORPHOS_SHIFT, onto=numpy.eye(3)):
    """Didymos and Dimorphos from Hera in TIRI's frame turned by `onto`, and
    their rotations: Dimorphos where the ephemeris has it `shift` later, about
    Didymos."""
    to_tiri = onto @ turn @ spice.pxform("J2000", INSTR, et)
    didymos = to_tiri @ spice.spkpos("DIDYMOS", et, "J2000", "LT+S", "HERA")[0]
    dimorphos = didymos + to_tiri @ spice.spkpos("DIMORPHOS", et + shift, "J2000", "NONE", "DIDYMOS")[0]
    turns = [to_tiri @ spice.pxform(f"{name}_FIXED", "J2000", et + s) for name, s in zip(MESHES, (DIDYMOS_SHIFT, shift))]
    return (didymos, dimorphos), turns, to_tiri


def pose(et, onto=numpy.eye(3)):
    """Bodies and Sun in TIRI's frame turned by `onto`; each body's distance to
    the Sun (AU)."""
    places, turns, to_tiri = place(et, onto=onto)
    for body, p, m in zip(sim.bodies, places, turns):
        body.mat[:3, :3] = m
        body.mat[:3, 3] = p
    sun = to_tiri @ spice.spkpos("SUN", et, "J2000", "LT+S", "HERA")[0]
    sim.sun.pos = sun
    return [numpy.linalg.norm(sun - p) / AU_KM for p in places]


def beside(et, shift):
    """Where Dimorphos is in the image against Didymos, pixels (x right, y down)."""
    d, m = place(et, shift)[0]
    return -focal * (m[:2] / m[2] - d[:2] / d[2])


def pixel(v):
    """Where a direction in TIRI's frame lands in the products: x, y."""
    return numpy.array([511.5 - BLIND - focal * v[0] / v[2], 383.5 - focal * v[1] / v[2]])


def measured(et):
    """Whether Hera's measured attitude covers et; after it, SPICE has the plan's."""
    try:
        spice.pxform("J2000", "HERA_SPACECRAFT_MEAS", et)
        return True
    except spice.exceptions.SpiceyError:
        return False


# A column under each facet, as deep as a day's wave reaches, its layers
# thickening with depth; each starting where its facet radiates what it
# absorbs over a spin.
et = images[0][0] - SPINUP * 86400.0
dau = pose(et)
grounds, columns, dz = [], [], []
for i, name in enumerate(MESHES):
    prop = getattr(properties, name)
    prop.compute_conductivity_diffusivity()
    period = getattr(kalast.entity, name).spin_period
    dz.append(properties.skin_depth_1(prop.diffusivity, period) / LAYERS_PER_SKIN)
    facets = len(sim.bodies[i].mesh.facets)
    grounds.append(core.Ground.graded(prop, facets, dz[i], properties.skin_depth_2pi(prop.diffusivity, period)))
    start = core.effective_temperature(dau[i], sim.facet_mean_incidence(i), prop.albedo, prop.emissivity)
    columns.append(core.columns(grounds[i].layers, facets, start))
dt = min(g.stability_maxdt(z) for g, z in zip(grounds, dz))
bands = radiance.tiri_bands(RESPONSE, emissivity=properties.DIDYMOS.emissivity)
os.makedirs(OUT, exist_ok=True)


def target(img):
    """The brightest feature of a frame, 3x3-median smoothed: its centroid
    (x, y) above the sky, and its peak in noise units."""
    smooth = ndimage.median_filter(img, 3)
    y, x = numpy.unravel_index(numpy.argmax(smooth), img.shape)
    noise = 1.4826 * numpy.median(numpy.abs(numpy.diff(img, axis=1))) / numpy.sqrt(2)
    sky = numpy.median(img[max(y - 15, 0):y + 16, max(x - 15, 0):x + 16])
    ys, xs = numpy.mgrid[max(y - 5, 0):y + 6, max(x - 5, 0):x + 6]
    w = numpy.clip(img[ys, xs] - sky, 0.0, None)
    return (w * xs).sum() / w.sum(), (w * ys).sum() / w.sum(), (smooth[y, x] - sky) / noise


def fit(observed, parts, at, drawn_at):
    """The two bodies' drawings spread, moved and scaled onto the image about
    the target: each one's scale, Didymos's move and Dimorphos's beyond it
    (px), the spread (a Gaussian's sigma, px), the residual's rms."""
    (ox, oy), (dx, dy) = numpy.round(at).astype(int), numpy.round(drawn_at).astype(int)
    window = numpy.pad(observed, 16)[oy:oy + 33, ox:ox + 33]
    cuts = [numpy.pad(p, 24)[dy + 4:dy + 45, dx + 4:dx + 45] for p in parts]

    def model(q):
        a, b, sky, sx, sy, tx, ty, s = q
        moved = [ndimage.shift(ndimage.gaussian_filter(c, s), m, order=1)[4:37, 4:37]
                 for c, m in zip(cuts, ((sy, sx), (sy + ty, sx + tx)))]
        return a * moved[0] + b * moved[1] + sky

    def solve(tx, ty):
        return optimize.least_squares(lambda q: (model(q) - window).ravel(), [scale, scale, sky, 0.0, 0.0, tx, ty, PSF],
                                      bounds=([0, 0, -numpy.inf, -9, -9, -14, -14, 0.3], [numpy.inf] * 3 + [9, 9, 14, 14, 4]))

    sky = numpy.median(window)
    scale = (window - sky).clip(0).sum() / max(cuts[0].sum() + cuts[1].sum(), 1e-12)
    # Dimorphos where it is drawn; then where Didymos alone leaves most light.
    # The better of the two.
    first = solve(0.0, 0.0)
    a, _, sky, sx, sy, _, _, s = first.x
    rest = ndimage.gaussian_filter(window - model([a, 0.0, sky, sx, sy, 0.0, 0.0, s]), 1.0)
    yy, xx = numpy.mgrid[:33, :33]
    rest[numpy.hypot(xx - 16 - sx, yy - 16 - sy) < 3.5] = -numpy.inf
    my, mx = numpy.unravel_index(numpy.argmax(rest), rest.shape)
    m = cuts[1][4:37, 4:37]
    if m.sum() > 0:
        second = solve(mx - sx - m.sum(axis=0) @ numpy.arange(33) / m.sum(), my - sy - m.sum(axis=1) @ numpy.arange(33) / m.sum())
        first = min(first, second, key=lambda f: f.cost)
    q = first.x
    return q[0], q[1], ox - dx + q[3], oy - dy + q[4], q[5], q[6], q[7], numpy.sqrt(numpy.mean(first.fun ** 2))


def cut(img, x, y):
    """32 x 32 pixels about (x, y), zero past the frame's edges."""
    x, y = int(round(x)), int(round(y))
    return numpy.pad(numpy.nan_to_num(img), 16)[y:y + 32, x:x + 32]


rows, crops, k = [], [], 0
while app.running:
    if sim.state.is_paused:
        app.step()
        continue

    dau = pose(et)
    imaging = k < len(images) and et >= images[k][0] - 1e-6
    if imaging or sim.state.iteration % 30 == 0:  # the temperatures shown
        for body, t in zip(sim.bodies, columns):
            body.mesh.values = t[0]
    if imaging:
        # Didymos where the image has it: SPICE's pointing turned onto it.
        observed = numpy.nan_to_num(fits.getdata(images[k][2]))
        ox, oy, snr = target(observed)
        aim = numpy.zeros(2)
        if snr >= 10.0:
            before = place(et)[0][0]
            seen = numpy.array([(511.5 - BLIND - ox) / focal, (383.5 - oy) / focal, 1.0])
            pose(et, spice.axisar(numpy.cross(before, seen), spice.vsep(before, seen)))
            aim = numpy.array([ox, oy]) - pixel(before)
        sim.request_facet_id()
    sim.huds[0].text = (f"{spice.et2utc(et, 'ISOC', 0)}  " + (f"image {k + 1}/{len(images)}, filter {images[k][1]}"
                        if imaging else "spin-up" if k == 0 else f"next image {k + 1}/{len(images)}"))
    if not app.step():
        break

    if imaging:
        # Each body's samples the radiance of the ground they see, averaged
        # into TIRI's pixels and spread by its PSF, cut as the products are.
        ids, offsets = sim.facet_id_map()
        parts = []
        for i, body in enumerate(sim.bodies):
            part = numpy.zeros(ids.shape)
            on = (ids > offsets[i]) & (ids <= offsets[i] + len(body.mesh.facets))
            part[on] = bands[images[k][1]](numpy.asarray(columns[i][0])[ids[on] - offsets[i] - 1])
            parts.append(part.reshape(768, SUPER, 1024, SUPER).mean(axis=(1, 3))[:, BLIND:])
        drawn = ndimage.gaussian_filter(parts[0] + parts[1], PSF)
        name = os.path.basename(images[k][2]).replace("tiri_par", "kalast_tiri")
        header = fits.Header([("DATE-OBS", spice.et2utc(et, "ISOC", 4)), ("FILTER", images[k][1]), ("BUNIT", "W m^-2 sr^-1"),
                              ("PSF", PSF, "[px] Gaussian sigma"), ("DIMSHIFT", DIMORPHOS_SHIFT, "[s] Dimorphos along its orbit")])
        fits.writeto(f"{OUT}/{name}", drawn.astype(numpy.float32), header, overwrite=True)

        ys, xs = numpy.nonzero(parts[0])
        if snr >= 10.0 and len(xs):
            w = parts[0][ys, xs]
            a, b, px, py, tx, ty, spread, rms = fit(observed, parts, (ox, oy), (w @ xs / w.sum(), w @ ys / w.sum()))
            # Dimorphos's move beyond Didymos's, as time along its orbit.
            v = (beside(et, DIMORPHOS_SHIFT + 60.0) - beside(et, DIMORPHOS_SHIFT - 60.0)) / 120.0
            lag = numpy.array([tx, ty]) @ v / (v @ v)
            rows.append((spice.et2utc(et, "ISOC", 3), images[k][1], aim[0] + px, aim[1] + py, tx, ty, lag, a, b, spread, rms, snr,
                         "measured" if measured(et) else "planned"))
            print("%s %s  pointing %+.1f %+.1f px  Dimorphos %+.1f %+.1f px, %+.0f s  signal %.1f, %.1f DN per W/m2/sr  "
                  "PSF %.2f px  rms %.1f DN" % rows[-1][:11] + f"  {rows[-1][-1]} attitude")
            crops.append((rows[-1][0], images[k][1], cut(observed, ox, oy), cut(drawn, ox - px, oy - py)))
        else:
            print(f"{spice.et2utc(et, 'ISOC', 3)} {images[k][1]}  no target in the image")
        k += 1
        if k == len(images):
            sim.state.toggle_pause()
            break
        continue  # the next image may be at this instant too

    # The model's step, up to the next image at most.
    h = min(dt, images[k][0] - et)
    for i, (t, ground) in enumerate(zip(columns, grounds)):
        light = sim.facet_illumination(i)
        core.solar_bc(t, dau[i], sim.facet_incidence(i) if light is None else light, ground, dz[i])
        core.bottom_adiabatic(t)
        core.heat_conduction(t, ground, h, dz[i])
    et += h

if rows:
    header = ("utc,filter,pointing_x,pointing_y,dimorphos_x,dimorphos_y,dimorphos_s,didymos_signal,dimorphos_signal,psf_px,rms_dn,"
              "peak_sigma,attitude")
    numpy.savetxt(f"{OUT}/tiri.csv", numpy.array(rows, dtype=object), fmt="%s", delimiter=",", header=header, comments="")
    r = {name: numpy.array([row[i] for row in rows]) for i, name in enumerate(header.split(","))}
    hours = numpy.array([(spice.str2et(t) - spice.str2et(rows[0][0])) / 3600.0 for t in r["utc"]])
    wide = r["filter"] == "g"
    held = r["attitude"] == "measured"
    if held.any():
        print(f"measured attitude, {held.sum()} images: pointing, observed - SPICE: x {numpy.median(r['pointing_x'][held]):+.1f} px, "
              f"y {numpy.median(r['pointing_y'][held]):+.1f} px")
    found = numpy.hypot(r["dimorphos_x"], r["dimorphos_y"]) < 3.0  # Dimorphos found beside its drawing
    if (wide & found & held).any():
        lag = numpy.median(r["dimorphos_s"][wide & found & held])
        print(f"Dimorphos, observed - drawn along its orbit: {lag:+.0f} s (median of {(wide & found & held).sum()} wide-band "
              f"images, measured attitude): DIMORPHOS_SHIFT = {DIMORPHOS_SHIFT + lag:.0f} would match them")

    fig = plt.figure(figsize=(15, 9.5))
    grid = fig.add_gridspec(3, 8, height_ratios=(1.0, 1.0, 1.3))
    ax = fig.add_subplot(grid[0, :6])
    for name, mark, color in (("pointing_x", "o", "C0"), ("pointing_y", "s", "C1")):
        for use, fill, att in ((held, color, "measured"), (~held, "none", "planned")):
            if use.any():
                ax.plot(hours[use], r[name][use], mark, color=color, mfc=fill, ms=4, label=f"Didymos, {name[-1]}, {att} attitude")
    ax.plot(hours, r["dimorphos_x"], "x", color="C2", label="Dimorphos beyond Didymos, x")
    ax.plot(hours, r["dimorphos_y"], "+", color="C3", label="Dimorphos beyond Didymos, y")
    ax.set_ylabel("observed - SPICE (px)")
    ax.legend(fontsize=8, ncol=2)
    ax.set_title(f"TIRI, {rows[0][0][:10]} to {rows[-1][0][:10]}: the bodies where the images have them, against kalast's drawings")
    ax = fig.add_subplot(grid[1, :6], sharex=ax)
    found = numpy.hypot(r["dimorphos_x"], r["dimorphos_y"]) < 3.0  # Dimorphos found beside its drawing
    for body, mark in (("didymos", "o"), ("dimorphos", "s")):
        signal = r[f"{body}_signal"].astype(float)
        for f in set(r["filter"]):
            signal[r["filter"] == f] /= numpy.median(r["didymos_signal"][r["filter"] == f])  # each filter its own calibration
        use = found if body == "dimorphos" else numpy.ones(len(signal), bool)
        ax.plot(hours[wide & use], signal[wide & use], mark, label=f"{body.capitalize()}, wide band g")
        ax.plot(hours[~wide & use], signal[~wide & use], "+" if body == "didymos" else "x", label=f"{body.capitalize()}, narrow bands a-f")
    ax.set_ylim(bottom=0.0)
    ax.set_xlabel(f"hours from {rows[0][0][:19]} UTC")
    ax.set_ylabel("observed / drawn signal,\nto Didymos's median a filter")
    ax.legend(fontsize=8, ncol=2)
    ax = fig.add_subplot(grid[:2, 6:])
    track = numpy.array([beside(spice.str2et(t), DIMORPHOS_SHIFT) for t in r["utc"]])
    ax.plot(track[:, 0], track[:, 1], "k.-", ms=3, lw=0.6, label="drawn")
    ax.plot(track[wide, 0] + r["dimorphos_x"][wide], track[wide, 1] + r["dimorphos_y"][wide], "o", ms=4, mfc="none", label="observed, g")
    ax.plot(0, 0, "o", color="0.6", ms=14)
    ax.set_aspect("equal")
    ax.set_title("Dimorphos beside Didymos (px)", fontsize=10)
    ax.legend(fontsize=8)
    for j, c in enumerate(numpy.linspace(0, len(crops) - 1, min(8, len(crops))).astype(int) if crops else []):
        time, f, seen, drawn = crops[c]
        ax = fig.add_subplot(grid[2, j])
        ax.imshow(numpy.vstack([seen / max(seen.max(), 1e-9), drawn / max(drawn.max(), 1e-9)]),
                  cmap="inferno", origin="lower", vmin=0.0, vmax=1.0)
        ax.set_title(f"{time[5:10]} {time[11:19]} {f}", fontsize=8)
        ax.set_xticks([])
        ax.set_yticks([16, 48], ["TIRI", "kalast"] if j == 0 else ["", ""], fontsize=8)
    fig.tight_layout()
    fig.savefig(f"{OUT}/tiri.png", dpi=110)
    print(f"wrote {OUT}/")
    if sys.platform == "win32":
        os.startfile(f"{OUT}/tiri.png")
    else:
        subprocess.Popen(["open" if sys.platform == "darwin" else "xdg-open", f"{OUT}/tiri.png"])

while app.running and sim.state.is_paused:
    app.step()

spice.kclear()
