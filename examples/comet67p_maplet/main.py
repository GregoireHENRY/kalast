#!/usr/bin/env python
"""Comet 67P's relief finer than its shape model, from the images that see
it: a maplet 120 m square at 0.5 m over Preusker et al.'s SHAP7, in Wosret
beside Abydos -- Philae's last site -- solved by kalast.shape from 209
Rosetta OSIRIS NAC images of 2014 and 2016, and checked on 13 more, from
five days held out.

    python main.py

Each image downloads on first use, only its lines that see the patch
(osiris.py, a few MB each). In `out/67p_maplet/`: the maplet (`maplet.npz`,
and `maplet.obj` in the comet's frame, to load beside SHAP7), and three
held-out images beside SHAP7 and the maplet drawn in them
(`held_out.png`).
"""

import csv
import os
import re
import sys

import matplotlib.pyplot as plt
import numpy
import spiceypy as spice
from scipy import ndimage

from kalast.scattering import Hapke
from kalast.shape import Maplet, View
from kalast.util import RPD

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "comet67p_shadows_fit"))
import osiris  # noqa: E402

# Where these are kept here: set them to yours.
SHAP7 = "/Users/gregoireh/data/mesh/67p/shap7/cg-dlr_spg-shap7-v1.0.ply"
KERNELS = os.path.join(HERE, "..", "comet67p_shadows_fit", "rosetta.tm")
OUT = "out/67p_maplet"

# The patch, in the comet's frame (km): its centre and mean normal.
CENTRE = [2.436, -0.123, -0.333]
NORMAL = [0.92, -0.31, -0.24]
# `hapke-fit` of comet67p_shadows_fit: Hapke's law fitted to 57 NAC images.
LAW = Hapke(w=0.0356, b=0.276, c=1.0, b0=4.73, h=0.061, theta_bar=24.4 * RPD, k=1.2)
IFOV = 18.82e-6  # the NAC's pixel, rad


def image(row):
    """An image's lines that see the patch, in I/F, and its camera and Sun
    in the comet's frame at mid-exposure, from SPICE."""
    path = osiris.fetch(row["dataset"], row["product"], rows=(int(row["first"]), int(row["last"])))
    pixels, label = osiris.read(path)
    et = spice.str2et(label["start"]) + label["exposure"] / 2
    to_nac = numpy.array(spice.pxform("J2000", "ROS_OSIRIS_NAC", et))
    comet, lt = spice.spkpos("CHURYUMOV-GERASIMENKO", et, "J2000", "LT+S", "ROSETTA")
    sun, _ = spice.spkpos("SUN", et, "J2000", "LT+S", "ROSETTA")
    body = numpy.array(spice.pxform("67P/C-G_CK", "J2000", et - lt))
    toward = body.T @ (numpy.array(sun) - numpy.array(comet))
    # Where the boresight falls among these pixels: a window's offset on the
    # CCD, then the first line fetched. Lines run along the NAC's +X.
    (x0, x1), (y0, y1) = label["x"], label["y"]
    sample = 0.0 if label["samples"] == 2048 else 1024 - (x0 + x1) / 2
    line = 0.0 if label["lines"] == 2048 else (y0 + y1) / 2 - 1024
    return dict(
        pixels=pixels,
        position=(-body.T @ numpy.array(comet)).tolist(),
        axes=(to_nac @ body)[[1, 0, 2]].tolist(),
        sun=(toward / numpy.linalg.norm(toward)).tolist(),
        centre=(0.5 * label["samples"] - sample, 0.5 * label["lines"] - line - label["first"]),
        name=row["product"][:16],
        pixel=float(row["m_per_px"]),
    )


def view(d, shift=(0.0, 0.0)):
    """The image as kalast.shape takes it, its pointing moved by `shift`."""
    centre = (d["centre"][0] + shift[0], d["centre"][1] + shift[1])
    return View(d["pixels"], d["position"], d["axes"], 1 / IFOV, d["sun"], centre=centre, valid=d["pixels"] > 0, name=d["name"])


def ply(path):
    """A binary PLY of triangles: its vertices and facets."""
    raw = open(path, "rb").read()
    end = raw.index(b"end_header\n") + len(b"end_header\n")
    nv, nf = (int(re.search(rb"element " + e + rb" (\d+)", raw[:end])[1]) for e in (b"vertex", b"face"))
    vertices = numpy.frombuffer(raw, "<f4", 3 * nv, end).reshape(-1, 3)
    facets = numpy.frombuffer(raw, [("n", "u1"), ("i", "<i4", 3)], nf, end + 12 * nv)["i"]
    return vertices, facets.astype(numpy.uint32)


