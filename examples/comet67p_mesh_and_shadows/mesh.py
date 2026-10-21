#!/usr/bin/env python
"""Comet 67P/Churyumov-Gerasimenko from Rosetta's OSIRIS NAC images alone,
as a spacecraft arriving at a body knows it: its images and its kernels, no
shape model to start from. The images' silhouettes carved into a hull
(`kalast.shape.Hull`), then maplets over it, coarse to fine -- each image
registered on each, every image's pointing and every maplet's place and
height fitted at once (`pointing`), the relief from the shading
(`Maplet.solve`) -- each round merged into the mesh made again (`merge`):
40 m, 20 m, then 10 m. SHAP7 only measures the result.

    python mesh.py fetch      # the 354 frames of frames.csv, 38 MB each
    python mesh.py hull       # the hull; the frames that show a limb registered on it
    python mesh.py tiles      # the rounds, each kept, from where they stopped
    python mesh.py compare    # against SHAP7

Into out/67p_from_images/: hull.ply, then 40m_1.ply ... 10m_3.ply, each
with its frames' pointing (.npz).
"""

import collections
import csv
import os
import re
import sys
import threading
import time

import numpy
import spiceypy as spice
from scipy import ndimage
from scipy.spatial import cKDTree

import osiris
from kalast.scattering import Hapke
from kalast.shape import Hull, Maplet, Surface, View, align, choose, merge, pointing
from kalast.util import RPD

HERE = os.path.dirname(os.path.abspath(__file__))
# Where these are kept here: set them to yours.
KERNELS = os.path.join(HERE, "rosetta.tm")
SHAP7 = "/Users/gregoireh/data/mesh/67p/shap7/cg-dlr_spg-shap7-v1.0.ply"  # for `compare` only
OUT = "out/67p_from_images"

IFOV = 18.82e-6  # the NAC's pixel, rad
RADIUS = 3.0  # km: a sphere the nucleus is inside
LIT = 0.05  # lit: over this share of a frame's lit ground (its 99.5th percentile)
# Fornasier et al. (2015)'s Hapke fit of the NAC's images: published, fitted
# on no shape model here.
LAW = Hapke(w=0.034, b=0.42, c=1.0, b0=2.25, h=0.061, theta_bar=28.0 * RPD, k=1.2)
# Spacing (km), rounds, the merged mesh's cubes (km); maplets of 31 x 31.
STAGES = [(0.040, 10, 0.020), (0.020, 4, 0.010), (0.010, 3, 0.010)]
SIZE = 31
WORKERS = 6  # maplets solved at once: each takes one to two cores

frames = list(csv.DictReader(open(os.path.join(HERE, "frames.csv"))))
step = sys.argv[1] if len(sys.argv) > 1 else ""


def read_ply(path):
    raw = open(path, "rb").read()
    end = raw.index(b"end_header\n") + len(b"end_header\n")
    nv, nf = (int(re.search(rb"element " + e + rb" (\d+)", raw[:end])[1]) for e in (b"vertex", b"face"))
    v = numpy.frombuffer(raw, "<f4", 3 * nv, end).reshape(-1, 3).astype(float)
    f = numpy.frombuffer(raw, [("n", "u1"), ("i", "<i4", 3)], nf, end + 12 * nv)["i"]
    return v, numpy.ascontiguousarray(f, dtype=numpy.uint32)


def save_ply(path, v, f):
    with open(path + ".part", "wb") as o:
        o.write(f"ply\nformat binary_little_endian 1.0\ncomment comet 67P from OSIRIS images by kalast.shape, km, 67P/C-G_CK\n"
                f"element vertex {len(v)}\nproperty float x\nproperty float y\nproperty float z\n"
                f"element face {len(f)}\nproperty list uchar int vertex_indices\nend_header\n".encode())
        o.write(numpy.asarray(v, "<f4").tobytes())
        rec = numpy.zeros(len(f), [("n", "u1"), ("i", "<i4", 3)])
        rec["n"], rec["i"] = 3, f
        o.write(rec.tobytes())
    os.replace(path + ".part", path)


