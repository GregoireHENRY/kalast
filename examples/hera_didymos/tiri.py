#!/usr/bin/env python
"""What TIRI saw: Didymos and Dimorphos in the thermal infrared at the times of
the TIRI images decoded by edds_decoder, beside the images themselves.

The surface temperatures are kalast's thermophysical model, spun up SPINUP days
before the first image under the Sun as SPICE has it, the shadows from the
frames. Each pixel is the radiance of the ground it sees in the image's filter,
averaged over SUPER x SUPER samples, written as FITS in the products' rows and
columns. tiri.csv and tiri.png set each image beside its drawing: where the
target is in both, and its signal."""
import glob
import os
import subprocess
import sys

import matplotlib
import numpy
import spiceypy as spice
from astropy.io import fits

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
ALIGN = True  # TIRI's boresight as measured in flight, kalast.tiri_alignment
BLIND = 6  # columns of the raw frame the products leave out, the first
INSTR = "HERA_TIRI"

spice.kclear()
spice.furnsh(f"{KERNELS}/hera_plan.tm")  # Didymos and Dimorphos
spice.furnsh(f"{KERNELS}/hera_ops.tm")  # Hera's measured attitude, over the plan's
_, _, _, _, bounds = spice.getfov(spice.bodn2c(INSTR), 4)
fovy = 2.0 * numpy.arctan2(abs(bounds[0][1]), bounds[0][2])
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


