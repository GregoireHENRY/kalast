#!/usr/bin/env python
"""The HRSC SRC images beside kalast's, from `main.py`'s runs.

Shadows: each image cut to lit or not, a pixel lit above a fifth of the
image's brightest (its 99.5th percentile) -- kalast's `mask` runs lit
wherever the Sun reaches -- scored by the intersection over union of the
lit pixels, as the image is pointed by SPICE and moved to where the two
overlap best. Brightness: where both are lit, the image over kalast's I/F,
one scale fitted, the camera not being absolutely calibrated.

    python compare.py [RUN ...]       # out/phobos_hrsc/* by default

Figures and table.md go to out/phobos_hrsc/figures/.
"""

import glob
import os
import sys

import matplotlib
import numpy
from PIL import Image

import hrsc

matplotlib.use("agg")
import matplotlib.pyplot as plt  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join("out", "phobos_hrsc", "figures")
LIT = 0.2
IFOV = 9e-3 / 984.76


def kalast_image(run, name):
    """The reference's floats where there are, else the exported frame."""
    mean = os.path.join(run, name, "mean.npy")
    if os.path.exists(mean):
        return numpy.load(mean)[..., 0].astype(float)
    frame = sorted(glob.glob(os.path.join(run, name, "*.png")))[-1]
    return numpy.asarray(Image.open(frame).convert("L")).astype(float) / 255.0


def lit(img, geometric=False):
    if geometric:
        # Lit at all: the frame saturates wherever the Sun reaches, and the
        # reference's floats are not clamped -- a pixel lit over half its area.
        return numpy.minimum(img, 1.0) > 0.5
    return img > LIT * numpy.percentile(img, 99.5)


def moved(a, dx, dy):
    out = numpy.zeros_like(a)
    h, w = a.shape
    out[max(dy, 0): h + min(dy, 0), max(dx, 0): w + min(dx, 0)] = a[max(-dy, 0): h - max(dy, 0), max(-dx, 0): w - max(dx, 0)]
    return out


def iou(a, b):
    return (a & b).sum() / max((a | b).sum(), 1)


def best_move(real, sim, reach=240):
    """Where `sim` overlaps `real` best: searched a quarter size out to
    `reach` pixels, then at full size."""
    def quarter(m):
        h, w = m.shape[0] // 4 * 4, m.shape[1] // 4 * 4
        return m[:h, :w].reshape(h // 4, 4, w // 4, 4).mean(axis=(1, 3)) > 0.5

    rq, sq, r = quarter(real), quarter(sim), reach // 4
    _, x, y = max((iou(rq, moved(sq, dx, dy)), dx, dy) for dy in range(-r, r + 1) for dx in range(-r, r + 1))
    _, x, y = max((iou(real, moved(sim, dx, dy)), dx, dy) for dy in range(4 * y - 4, 4 * y + 5) for dx in range(4 * x - 4, 4 * x + 5))
    return x, y


def score(real, sim, geometric):
    r, s = lit(real), lit(sim, geometric)
    dx, dy = best_move(r, s)
    out = dict(iou=iou(r, s), dx=dx, dy=dy, iou_moved=iou(r, moved(s, dx, dy)))
    if not geometric:
        sim = moved(sim, dx, dy)
        both = r & lit(sim)
        k = numpy.median(real[both] / sim[both])
        ratio = real[both] / (k * sim[both])
        out.update(scale=k, p16=numpy.percentile(ratio, 16), p84=numpy.percentile(ratio, 84),
                   r=numpy.corrcoef(real[both], sim[both])[0, 1])
    return out, r, s


def figures(name, time, real, sim, s, r, m, title):
    blue_red = matplotlib.colors.ListedColormap(["#2b6cb0", "#101010", "#c53030"])
    with plt.style.context("dark_background"):
        fig, ax = plt.subplots(1, 4, figsize=(20, 5.6))
        ax[0].imshow(r, cmap="gray"), ax[0].set_title("image, lit or not")
        ax[1].imshow(m, cmap="gray"), ax[1].set_title(title)
        ax[2].imshow(m.astype(int) - r.astype(int), cmap=blue_red, vmin=-1, vmax=1)
        ax[2].set_title(f"kalast - image, as SPICE points: IoU {s['iou']:.3f}")
        ax[3].imshow(moved(m, s["dx"], s["dy"]).astype(int) - r.astype(int), cmap=blue_red, vmin=-1, vmax=1)
        ax[3].set_title(f"moved ({s['dx']:+d}, {s['dy']:+d}) px: IoU {s['iou_moved']:.3f}")
        fig.suptitle(f"{name}  {time}   red: lit in kalast only, blue: in the image only")
        for a in ax:
            a.set_axis_off()
        fig.tight_layout()
        fig.savefig(os.path.join(OUT, f"shadows_{name}_{title.replace(', ', '_').replace(' ', '_')}.png"), dpi=72)
        plt.close(fig)
        if "scale" not in s:
            return
        sim = moved(sim, s["dx"], s["dy"]) * s["scale"]
        ratio = numpy.where(r & lit(sim), real / numpy.maximum(sim, 1e-9), numpy.nan)
        hi = numpy.percentile(real, 99.5)
        fig, ax = plt.subplots(1, 3, figsize=(16, 5.8))
        ax[0].imshow(real, cmap="gray", vmin=0, vmax=hi), ax[0].set_title("image")
        ax[1].imshow(sim, cmap="gray", vmin=0, vmax=hi), ax[1].set_title(title + ", one scale, moved")
        shown = ax[2].imshow(ratio, cmap="RdBu_r", vmin=0.5, vmax=1.5)
        ax[2].set_title(f"image / kalast: 16-84 % {s['p16']:.2f}-{s['p84']:.2f}, r {s['r']:.2f}")
        fig.colorbar(shown, ax=ax[2], fraction=0.046)
        fig.suptitle(f"{name}  {time}   where both are lit")
        for a in ax:
            a.set_axis_off()
        fig.tight_layout()
        fig.savefig(os.path.join(OUT, f"brightness_{name}_{title.replace(', ', '_').replace(' ', '_')}.png"), dpi=72)
        plt.close(fig)


os.makedirs(OUT, exist_ok=True)
runs = sys.argv[1:] or sorted(d for d in glob.glob(os.path.join("out", "phobos_hrsc", "*_*_*")) if os.path.isdir(d))
images = [(os.path.basename(p)[:10], *hrsc.read(p)) for p in sorted(glob.glob(os.path.join(HERE, "*.img")))]
rows = ["| run | image | IoU | moved (px) | IoU moved | image / kalast, 16-84 % | r |", "|---|---|---|---|---|---|---|"]
for run in runs:
    method, law, mesh = os.path.basename(run).split("_")
    for name, real, label in images:
        if not os.path.isdir(os.path.join(run, name)):
            continue
        sim = kalast_image(run, name)
        s, r, m = score(real, sim, geometric=(law == "mask"))
        figures(name, label["time"], real, sim, s, r, m, f"{method}, {law}, {mesh}")
        brightness = f"{s['p16']:.2f}-{s['p84']:.2f} | {s['r']:.2f}" if "scale" in s else " | "
        rows.append(f"| {method} {law} {mesh} | {name} | {s['iou']:.3f} | {s['dx']:+d}, {s['dy']:+d} | {s['iou_moved']:.3f} | {brightness} |")
        print(rows[-1], flush=True)
open(os.path.join(OUT, "table.md"), "w").write("\n".join(rows) + "\n")