def pixels_of(row):
    return numpy.nan_to_num(osiris.read(os.path.join(osiris.DATA, row["product"]))[0]).astype(numpy.float32)


def camera(row):
    """A frame's camera and Sun in the comet's frame at mid-exposure."""
    label = osiris.read(os.path.join(osiris.DATA, row["product"]))[1]
    et = spice.str2et(label["start"]) + label["exposure"] / 2
    to_nac = numpy.array(spice.pxform("J2000", "ROS_OSIRIS_NAC", et))
    comet, lt = spice.spkpos("CHURYUMOV-GERASIMENKO", et, "J2000", "LT+S", "ROSETTA")
    sun, _ = spice.spkpos("SUN", et, "J2000", "LT+S", "ROSETTA")
    body = numpy.array(spice.pxform("67P/C-G_CK", "J2000", et - lt))
    toward = body.T @ (numpy.array(sun) - numpy.array(comet))
    return dict(position=-body.T @ numpy.array(comet), axes=(to_nac @ body)[[1, 0, 2]], sun=toward / numpy.linalg.norm(toward))


def view(pixels, cam, shift, scale=1, name=""):
    """A whole frame as kalast.shape takes it, binned `scale` x `scale`, its
    pointing moved by `shift` (pixels of the frame)."""
    if scale > 1:
        n = pixels.shape[0] // scale
        pixels = pixels[: n * scale, : n * scale].reshape(n, scale, n, scale).mean(axis=(1, 3))
    return View(pixels, cam["position"], cam["axes"], 1 / IFOV / scale, cam["sun"],
                centre=((1024.0 + shift[0]) / scale, (1024.0 + shift[1]) / scale), name=name)


