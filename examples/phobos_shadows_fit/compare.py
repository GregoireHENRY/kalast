#!/usr/bin/env python
"""The HRSC SRC images beside kalast's, from `main.py`'s runs.

Shadows: each image cut to lit or not, a pixel lit above a fifth of the
image's brightest (its 99.5th percentile) -- kalast's `mask` runs lit
wherever the Sun reaches -- scored by the intersection over union of the
lit pixels as pointed. Pointing: where kalast's craters fall on the
image's, what `main.py`'s `POINTING` leaves. Brightness, where both are lit:
the image over kalast's I/F, one scale fitted, the camera not being
absolutely calibrated; and over kalast's through the SRC's response
(`hrsc.response`), a scale and an offset fitted, with the shadows' level.

    python compare.py [RUN ...]       # out/phobos_hrsc/* by default

Figures and table.md go to out/phobos_hrsc/figures/.
"""

import glob
import os
import sys

import matplotlib
import numpy
from PIL import Image
from scipy import ndimage

import hrsc

matplotlib.use("agg")
import matplotlib.pyplot as plt  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join("out", "phobos_hrsc", "figures")
LIT = 0.2


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


def texture(img):
    """The craters: brightness in log, high-passed, the unlit zeroed."""
    t = numpy.log(numpy.maximum(img, 1e-3 * img.max()))
    t = t - ndimage.gaussian_filter(t, 6)
    t[~lit(img)] = 0
    return ndimage.gaussian_filter(t, 1.0)


def register(real, sim, reach=150):
    """Where `sim`'s craters fall on `real`'s, (dx, dy) pixels to move it by:
    the peak of their phase correlation, to a fraction of a pixel."""
    a, b = texture(real), texture(sim)
    h, w = a.shape
    r = numpy.fft.rfft2(numpy.pad(a, ((0, h), (0, w)))) * numpy.conj(numpy.fft.rfft2(numpy.pad(b, ((0, h), (0, w)))))
    c = numpy.fft.irfft2(r / numpy.maximum(numpy.abs(r), 1e-12), s=(2 * h, 2 * w))
    c = numpy.fft.fftshift(ndimage.gaussian_filter(c, 1.5, mode="wrap"))[h - reach: h + reach + 1, w - reach: w + reach + 1]
    y, x = numpy.unravel_index(numpy.argmax(c), c.shape)
    y, x = min(max(y, 1), 2 * reach - 1), min(max(x, 1), 2 * reach - 1)

    def vertex(m, z, p):
        return 0.5 * (m - p) / (m - 2 * z + p) if m - 2 * z + p else 0.0

    return x - reach + vertex(*c[y, x - 1: x + 2]), y - reach + vertex(*c[y - 1: y + 2, x])


def score(name, real, sim, geometric):
    r, s = lit(real), lit(sim, geometric)
    out = dict(iou=(r & s).sum() / max((r | s).sum(), 1))
    if geometric:
        return out, r, s
    out["dx"], out["dy"] = register(real, sim)
    both = r & s
    k = numpy.median(real[both] / sim[both])
    ratio = real[both] / (k * sim[both])
    out.update(p16=numpy.percentile(ratio, 16), p84=numpy.percentile(ratio, 84), r=numpy.corrcoef(real[both], sim[both])[0, 1])
    # Through the SRC: the scale on the lit ground, the offset -- light the
    # frame has from beyond it, or none -- in the shadows and the sky.
    m = hrsc.response(sim, name)
    both, dark = r & lit(m), ~r & ~lit(m)
    k = numpy.median(real[both] / m[both])
    b = max(numpy.median(real[dark] - k * m[dark]), 0.0)
    k = numpy.median((real[both] - b) / m[both])
    m = k * m + b
    ratio = real[both] / m[both]
    out.update(src=m, q16=numpy.percentile(ratio, 16), q84=numpy.percentile(ratio, 84), qr=numpy.corrcoef(real[both], m[both])[0, 1],
               dark=numpy.median(real[dark]), dark_src=numpy.median(m[dark]))
    return out, r, s