rows = list(csv.DictReader(open(os.path.join(HERE, "images.csv"))))
spice.furnsh(KERNELS)
images = [image(r) for r in rows]
spice.kclear()
train = [view(d) for d, r in zip(images, rows) if r["held"] == "0"]
held = [d for d, r in zip(images, rows) if r["held"] == "1"]

maplet = Maplet.from_surface(*ply(SHAP7), CENTRE, 241, 0.0005, normal=NORMAL)
shap7 = Maplet(maplet.centre, NORMAL, maplet.spacing, maplet.heights)
# Three levels, 2 m to 0.5 m; each image's pointing searched up to 12 m on
# the ground as it joins its level; an offset per image, for the light
# scattered into shadows; the albedo held near its neighbours', and real
# images' noise by a stronger curvature penalty than the default.
report = maplet.solve(train, LAW, iterations=12, levels=3, offset=True, reach=0.012, albedo_sigma=0.05, curvature=0.02,
                      verbose=True)
print(report)

os.makedirs(OUT, exist_ok=True)
numpy.savez(os.path.join(OUT, "maplet.npz"), heights=maplet.heights, albedo=maplet.albedo, shap7=shap7.heights,
            centre=maplet.centre, axes=maplet.axes, spacing=maplet.spacing)
vertices, facets = maplet.mesh()
with open(os.path.join(OUT, "maplet.obj"), "w") as f:
    numpy.savetxt(f, vertices, fmt="v %.7f %.7f %.7f")
    numpy.savetxt(f, facets + 1, fmt="f %d %d %d")

# The held-out images: each model registered on each image on its own, as
# far as the pointing is off, drawn in it, and correlated with it where it
# shows the patch lit -- at its pixels, and smoothed to half the maplet's
# spacing, the model's own scale.
scores = {"SHAP7": [], "maplet": []}
figure = []
for d in held:
    drawn = {}
    for label, model in (("SHAP7", shap7), ("maplet", maplet)):
        probe = Maplet(model.centre, NORMAL, model.spacing, model.heights, model.albedo)
        r = probe.solve([view(d)], LAW, iterations=0, offset=True, reach=0.012, resolution=8.0).views[0]
        drawn[label] = model.render(view(d, r.shift), LAW, oversample=2)
    img = d["pixels"].astype(float)
    use = img > 0
    for x in drawn.values():
        use &= numpy.isfinite(x) & (x > 0)
    use &= img > 0.1 * numpy.median(img[use])
    sigma = 0.5 * maplet.spacing * 1000 / d["pixel"]
    inner = use & (ndimage.gaussian_filter(use.astype(float), sigma) > 0.9)
    smooth = lambda a: ndimage.gaussian_filter(numpy.where(use, a, 0.0), sigma) / numpy.maximum(  # noqa: E731
        ndimage.gaussian_filter(use.astype(float), sigma), 1e-9)
    line = []
    for label, x in drawn.items():
        scores[label].append((numpy.corrcoef(x[use], img[use])[0, 1], numpy.corrcoef(smooth(x)[inner], smooth(img)[inner])[0, 1]))
        line.append(f"{label} {scores[label][-1][0]:.2f} ({scores[label][-1][1]:.2f})")
    print(f"held out {d['name']}, {d['pixel']:.2f} m/px: " + ", ".join(line), flush=True)
    figure.append((d, img, drawn, (img > 0) & numpy.isfinite(drawn["maplet"]) & numpy.isfinite(drawn["SHAP7"])))
for label, s in scores.items():
    s = numpy.array(s)
    print(f"{label}: median correlation with the held-out images {numpy.median(s[:, 0]):.2f}, at the model's scale {numpy.median(s[:, 1]):.2f}")

shown = [figure[k] for k in (2, 4, 10) if k < len(figure)]
fig, axs = plt.subplots(len(shown), 3, figsize=(12, 4.2 * len(shown)), squeeze=False)
for row, (d, img, drawn, seen) in zip(axs, shown):
    jj, ii = numpy.nonzero(seen)
    box = (slice(jj.min(), jj.max() + 1), slice(ii.min(), ii.max() + 1))
    for ax, (label, a) in zip(row, [("image", img)] + list(drawn.items())):
        lo, hi = numpy.percentile(a[box][seen[box]], [1, 99.5])
        ax.imshow(numpy.where(seen[box], a[box], numpy.nan), cmap="gray", vmin=lo, vmax=hi)
        ax.set_title(f"{d['name']} ({d['pixel']:.2f} m/px), held out" if label == "image" else label, fontsize=9)
        ax.axis("off")
plt.tight_layout()
plt.savefig(os.path.join(OUT, "held_out.png"), dpi=80)