def pose(et):
    """Bodies and Sun in TIRI's frame; each body's distance to the Sun (AU)."""
    sun = turn @ spice.spkpos("SUN", et, INSTR, "LT+S", "HERA")[0]
    for body, name in zip(sim.bodies, MESHES):
        body.mat[:3, :3] = turn @ spice.pxform(f"{name}_FIXED", INSTR, et)
        body.mat[:3, 3] = turn @ spice.spkpos(name, et, INSTR, "LT+S", "HERA")[0]
    sim.sun.pos = sun
    return [numpy.linalg.norm(sun - body.mat[:3, 3]) / AU_KM for body in sim.bodies]


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
    (x, y) above the sky, its summed signal and its peak in noise units."""
    smooth = numpy.median(numpy.lib.stride_tricks.sliding_window_view(numpy.pad(img, 1, mode="edge"), (3, 3)), axis=(2, 3))
    y, x = numpy.unravel_index(numpy.argmax(smooth), img.shape)
    noise = 1.4826 * numpy.median(numpy.abs(numpy.diff(img, axis=1))) / numpy.sqrt(2)
    sky = numpy.median(img[max(y - 15, 0):y + 16, max(x - 15, 0):x + 16])
    ys, xs = numpy.mgrid[max(y - 5, 0):y + 6, max(x - 5, 0):x + 6]
    w = numpy.clip(img[ys, xs] - sky, 0.0, None)
    return (w * xs).sum() / w.sum(), (w * ys).sum() / w.sum(), w.sum(), (smooth[y, x] - sky) / noise


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
        sim.request_facet_id()
    sim.huds[0].text = (f"{spice.et2utc(et, 'ISOC', 0)}  " + (f"image {k + 1}/{len(images)}, filter {images[k][1]}"
                        if imaging else "spin-up" if k == 0 else f"next image {k + 1}/{len(images)}"))
    if not app.step():
        break

    if imaging:
        # Each sample the radiance of the ground it sees, averaged into
        # TIRI's pixels, the frame cut as the products are.
        ids, offsets = sim.facet_id_map()
        drawn = numpy.zeros(ids.shape)
        for i, body in enumerate(sim.bodies):
            on = (ids > offsets[i]) & (ids <= offsets[i] + len(body.mesh.facets))
            drawn[on] = bands[images[k][1]](numpy.asarray(columns[i][0])[ids[on] - offsets[i] - 1])
        drawn = drawn.reshape(768, SUPER, 1024, SUPER).mean(axis=(1, 3))[:, BLIND:]
        name = os.path.basename(images[k][2]).replace("tiri_par", "kalast_tiri")
        header = fits.Header([("DATE-OBS", spice.et2utc(et, "ISOC", 4)), ("FILTER", images[k][1]), ("BUNIT", "W m^-2 sr^-1")])
        fits.writeto(f"{OUT}/{name}", drawn.astype(numpy.float32), header, overwrite=True)

        observed = numpy.nan_to_num(fits.getdata(images[k][2]))
        ox, oy, signal, snr = target(observed)
        ys, xs = numpy.nonzero(drawn)
        px, py = ((drawn[ys, xs] * xs).sum() / drawn.sum(), (drawn[ys, xs] * ys).sum() / drawn.sum()) if len(xs) else (numpy.nan,) * 2
        if snr < 10.0:  # no target in the image
            ox = oy = signal = numpy.nan
        rows.append((spice.et2utc(et, "ISOC", 3), images[k][1], px, py, ox, oy, signal, drawn.sum(), snr))
        print("%s %s  drawn (%.1f, %.1f)  observed (%.1f, %.1f)  signal %.0f DN, %.2f W/m2/sr, peak %.0f sigma" % rows[-1])
        if numpy.isfinite(ox) and len(xs):
            crops.append((rows[-1][0], images[k][1], cut(observed, ox, oy), cut(drawn, px, py)))
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
    header = "utc,filter,drawn_x,drawn_y,observed_x,observed_y,observed_dn,drawn_radiance,peak_sigma"
    numpy.savetxt(f"{OUT}/tiri.csv", numpy.array(rows, dtype=object), fmt="%s", delimiter=",", header=header, comments="")
    hours = numpy.array([(spice.str2et(r[0]) - spice.str2et(rows[0][0])) / 3600.0 for r in rows])
    letter = numpy.array([r[1] for r in rows])
    dx = numpy.array([r[4] - r[2] for r in rows])
    dy = numpy.array([r[5] - r[3] for r in rows])
    ratio = numpy.array([r[6] / r[7] if r[7] > 0 else numpy.nan for r in rows])
    for f in set(letter):
        ratio[letter == f] /= numpy.nanmedian(ratio[letter == f])  # each filter its own calibration
    print(f"observed - drawn: x {numpy.nanmedian(dx):+.1f} px, y {numpy.nanmedian(dy):+.1f} px (median of {numpy.isfinite(dx).sum()})")

    fig = plt.figure(figsize=(14, 9))
    grid = fig.add_gridspec(3, 8, height_ratios=(1.0, 1.0, 1.3))
    ax = fig.add_subplot(grid[0, :])
    ax.plot(hours, dx, "o", label="x (columns)")
    ax.plot(hours, dy, "s", label="y (rows)")
    ax.set_ylabel("observed - drawn (px)")
    ax.legend()
    ax.set_title(f"TIRI, {rows[0][0][:10]}: the target where the image has it, against kalast's drawing")
    ax = fig.add_subplot(grid[1, :], sharex=ax)
    ax.plot(hours[letter == "g"], ratio[letter == "g"], "o", label="wide band g")
    ax.plot(hours[letter != "g"], ratio[letter != "g"], "+", label="narrow bands a-f")
    ax.set_xlabel(f"hours from {rows[0][0][:19]} UTC")
    ax.set_ylabel("observed / drawn signal,\nto its filter's median")
    ax.legend()
    for j, c in enumerate(numpy.linspace(0, len(crops) - 1, min(8, len(crops))).astype(int) if crops else []):
        time, f, seen, drawn = crops[c]
        ax = fig.add_subplot(grid[2, j])
        ax.imshow(numpy.vstack([seen / max(seen.max(), 1e-9), drawn / max(drawn.max(), 1e-9)]),
                  cmap="inferno", origin="lower", vmin=0.0, vmax=1.0)
        ax.set_title(f"{time[11:19]} {f}", fontsize=8)
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
