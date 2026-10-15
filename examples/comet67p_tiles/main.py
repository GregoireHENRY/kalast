#!/usr/bin/env python
"""Comet 67P whole, finer than its shape model where its images allow:
Preusker et al.'s SHAP7 under 6,489 tiles of 241 x 241 points, each at the
finest spacing -- 0.5, 1 or 2 m -- that five of Rosetta's OSIRIS NAC images
resolve, solved by kalast.shape from up to 70 of the images that see it
(`tiles.csv`, `images.csv`). Eight days of images are left out of every
tile, to score it on.

    python main.py fetch [A B]   # the lines of each image tiles A to B need
    python main.py solve [A B]   # tiles A to B, several at once
    python main.py score [A B]   # each solved tile on the held-out images that see it

Tiles A, A+1, ..., B-1: all of them by default. Each step keeps what it made
and starts again where it stopped. The whole comet: 30 GB of image lines,
and a day of solving on an M1 Pro.
"""

import collections
import concurrent.futures
import csv
import os
import re
import sys
import threading
import time

import numpy
import spiceypy as spice
from scipy import ndimage

from kalast.scattering import Hapke
from kalast.shape import Maplet, Surface, View
from kalast.util import RPD

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "comet67p_shadows_fit"))
import osiris  # noqa: E402

# Where these are kept here: set them to yours.
SHAP7 = "/Users/gregoireh/data/mesh/67p/shap7/cg-dlr_spg-shap7-v1.0.ply"
SHAP7_4M = "/Users/gregoireh/data/mesh/67p/shap7/cg-dlr_spg-shap7-v1.0_4Mfacets.ply"
KERNELS = os.path.join(HERE, "..", "comet67p_shadows_fit", "rosetta.tm")
LINES = os.path.join(osiris.DATA, "tiles67p")  # the images' lines
OUT = "out/67p_tiles"
WORKERS = 5  # tiles solved at once: each takes one to two cores

# `hapke-fit` of comet67p_shadows_fit: Hapke's law fitted to 57 NAC images.
LAW = Hapke(w=0.0356, b=0.276, c=1.0, b0=4.73, h=0.061, theta_bar=24.4 * RPD, k=1.2)
IFOV = 18.82e-6  # the NAC's pixel, rad
SIZE = 241

tiles = list(csv.DictReader(open(os.path.join(HERE, "tiles.csv"))))
images = list(csv.DictReader(open(os.path.join(HERE, "images.csv"))))
step = sys.argv[1] if len(sys.argv) > 1 else ""
first, last = (int(sys.argv[2]), int(sys.argv[3])) if len(sys.argv) > 3 else (0, len(tiles))
todo = range(first, last)


def ids(text):
    return [int(k) for k in text.split()]


def frame(w):
    """`u`, `v` across the normal `w`, as kalast.shape lays a maplet."""
    axis = numpy.array([0.0, 0.0, 1.0]) if abs(w[2]) < 0.9 else numpy.array([1.0, 0.0, 0.0])
    u = numpy.cross(axis, w)
    u /= numpy.linalg.norm(u)
    return u, numpy.cross(w, u)


def box(tile):
    """The corners of a tile's square, a quarter of its width above and below."""
    c = numpy.array([float(tile[k]) for k in "xyz"])
    w = numpy.array([float(tile[k]) for k in ("nx", "ny", "nz")])
    half = 0.5 * (SIZE - 1) * float(tile["spacing"])
    u, v = frame(w)
    return numpy.array([c + a * half * u + b * half * v + e * 0.25 * half * w for a in (-1, 1) for b in (-1, 1) for e in (-1, 1)])