def parallel(jobs, work, label):
    """`work(job)` on WORKERS threads -- kalast.shape lets go of Python while
    it works."""
    jobs, lock, done, start = collections.deque(jobs), threading.Lock(), [0], time.time()
    total = len(jobs)

    def run():
        while True:
            with lock:
                if not jobs:
                    return
                job = jobs.popleft()
            try:
                work(job)
            except Exception as e:  # noqa: BLE001
                print(f"{label} {job}: failed, {e!r}", flush=True)
            with lock:
                done[0] += 1
                if done[0] % 100 == 0 or done[0] == total:
                    print(f"{label}: {done[0]}/{total} ({time.time() - start:.0f} s)", flush=True)

    threads = [threading.Thread(target=run) for _ in range(WORKERS)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()


if step == "fetch":
    for row in frames:
        osiris.fetch(row["dataset"], row["product"])
    sys.exit()
if step not in ("hull", "tiles", "compare"):
    sys.exit(__doc__)
os.makedirs(OUT, exist_ok=True)
spice.furnsh(KERNELS)

if step == "hull":
    # The frames for the hull, those that show the nucleus lit: a frame of
    # stray light or empty sky would take all of space for sky.
    used = [r for r in frames if r["hull"] == "1" and numpy.percentile(pixels_of(r), 99.5) > 1e-3]
    cams = {r["product"]: camera(r) for r in used}
    shifts = {r["product"]: (0.0, 0.0) for r in used}

    def carve():
        hull, sky = Hull(RADIUS), {}
        for k in range(0, len(used), 16):
            batch = used[k : k + 16]
            views = [view(pixels_of(r), cams[r["product"]], shifts[r["product"]], name=r["product"][:16]) for r in batch]
            for r, c in zip(batch, hull.add_many(views, threshold=LIT)):
                sky[r["product"]] = (c == -1).reshape(1024, 2, 1024, 2).all(axis=(1, 3))
        return hull, sky

    held = set()
    for round_ in range(4):
        hull, sky = carve()
        v, f, report = hull.mesh(0.020)
        print(f"hull {round_}: {len(f):,} facets, {report['volume']:.2f} km3, genus {report['genus']}", flush=True)
        if round_ == 3:
            break
        # Each frame that shows a limb registered on the hull drawn in its
        # geometry: its lit pixels next to the sky on the hull, that sky
        # off it. The others are the maplets' to register.
        surface = Surface(v, f)
        for r in used:
            pixels = pixels_of(r)
            half = view(pixels, cams[r["product"]], shifts[r["product"]], scale=2)
            binned = pixels[:2048, :2048].reshape(1024, 2, 1024, 2).mean(axis=(1, 3))
            lit = binned > LIT * numpy.percentile(binned, 99.5)
            s = sky[r["product"]]
            lit_band, sky_band = lit & ndimage.binary_dilation(s, iterations=3), s & ndimage.binary_dilation(lit, iterations=3)
            if lit_band.sum() < 300 or sky_band.sum() < 300:
                continue
            signs = numpy.where(lit_band, 1, numpy.where(sky_band, -1, 0)).astype(numpy.int8)
            (dx, dy), score = align(signs, surface.draw(half) > 0, search=16)
            if score > 0.25:
                shifts[r["product"]] = (shifts[r["product"]][0] + 2 * dx, shifts[r["product"]][1] + 2 * dy)
                held.add(r["product"])
        print(f"registered {len(held)} frames by their limbs", flush=True)
    # Smoothed by 30 m: the tunnels a few pixels' errors cut closed, one piece.
    v, f, report = hull.mesh(0.020, smooth=0.030)
    print(f"the hull: {len(f):,} facets, {report['volume']:.2f} km3, genus {report['genus']}", flush=True)
    save_ply(os.path.join(OUT, "hull.ply"), v, f)
    names = [r["product"] for r in frames]
    numpy.savez(os.path.join(OUT, "hull.npz"), names=names, pointing=[shifts.get(n, (0.0, 0.0)) for n in names],
                held=[n in held for n in names])

if step == "tiles":
    names = [r["product"] for r in frames]
    cams = {r["product"]: camera(r) for r in frames}
    saved = numpy.load(os.path.join(OUT, "hull.npz"))
    held = {str(n) for n, h in zip(saved["names"], saved["held"]) if h}

    def one_round(mesh, points_from, out, layout, spacing, cube):
        """A round of maplets over `mesh`: laid as on `layout`, each frame
        registered on each, all the frames' pointing and the maplets' places
        fitted at once, solved again with the pointing held, merged."""
        v, f = read_ply(mesh)
        surface = Surface(v, f)
        given = numpy.load(points_from)
        point0 = {str(n): tuple(p) for n, p in zip(given["names"], given["pointing"])}
        lv, lf = read_ply(layout)
        samples = Surface(lv, lf).samples(spacing)
        tiles = Surface.tiles(*samples, numpy.full(len(samples[0]), spacing), size=SIZE, overlap=0.3)
        normals = tiles[:, 3:6].copy()
        # Each maplet's middle on the mesh as it is now: down its normal, the
        # vertex nearest that line.
        tree = cKDTree(v)
        centres = numpy.empty((len(tiles), 3))
        for k, t in enumerate(tiles):
            d, i = tree.query(t[:3] + numpy.linspace(-0.8, 0.2, 101)[:, None] * t[3:6])
            centres[k] = v[i[numpy.argmin(d)]]
        seen = collections.defaultdict(list)
        for k, n in enumerate(names):
            c, s = cams[n], point0[n]
            flags, scale, inc, emi = surface.sees(centres, normals, c["position"], c["axes"], 1 / IFOV, (2048, 2048), c["sun"],
                                                  centre=(1024.0 + s[0], 1024.0 + s[1]))
            for t in numpy.flatnonzero(((flags & 15) == 15) & (inc < 75 * RPD) & (emi < 70 * RPD) & (scale < spacing)):
                seen[t].append((k, scale[t]))
        chosen = {}
        for t, cand in seen.items():
            if len(cand) >= 5:
                ks = [k for k, _ in cand]
                taken = choose(numpy.array([s for _, s in cand]), numpy.array([cams[names[k]]["sun"] for k in ks]),
                               numpy.array([cams[names[k]]["position"] - centres[t] for k in ks]), cap=40)
                chosen[t] = [ks[i] for i in taken]
        cache, lock = collections.OrderedDict(), threading.Lock()

        def frame(k):
            with lock:
                if k in cache:
                    return cache[k]
            p = pixels_of(frames[k])
            with lock:
                cache[k] = p
                while len(cache) > 80:
                    cache.popitem(last=False)
            return p

        def views_for(m, t, pointing_of, reach):
            pts = m.points().reshape(-1, 3)
            out = []
            for k in chosen[t]:
                c, s = cams[names[k]], pointing_of(k)
                cx, cy = 1024.0 + s[0], 1024.0 + s[1]
                q = (pts - c["position"]) @ numpy.array(c["axes"]).T
                x, y = cx + q[:, 0] / q[:, 2] / IFOV, cy + q[:, 1] / q[:, 2] / IFOV
                margin = reach / (numpy.median(q[:, 2]) * IFOV) + 8
                c0, c1 = int(max(0, x.min() - margin)), int(min(2048, x.max() + margin + 1))
                r0, r1 = int(max(0, y.min() - margin)), int(min(2048, y.max() + margin + 1))
                if c1 - c0 < 16 or r1 - r0 < 16:
                    continue
                cut = frame(k)[r0:r1, c0:c1]
                bare = View(cut, c["position"], c["axes"], 1 / IFOV, c["sun"], centre=(cx - c0, cy - r0), name=names[k][:16])
                valid = (cut > 0) & surface.mask(m, bare, margin=2 * spacing)
                out.append(View(cut, c["position"], c["axes"], 1 / IFOV, c["sun"], centre=(cx - c0, cy - r0), valid=valid, name=names[k][:16]))
            return out

        index = {n[:16]: k for k, n in enumerate(names)}
        first = {}

        def solve_first(t):
            m = Maplet.from_surface(v, f, centres[t], SIZE, spacing, normal=normals[t])
            views = views_for(m, t, lambda k: point0[names[k]], 3 * spacing)
            if len(views) >= 5:
                r = m.solve(views, LAW, iterations=8, levels=1, reach=3 * spacing, albedo_sigma=0.05, curvature=0.02)
                first[t] = (m, [(index[x.name], x.shift, x.correlation[1]) for x in r.views if x.used and numpy.isfinite(x.shift).all()])

        parallel(sorted(chosen), solve_first, "first pass")
        order = sorted(first)
        column = {t: i for i, t in enumerate(order)}
        reg = numpy.array([(k, column[t], s[0], s[1], max(float(c), 0.05)) for t in order for k, s, c in first[t][1]])
        ks = sorted(set(reg[:, 0].astype(int).tolist()))
        kcol = {k: i for i, k in enumerate(ks)}
        # The frames registered by their limbs keep their pointing: they hold
        # the model in SPICE's frame.
        fit, behind, offsets, residual = pointing(
            numpy.array([cams[names[k]]["position"] for k in ks]), numpy.array([cams[names[k]]["axes"] for k in ks]), 1 / IFOV,
            numpy.array([first[t][0].centre for t in order]), numpy.array([first[t][0].axes[2] for t in order]),
            [kcol[int(k)] for k in reg[:, 0]], reg[:, 1].astype(int), reg[:, 2:4], reg[:, 4],
            offset_sigma=0.05, height_sigma=0.3, fixed=[names[k] in held for k in ks])
        now = {k: (point0[names[k]][0] + fit[kcol[k]][0], point0[names[k]][1] + fit[kcol[k]][1]) for k in ks if behind[kcol[k]] >= 3}
        print(f"pointing: {len(ks)} frames, {len(reg)} registrations, residual {numpy.median(residual):.1f} px at the median", flush=True)
        second = {}

        def solve_second(t):
            m0, o = first[t][0], offsets[column[t]]
            u, w, n = m0.axes
            m = Maplet.from_surface(v, f, m0.centre + o[0] * u + o[1] * w, SIZE, spacing, normal=n)
            known = numpy.isfinite(m.reference)
            m.heights = m.heights + o[2]
            m.reference = numpy.where(known, m.heights, numpy.nan)
            views = views_for(m, t, lambda k: now.get(k, point0[names[k]]), 0.0)
            if len(views) >= 5:
                m.solve(views, LAW, iterations=8, levels=1, reach=0.0, search=3.0, follow=False, albedo_sigma=0.05, curvature=0.02)
                second[t] = m

        parallel(order, solve_second, "second pass")
        nv, nf, report = merge(v, f, [second[t] for t in sorted(second)], size=cube, smooth=cube, relax=2, depth=10 * spacing)
        save_ply(out + ".ply", nv, nf)
        numpy.savez(out + ".npz", names=names, pointing=[now.get(k, point0[n]) for k, n in enumerate(names)])
        print(f"{os.path.basename(out)}: {len(second)} maplets, {len(nf):,} facets, {report['volume']:.2f} km3, genus {report['genus']}", flush=True)

    mesh, points_from = os.path.join(OUT, "hull.ply"), os.path.join(OUT, "hull.npz")
    for spacing, rounds, cube in STAGES:
        # A stage's maplets laid once, on the mesh it starts from: they stay
        # where they are through its rounds, as SPC's landmarks.
        layout = mesh
        for r in range(1, rounds + 1):
            out = os.path.join(OUT, f"{round(spacing * 1000)}m_{r}")
            if not os.path.exists(out + ".ply"):
                one_round(mesh, points_from, out, layout, spacing, cube)
            mesh, points_from = out + ".ply", out + ".npz"

if step == "compare":
    # The last mesh made against SHAP7: how far each of its points is from
    # the mesh, signed by the mesh's normal -- as the kernels place both,
    # then SHAP7 laid on the mesh at its best (iterated closest points).
    last = sorted((f for f in os.listdir(OUT) if f.endswith(".ply") and f[0].isdigit()), key=lambda f: (-int(f.split("m_")[0]), int(f.split("_")[1][:-4])))[-1]
    v, f = read_ply(os.path.join(OUT, last))
    raw = open(SHAP7, "rb").read(400)
    nv = int(re.search(rb"element vertex (\d+)", raw)[1])
    s7 = numpy.fromfile(SHAP7, "<f4", 3 * nv, offset=raw.index(b"end_header\n") + 11).reshape(-1, 3)[::50].astype(float)
    normal = numpy.zeros_like(v)
    fi = f.astype(numpy.int64)
    for k in range(3):
        numpy.add.at(normal, fi[:, k], numpy.cross(v[fi[:, 1]] - v[fi[:, 0]], v[fi[:, 2]] - v[fi[:, 0]]))
    normal /= numpy.linalg.norm(normal, axis=1, keepdims=True)
    tree = cKDTree(v)

    def against(points, label):
        d, i = tree.query(points, workers=8)
        signed = numpy.where(((v[i] - points) * normal[i]).sum(1) >= 0, d, -d) * 1000
        a = numpy.abs(signed)
        print(f"{last}, {label}: SHAP7 against it, m: median {numpy.median(a):.1f}, rms {numpy.sqrt((signed ** 2).mean()):.1f};"
              f" within 10 m {(a < 10).mean():.0%}, 20 m {(a < 20).mean():.0%}, 50 m {(a < 50).mean():.1%}")
        return i

    against(s7, "as the kernels place it")
    p = s7.copy()
    for _ in range(30):
        d, i = tree.query(p, workers=8)
        keep = d < numpy.percentile(d, 90)
        a, b = p[keep], v[i[keep]]
        u, _, vt = numpy.linalg.svd((a - a.mean(0)).T @ (b - b.mean(0)))
        r = vt.T @ numpy.diag([1, 1, numpy.sign(numpy.linalg.det(vt.T @ u.T))]) @ u.T
        p = (p - a.mean(0)) @ r.T + b.mean(0)
    move = (p - s7).mean(0) * 1000
    print(f"SHAP7 laid on it moves {numpy.round(move, 1).tolist()} m")
    against(p, "SHAP7 laid on it")
    vol = numpy.einsum("ij,ij->i", v[fi[:, 0]], numpy.cross(v[fi[:, 1]], v[fi[:, 2]])).sum() / 6
    print(f"volume {vol:.2f} km3, SHAP7's 18.56")

spice.kclear()
