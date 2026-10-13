#!/usr/bin/env python
"""The OSIRIS NAC images beside kalast's, from `main.py`'s runs.

Within the nucleus's outline -- the pixels kalast's facets cover, widened by
40 pixels, the coma and the stars left out -- each image cut to lit or not,
a pixel lit above a fifth of the brightest there (its 99.5th percentile;
kalast's `mask` runs lit wherever the Sun reaches), scored by the
intersection over union of the lit pixels as pointed. Pointing: where
kalast's terrain falls on the image's, what `images.csv`'s `dx` and `dy`
leave, to be trusted where the correlation's peak stands twice above any
other (`peak`); and the IoU there, kalast's image moved by it -- the shadows
with the pointing fitted, which it cannot then stand in for. Brightness,
where both are lit: the image's I/F over kalast's, the images being
calibrated.

    python compare.py [RUN ...]       # out/67p_osiris/* by default

Figures and table.md go to out/67p_osiris/figures/: each image beside
kalast's, and all of them on one sheet a run.
"""

import csv
import glob
import os
import sys

import matplotlib
import numpy
from PIL import Image, ImageDraw, ImageFont
from scipy import ndimage

import osiris

matplotlib.use("agg")
import matplotlib.pyplot as plt  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join("out", "67p_osiris", "figures")
EXPOSURE = {"hapke": 12.0, "hapke-fit": 12.0, "lambert": 0.8, "mask": 1e4}  # main.py's
LIT = 0.2
PEAK = 2.0


def kalast_image(run, name, law):
    """kalast's I/F: the reference's floats where there are, else the
    exported frame."""
    mean = os.path.join(run, name, "mean.npy")
    if os.path.exists(mean):
        return numpy.load(mean)[..., 0].astype(float) / EXPOSURE[law]
    frame = sorted(glob.glob(os.path.join(run, name, "*.png")))[-1]
    return numpy.asarray(Image.open(frame).convert("L")).astype(float) / 255.0 / EXPOSURE[law]


def lit(img, roi, geometric=False):
    if geometric:
        return (img * EXPOSURE["mask"] > 0.5) & roi
    return (img > LIT * numpy.percentile(img[roi], 99.5)) & roi


def texture(img, roi):
    """The terrain: brightness in log, high-passed, the unlit zeroed."""
    t = numpy.log(numpy.maximum(img, 1e-3 * max(img[roi].max(), 1e-9)))
    t = t - ndimage.gaussian_filter(t, 6)
    t[~lit(img, roi)] = 0
    return ndimage.gaussian_filter(t, 1.0)


def register(a, b, reach=120):
    """Where `b`'s terrain falls on `a`'s, (dx, dy) pixels to move it by --
    the peak of their phase correlation, to a fraction of a pixel -- and the
    peak over the highest beyond 8 pixels of it."""
    h, w = a.shape
    r = numpy.fft.rfft2(numpy.pad(a, ((0, h), (0, w)))) * numpy.conj(numpy.fft.rfft2(numpy.pad(b, ((0, h), (0, w)))))
    c = numpy.fft.irfft2(r / numpy.maximum(numpy.abs(r), 1e-12), s=(2 * h, 2 * w))
    c = numpy.fft.fftshift(ndimage.gaussian_filter(c, 1.5, mode="wrap"))[h - reach: h + reach + 1, w - reach: w + reach + 1]
    y, x = numpy.unravel_index(numpy.argmax(c), c.shape)
    others = c.copy()
    others[max(y - 8, 0): y + 9, max(x - 8, 0): x + 9] = -numpy.inf
    peak = c[y, x] / others.max()
    y, x = min(max(y, 1), 2 * reach - 1), min(max(x, 1), 2 * reach - 1)

    def vertex(m, z, p):
        return 0.5 * (m - p) / (m - 2 * z + p) if m - 2 * z + p else 0.0

    return x - reach + vertex(*c[y, x - 1: x + 2]), y - reach + vertex(*c[y - 1: y + 2, x]), peak


def score(real, sim, roi, geometric):
    r, s = lit(real, roi), lit(sim, roi, geometric)
    out = dict(iou=(r & s).sum() / max((r | s).sum(), 1))
    if geometric or not s.any():
        return out, r, s
    out["dx"], out["dy"], out["peak"] = register(texture(real, roi), texture(sim, roi))
    if out["peak"] >= PEAK:
        there = lit(ndimage.shift(sim, (out["dy"], out["dx"]), order=1), roi)
        out["iou_there"] = (r & there).sum() / max((r | there).sum(), 1)
    both = r & s
    if both.sum() > 100:
        ratio = real[both] / sim[both]
        out.update(ratio=numpy.median(ratio), p16=numpy.percentile(ratio, 16), p84=numpy.percentile(ratio, 84),
                   r=numpy.corrcoef(real[both], sim[both])[0, 1])
    return out, r, s


def stretch(img, hi):
    """Square roots, on the image's scale: the shadows show."""
    return numpy.sqrt(numpy.clip(img / hi, 0, 1))