def camera(image):
    """An image's camera and Sun in the comet's frame at mid-exposure, from
    its label and SPICE; and where its boresight falls in the stored lines,
    their first counted from the frame's."""
    head = osiris.label(image["dataset"], image["product"])
    info = osiris._image_object(head)
    lines, samples = osiris._key(info, "LINES", int), osiris._key(info, "LINE_SAMPLES", int)
    et = spice.str2et(osiris._key(head, "START_TIME")) + osiris._key(head, "EXPOSURE_DURATION", float) / 2
    to_nac = numpy.array(spice.pxform("J2000", "ROS_OSIRIS_NAC", et))
    comet, lt = spice.spkpos("CHURYUMOV-GERASIMENKO", et, "J2000", "LT+S", "ROSETTA")
    sun, _ = spice.spkpos("SUN", et, "J2000", "LT+S", "ROSETTA")
    body = numpy.array(spice.pxform("67P/C-G_CK", "J2000", et - lt))
    toward = body.T @ (numpy.array(sun) - numpy.array(comet))
    # A window of the CCD has its centre off the boresight; lines along the
    # NAC's +X.
    x0, x1 = osiris._key(head, "ROSETTA:X_START", int), osiris._key(head, "ROSETTA:X_END", int)
    y0, y1 = osiris._key(head, "ROSETTA:Y_START", int), osiris._key(head, "ROSETTA:Y_END", int)
    sample = 0.0 if samples == 2048 else 1024 - (x0 + x1) / 2
    line = 0.0 if lines == 2048 else (y0 + y1) / 2 - 1024
    return dict(position=-body.T @ numpy.array(comet), axes=(to_nac @ body)[[1, 0, 2]], sun=toward / numpy.linalg.norm(toward),
                centre=(0.5 * samples - sample, 0.5 * lines - line), lines=lines)


def cameras(which):
    """Every camera `which` (image indices) needs: the labels fetched eight
    at a time -- each a request to the archive, half a second -- then SPICE,
    one thread's."""
    which = sorted(which)
    with concurrent.futures.ThreadPoolExecutor(8) as pool:
        list(pool.map(lambda k: osiris.label(images[k]["dataset"], images[k]["product"]), which))
    spice.furnsh(KERNELS)
    out = {k: camera(images[k]) for k in which}
    spice.kclear()
    return out


def lines_of(cam, points, reach):
    """The lines of an image `points` fall on, `reach` around."""
    q = (points - cam["position"]) @ cam["axes"].T
    y = cam["centre"][1] + q[:, 1] / q[:, 2] / IFOV
    margin = reach / (numpy.median(q[:, 2]) * IFOV) + 8
    return int(max(0, numpy.floor(y.min() - margin))), int(min(cam["lines"], numpy.ceil(y.max() + margin) + 1))


def ply(path):
    """A binary PLY of triangles: its vertices and facets."""
    raw = open(path, "rb").read()
    end = raw.index(b"end_header\n") + len(b"end_header\n")
    nv, nf = (int(re.search(rb"element " + e + rb" (\d+)", raw[:end])[1]) for e in (b"vertex", b"face"))
    vertices = numpy.frombuffer(raw, "<f4", 3 * nv, end).reshape(-1, 3)
    facets = numpy.frombuffer(raw, [("n", "u1"), ("i", "<i4", 3)], nf, end + 12 * nv)["i"]
    return vertices.astype(numpy.float64), numpy.ascontiguousarray(facets, dtype=numpy.uint32)


def cut(k, cam, points, reach):
    """Image `k` cut to where `points` fall, `reach` around: its pixels and
    where its boresight falls in them; `None` if they fall off it."""
    stored = numpy.load(os.path.join(LINES, images[k]["product"][:-4] + ".npz"))
    top, pixels = int(stored["first"]), stored["image"]
    q = (points - cam["position"]) @ cam["axes"].T
    x = cam["centre"][0] + q[:, 0] / q[:, 2] / IFOV
    y = cam["centre"][1] + q[:, 1] / q[:, 2] / IFOV - top
    margin = reach / (numpy.median(q[:, 2]) * IFOV) + 8
    r0, r1 = int(max(0, y.min() - margin)), int(min(pixels.shape[0], y.max() + margin + 1))
    c0, c1 = int(max(0, x.min() - margin)), int(min(pixels.shape[1], x.max() + margin + 1))
    if r1 - r0 < 16 or c1 - c0 < 16:
        return None
    return pixels[r0:r1, c0:c1].astype(numpy.float32), (cam["centre"][0] - c0, cam["centre"][1] - top - r0)