def figures(name, time, real, sim, s, r, m, title):
    tag = title.replace(", ", "_").replace(" ", "_")
    blue_red = matplotlib.colors.ListedColormap(["#2b6cb0", "#101010", "#c53030"])
    with plt.style.context("dark_background"):
        fig, ax = plt.subplots(1, 3, figsize=(15, 5.6))
        ax[0].imshow(r, cmap="gray"), ax[0].set_title(f"{name}  {time}\nimage, lit or not")
        ax[1].imshow(m, cmap="gray"), ax[1].set_title(title)
        ax[2].imshow(m.astype(int) - r.astype(int), cmap=blue_red, vmin=-1, vmax=1)
        ax[2].set_title(f"kalast - image: IoU {s['iou']:.3f}\nred: lit in kalast only, blue: in the image only")
        for a in ax:
            a.set_axis_off()
        fig.tight_layout()
        fig.savefig(os.path.join(OUT, f"shadows_{name}_{tag}.png"), dpi=72, bbox_inches="tight")
        plt.close(fig)
        if "src" not in s:
            return
        hi = numpy.percentile(real, 99.5)
        k = numpy.median(real[r & lit(sim)] / sim[r & lit(sim)])
        ratio = numpy.where(r & lit(s["src"]), real / numpy.maximum(s["src"], 1e-9), numpy.nan)
        fig, ax = plt.subplots(1, 4, figsize=(21, 5.8))
        # Square roots: the shadows and the sky show.
        ax[0].imshow(numpy.sqrt(numpy.clip(real / hi, 0, 1)), cmap="gray", vmin=0, vmax=1), ax[0].set_title(f"{name}  {time}\nimage")
        ax[1].imshow(numpy.sqrt(numpy.clip(k * sim / hi, 0, 1)), cmap="gray", vmin=0, vmax=1), ax[1].set_title(f"{title}, one scale")
        ax[2].imshow(numpy.sqrt(numpy.clip(s["src"] / hi, 0, 1)), cmap="gray", vmin=0, vmax=1)
        ax[2].set_title(f"through the SRC's response\nshadows {s['dark']:.0f} DN, kalast's {s['dark_src']:.0f}")
        shown = ax[3].imshow(ratio, cmap="RdBu_r", vmin=0.5, vmax=1.5)
        ax[3].set_title(f"image / kalast through the SRC\n16-84 % {s['q16']:.2f}-{s['q84']:.2f}, r {s['qr']:.2f}")
        fig.colorbar(shown, ax=ax[3], fraction=0.046)
        for a in ax:
            a.set_axis_off()
        fig.tight_layout()
        fig.savefig(os.path.join(OUT, f"brightness_{name}_{tag}.png"), dpi=72, bbox_inches="tight")
        plt.close(fig)


os.makedirs(OUT, exist_ok=True)
runs = sys.argv[1:] or sorted(d for d in glob.glob(os.path.join("out", "phobos_hrsc", "*_*_*")) if os.path.isdir(d))
images = [(os.path.basename(p)[:10], *hrsc.read(p)) for p in sorted(glob.glob(os.path.join(HERE, "*.img")))]
rows = ["| run | image | IoU | craters left (px) | image / kalast, 16-84 %, r | through the SRC, 16-84 %, r | shadows, image / kalast (DN) |",
        "|---|---|---|---|---|---|---|"]
for run in runs:
    method, law, mesh = os.path.basename(run).split("_")
    for name, real, label in images:
        if not os.path.isdir(os.path.join(run, name)):
            continue
        sim = kalast_image(run, name)
        s, r, m = score(name, real, sim, geometric=(law == "mask"))
        figures(name, label["time"], real, sim, s, r, m, f"{method}, {law}, {mesh}")
        if "src" in s:
            rest = (f"{s['dx']:+.1f}, {s['dy']:+.1f} | {s['p16']:.2f}-{s['p84']:.2f}, {s['r']:.2f} | "
                    f"{s['q16']:.2f}-{s['q84']:.2f}, {s['qr']:.2f} | {s['dark']:.0f} / {s['dark_src']:.0f}")
        else:
            rest = " | | | "
        rows.append(f"| {method} {law} {mesh} | {name} | {s['iou']:.3f} | {rest} |")
        print(rows[-1], flush=True)
open(os.path.join(OUT, "table.md"), "w").write("\n".join(rows) + "\n")