def figure(name, label, real, sim, s, r, m, title):
    hi = max(numpy.percentile(real, 99.8), 1e-9)
    blue_red = matplotlib.colors.ListedColormap(["#2b6cb0", "#101010", "#c53030"])
    with plt.style.context("dark_background"):
        fig, ax = plt.subplots(1, 4, figsize=(22, 5.9))
        ax[0].imshow(stretch(real, hi), cmap="gray", vmin=0, vmax=1)
        ax[0].set_title(f"{name}, {label['phase']:.0f} deg, {label['distance']:.0f} km\nimage")
        ax[1].imshow(stretch(sim, hi), cmap="gray", vmin=0, vmax=1)
        ax[1].set_title(f"{title}\nthe same I/F scale")
        ax[2].imshow(m.astype(int) - r.astype(int), cmap=blue_red, vmin=-1, vmax=1)
        ax[2].set_title(f"kalast - image: IoU {s['iou']:.3f}\nred: lit in kalast only, blue: in the image only")
        if "r" in s:
            ratio = numpy.where(r & m, real / numpy.maximum(sim, 1e-12), numpy.nan)
            shown = ax[3].imshow(ratio, cmap="RdBu_r", norm=matplotlib.colors.LogNorm(0.5, 2.0))
            ax[3].set_title(f"image / kalast\n16-84 % {s['p16']:.2f}-{s['p84']:.2f}, r {s['r']:.2f}")
            bar = fig.colorbar(shown, ax=ax[3], fraction=0.046)
            bar.ax.minorticks_off()
            bar.set_ticks([0.5, 0.7, 1.0, 1.4, 2.0], labels=["0.5", "0.7", "1", "1.4", "2"])
        for a in ax:
            a.set_axis_off()
        fig.tight_layout()
        fig.savefig(os.path.join(OUT, f"{name}_{title.replace(', ', '_')}.jpg"), dpi=150, bbox_inches="tight",
                    pil_kwargs={"quality": 90})
        plt.close(fig)


def sheet(pairs, path, cols=4, size=300):
    """Every image beside kalast's by phase, each pair on the image's scale."""
    rows = (len(pairs) + cols - 1) // cols
    canvas = Image.new("RGB", (cols * (2 * size + 16), rows * (size + 28)), (12, 12, 12))
    draw = ImageDraw.Draw(canvas)
    font = ImageFont.load_default(size=15)
    for k, (_, caption, real, sim) in enumerate(sorted(pairs, key=lambda p: p[0])):
        hi = max(numpy.percentile(real, 99.8), 1e-9)
        x, y = (k % cols) * (2 * size + 16), (k // cols) * (size + 28)
        for i, img in enumerate((real, sim)):
            tile = Image.fromarray((stretch(img, hi) * 255).astype(numpy.uint8))
            canvas.paste(tile.resize((size, size), Image.LANCZOS), (x + i * (size + 4), y))
        draw.text((x + 2, y + size + 5), caption, fill=(230, 230, 230), font=font)
    canvas.save(path, quality=90)


os.makedirs(OUT, exist_ok=True)
runs = sys.argv[1:] or sorted(d for d in glob.glob(os.path.join("out", "67p_osiris", "*_*_*")) if os.path.isdir(d))
images = list(csv.DictReader(open(os.path.join(HERE, "images.csv"))))
rows = ["| run | image | phase (deg) | distance (km) | IoU | terrain left (px) | peak | IoU there | image / kalast, 16-84 %, r |",
        "|---|---|---|---|---|---|---|---|---|"]
for run in runs:
    method, law, mesh = os.path.basename(run).split("_")
    pairs = []
    for image in images:
        name = image["product"][:16]
        if not os.path.exists(os.path.join(run, name, "ids.npy")):
            continue
        real, label = osiris.read(os.path.join(osiris.DATA, image["product"]))
        sim = kalast_image(run, name, law)
        ids = numpy.load(os.path.join(run, name, "ids.npy"))
        caption = f"{name}  {label['phase']:.0f} deg, {label['distance']:.0f} km"
        if not ids.any():
            pairs.append((label["phase"], caption, real, sim))
            rows.append(f"| {method} {law} {mesh} | {name} | {label['phase']:.1f} | {label['distance']:.1f} | no nucleus | | | | |")
            continue
        roi = ndimage.binary_dilation(ids > 0, iterations=40)
        s, r, m = score(real, sim, roi, geometric=(law == "mask"))
        if law == "mask":
            # Lit or not, drawn as bright as the image's brightest.
            sim = m * numpy.percentile(real, 99.8)
        pairs.append((label["phase"], caption, real, sim))
        figure(name, label, real, sim, s, r, m, f"{method}, {law}, {mesh}")
        there = f"{s['iou_there']:.3f}" if "iou_there" in s else ""
        moved = f"{s['dx']:+.1f}, {s['dy']:+.1f} | {s['peak']:.1f} | {there}" if "dx" in s else " | | "
        light = f"{s['ratio']:.2f}, {s['p16']:.2f}-{s['p84']:.2f}, {s['r']:.2f}" if "ratio" in s else ""
        rows.append(f"| {method} {law} {mesh} | {name} | {label['phase']:.1f} | {label['distance']:.1f} | {s['iou']:.3f} | {moved} | {light} |")
        print(rows[-1], flush=True)
    sheet(pairs, os.path.join(OUT, f"sheet_{os.path.basename(run)}.jpg"))
open(os.path.join(OUT, "table.md"), "w").write("\n".join(rows) + "\n")