def view(k, cam, pixels, centre, valid=True, shift=(0.0, 0.0)):
    """The cut image as kalast.shape takes it, its pointing moved by `shift`."""
    return View(pixels, cam["position"], cam["axes"], 1 / IFOV, cam["sun"], centre=(centre[0] + shift[0], centre[1] + shift[1]),
                valid=(pixels > 0) & valid, name=images[k]["product"][:16])


def parallel(jobs, work):
    """`work(job)` on WORKERS threads -- kalast.shape lets go of Python while
    it works -- each line it returns printed."""
    jobs, lock, start, done = collections.deque(jobs), threading.Lock(), time.time(), [0]
    total = len(jobs)

    def run():
        while True:
            with lock:
                if not jobs:
                    return
                job = jobs.popleft()
            try:
                line = work(job)
            except Exception as e:  # noqa: BLE001
                line = f"{job}: failed, {e!r}"
            with lock:
                done[0] += 1
                left = (time.time() - start) / done[0] * (total - done[0]) / 3600
                print(f"{line} | {done[0]}/{total}, {left:.1f} h left", flush=True)

    threads = [threading.Thread(target=run) for _ in range(WORKERS)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()


if step == "fetch":
    # Per image, the lines all its tiles fall on: one range each, fetched
    # once.
    users = collections.defaultdict(list)
    for t in todo:
        for k in ids(tiles[t]["images"]) + ids(tiles[t]["held"]):
            users[k].append(t)
    cams = cameras(users)
    need = {k: lines_of(cams[k], numpy.concatenate([box(tiles[t]) for t in ts]), max(24 * float(tiles[t]["spacing"]) for t in ts))
            for k, ts in users.items()}
    print(f"{len(need)} images, {sum(b - a for a, b in need.values()) * 2048 * 4 / 1e9:.0f} GB of lines at most", flush=True)
    parallel(sorted(need), lambda k: os.path.basename(osiris.fetch_lines(images[k]["dataset"], images[k]["product"], *need[k], LINES)))

if step not in ("fetch", "solve", "score"):
    sys.exit(__doc__)

if step in ("solve", "score"):
    surface = ply(SHAP7)
    context = Surface(*ply(SHAP7_4M))
    os.makedirs(os.path.join(OUT, "tiles"), exist_ok=True)

if step == "solve":
    todo = [t for t in todo if not os.path.exists(os.path.join(OUT, "tiles", f"t{t:05d}.npz"))]
    cams = cameras({k for t in todo for k in ids(tiles[t]["images"])})

    def solve(t):
        tile = tiles[t]
        s = float(tile["spacing"])
        c = [float(tile[k]) for k in "xyz"]
        w = [float(tile[k]) for k in ("nx", "ny", "nz")]
        maplet = Maplet.from_surface(*surface, c, SIZE, s, normal=w)
        points = maplet.points().reshape(-1, 3)
        # Each image's pointing searched 24 spacings on the ground; what the
        # rest of the comet hides of the tile, or shadows, left out of it.
        reach = 24 * s
        views = []
        for k in ids(tile["images"]):
            if (found := cut(k, cams[k], points, reach)) is not None:
                hidden = context.mask(maplet, view(k, cams[k], *found), margin=0.002 + 2 * s)
                views.append(view(k, cams[k], *found, valid=hidden))
        report = maplet.solve(views, LAW, iterations=12, levels=2, offset=True, reach=reach, albedo_sigma=0.05, curvature=0.02)
        r = report.views
        numpy.savez(os.path.join(OUT, "tiles", f"t{t:05d}.npz"), heights=maplet.heights.astype(numpy.float32),
                    albedo=maplet.albedo.astype(numpy.float32), reference=maplet.reference.astype(numpy.float32),
                    centre=maplet.centre, axes=maplet.axes, spacing=s, views=[v.name for v in r], used=[v.used for v in r],
                    shift=[v.shift for v in r], correlation=[v.correlation for v in r], residual=report.residual)
        fit = numpy.median([v.correlation for v in r if v.used] or [[numpy.nan] * 2], axis=0)
        return f"t{t:05d} {s * 1000:.1f} m: {len(views)} images, correlation {fit[0]:.2f} -> {fit[1]:.2f} at the last level"

    parallel(todo, solve)

if step == "score":
    # Each solved tile in up to three held-out images that see it, the
    # finest: SHAP7 and the solution each registered on the image on its
    # own, drawn in it, and correlated with it where it shows the tile lit
    # and nothing else hides or shadows it -- at its pixels, and smoothed to
    # half the tile's spacing, the model's own scale.
    scores = os.path.join(OUT, "scores.csv")
    done = {(r["tile"], r["image"]) for r in csv.DictReader(open(scores))} if os.path.exists(scores) else set()
    solved = [t for t in todo if os.path.exists(os.path.join(OUT, "tiles", f"t{t:05d}.npz"))]
    cams = cameras({k for t in solved for k in ids(tiles[t]["held"])})
    jobs = []
    for t in solved:
        held = sorted(ids(tiles[t]["held"]), key=lambda k: numpy.linalg.norm(cams[k]["position"] - box(tiles[t]).mean(0)))
        jobs += [(t, k) for k in held[:3] if (str(t), images[k]["product"][:16]) not in done]
    out = open(scores, "a", newline="")
    fields = ["tile", "image", "spacing", "pixel", "shap7", "solved", "shap7_model", "solved_model"]
    rows = csv.DictWriter(out, fields)
    if not done:
        rows.writeheader()

    def score(job):
        t, k = job
        d = numpy.load(os.path.join(OUT, "tiles", f"t{t:05d}.npz"))
        s, w = float(d["spacing"]), d["axes"][2]
        heights, albedo, reference = d["heights"].astype(float), d["albedo"].astype(float), d["reference"].astype(float)
        models = {"shap7": Maplet(d["centre"], w, s, numpy.where(numpy.isfinite(reference), reference, heights)),
                  "solved": Maplet(d["centre"], w, s, heights, albedo)}
        points = models["solved"].points().reshape(-1, 3)
        found = cut(k, cams[k], points, 24 * s)
        if found is None:
            return f"t{t:05d} {images[k]['product'][:16]}: off the image"
        img, centre = found
        use = img > 0
        drawn = {}
        for label, m in models.items():
            hidden = context.mask(m, view(k, cams[k], img, centre), margin=0.002 + 2 * s)
            probe = Maplet(m.centre, w, s, m.heights, m.albedo)
            shift = probe.solve([view(k, cams[k], img, centre, hidden)], LAW, iterations=0, offset=True, reach=24 * s, resolution=8.0).views[0].shift
            drawn[label] = m.render(view(k, cams[k], img, centre, shift=shift), LAW, oversample=2)
            use &= numpy.isfinite(drawn[label]) & (drawn[label] > 0) & hidden
        if use.sum() < 200:
            return f"t{t:05d} {images[k]['product'][:16]}: too little of it seen"
        use &= img > 0.1 * numpy.median(img[use])
        q = (points - cams[k]["position"]) @ cams[k]["axes"].T
        pixel = numpy.median(q[:, 2]) * IFOV
        sigma = 0.5 * s / pixel
        inner = use & (ndimage.gaussian_filter(use.astype(float), sigma) > 0.9)

        def smooth(a):
            return ndimage.gaussian_filter(numpy.where(use, a, 0.0), sigma) / numpy.maximum(ndimage.gaussian_filter(use.astype(float), sigma), 1e-9)

        row = dict(tile=t, image=images[k]["product"][:16], spacing=s, pixel=round(pixel * 1000, 3))
        for label, a in drawn.items():
            row[label] = round(numpy.corrcoef(a[use], img[use])[0, 1], 4)
            row[label + "_model"] = round(numpy.corrcoef(smooth(a)[inner], smooth(img)[inner])[0, 1], 4) if inner.sum() > 50 else ""
        rows.writerow(row)
        out.flush()
        return f"t{t:05d} {row['image']}: SHAP7 {row['shap7']:.2f}, solved {row['solved']:.2f}"

    parallel(jobs, score)
    out.close()
    every = list(csv.DictReader(open(scores)))
    for label in ("shap7", "solved"):
        values = numpy.array([float(r[label]) for r in every])
        model = numpy.array([float(r[label + "_model"]) for r in every if r[label + "_model"]])
        print(f"{label}: mean correlation with the held-out images {values.mean():.3f}, at the model's scale {model.mean():.3f}"
              f" ({len(values)} tile-image pairs)")
