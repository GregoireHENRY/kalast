#!/usr/bin/env python
"""`kalast.shape`: a terrain's relief back from its images.

Rolling ground with boulders and a crater, 6.4 m square at 4 cm, drawn by
`Maplet.render` from eight cameras under eight Suns, 4.5 cm a pixel, each
camera pointed a pixel or two off. A maplet of the same 4 cm over the same
ground, started from the terrain smoothed by 30 cm, its plane kept -- a
shape model a few times coarser -- held to it above 10 m, where the
smoothing leaves it right, and solved both ways, the heights fitted to the
images directly (the default) and point by point:

- every view used, and its pointing error found back to a fifth of a
  pixel -- a tenth point by point;
- the heights' RMS against the truth a tenth of what it was, the relief
  under 30 cm the truth's;
- the images' residual a tenth of what it was.

Then `Maplet.from_surface` over a mesh of the truth: on it, its facets
found; and `mesh()` facing out. And `pointing` with `roll`: cameras about a
sphere of maplets, every other one turned about its boresight by up to two
degrees, as a planned attitude is, each shift what its turned axes give --
the rolls found back within 0.06 degrees, applied as `Rz(roll) @ axes`.

No window.
"""

import math
import sys

import numpy

from kalast.scattering import Hapke
from kalast.shape import Maplet, View, pointing
from kalast.util import RPD

failures: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    print(("ok   " if ok else "FAIL ") + name + (f"  ({detail})" if detail else ""))
    if not ok:
        failures.append(name)


def terrain(n: int, s: float) -> numpy.ndarray:
    c = (n - 1) / 2
    x = (numpy.arange(n) - c) * s
    X, Y = numpy.meshgrid(x, x)
    h = 0.15 * numpy.sin(1.3 * X) * numpy.cos(0.9 * Y) + 0.08 * numpy.sin(2.9 * X + 1.7 * Y)
    for bx, by, r, top in ((-1.2, 0.8, 0.35, 0.25), (1.5, -1.0, 0.25, 0.2), (0.4, 1.7, 0.3, 0.15), (-0.9, -1.6, 0.2, 0.18)):
        h += top * numpy.exp(-((X - bx) ** 2 + (Y - by) ** 2) / r**2)
    rho = numpy.hypot(X - 0.6, Y + 0.3)
    R = (0.8**2 + 0.15**2) / 0.3
    inside = rho < 0.8
    h[inside] += R - 0.15 - numpy.sqrt(R * R - rho[inside] ** 2)
    return h


def smooth(h: numpy.ndarray, sigma: float) -> numpy.ndarray:
    r = int(math.ceil(3 * sigma))
    k = numpy.exp(-0.5 * (numpy.arange(-r, r + 1) / sigma) ** 2)
    k /= k.sum()
    pad = numpy.pad(h, r, mode="edge")
    rows = numpy.apply_along_axis(lambda v: numpy.convolve(v, k, mode="valid"), 1, pad)
    return numpy.apply_along_axis(lambda v: numpy.convolve(v, k, mode="valid"), 0, rows)


def coarser(h: numpy.ndarray, sigma: float) -> numpy.ndarray:
    """`h` as a shape model a few times coarser has it: smoothed, its plane
    -- mean and tilt -- kept, which a coarse model gets right."""
    n = len(h)
    j, i = numpy.indices(h.shape)
    a = numpy.column_stack([numpy.ones(n * n), i.ravel(), j.ravel()])
    plane = (a @ numpy.linalg.lstsq(a, h.ravel(), rcond=None)[0]).reshape(h.shape)
    return plane + smooth(h - plane, sigma)


def direction(az: float, el: float) -> numpy.ndarray:
    az, el = az * RPD, el * RPD
    return numpy.array([math.cos(az) * math.cos(el), math.sin(az) * math.cos(el), math.sin(el)])


def main() -> int:
    law = Hapke(w=0.0385, b=0.276, c=1.0, b0=4.73, h=0.061, theta_bar=24.4 * RPD, k=1.2)
    nt, st = 161, 0.04
    truth = Maplet([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], st, terrain(nt, st))
    size, pixel, distance = 200, 0.045, 300.0
    rng = numpy.random.default_rng(0)
    cameras, errors = [], []
    for k in range(8):
        toward = direction(45.0 * k + 20.0, 90.0 - 15.0 * (k % 3))
        sun = direction(45.0 * ((3 * k) % 8), 55.0 - 25.0 * (k % 2))
        z = -toward
        x = numpy.cross(z, [0.0, 0.0, 1.0])
        x = x / numpy.linalg.norm(x) if numpy.linalg.norm(x) > 1e-6 else numpy.array([1.0, 0.0, 0.0])
        y = numpy.cross(z, x)
        axes = [x.tolist(), y.tolist(), z.tolist()]
        exact = View(numpy.zeros((size, size)), (toward * distance).tolist(), axes, distance / pixel, sun.tolist())
        cameras.append((exact, truth.render(exact, law), toward, axes, sun))
        errors.append(rng.uniform(-2.0, 2.0, 2))
    # Each camera pointed a pixel or two off, nothing of it shared as one
    # move of the ground: what is, the maplet follows, its place the views'
    # pointing on average.
    J = numpy.array([[c[0].project([[1.0, 0.0, 0.0]])[0, :2] - c[0].project([[0.0, 0.0, 0.0]])[0, :2],
                      c[0].project([[0.0, 1.0, 0.0]])[0, :2] - c[0].project([[0.0, 0.0, 0.0]])[0, :2]] for c in cameras])
    J = J.transpose(0, 2, 1)  # pixels per metre of ground: [view, (x, y), (ground x, ground y)]
    errors = numpy.array(errors)
    common, *_ = numpy.linalg.lstsq(J.reshape(-1, 2), errors.ravel(), rcond=None)
    errors = errors - J @ common
    views = []
    for k, ((exact, image, toward, axes, sun), err) in enumerate(zip(cameras, errors)):
        views.append(View(numpy.nan_to_num(image), (toward * distance).tolist(), axes, distance / pixel, sun.tolist(),
                          centre=(size / 2 + err[0], size / 2 + err[1]), valid=numpy.isfinite(image), name=f"v{k}"))

    n, s = nt, st
    want = truth.heights
    start = coarser(want, 0.30 / s)

    def rms(a: numpy.ndarray) -> float:
        d = a - want
        return float(numpy.sqrt(((d - d.mean()) ** 2).mean()))

    # Fitted directly, the images a little coarser than the grid, the views'
    # pointing settles a tenth of a pixel or so off and the heights at 0.2 cm.
    for direct, iterations, within, how in ((True, 16, 0.2, "directly"), (False, 12, 0.1, "point by point")):
        m = Maplet([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], s, start)
        report = m.solve(views, law, iterations=iterations, anchor=10.0, direct=direct)
        used = [v.used for v in report.views]
        check(f"{how}: every view used", all(used), f"{sum(used)} of {len(used)}")
        off = [math.hypot(v.shift[0] + e[0], v.shift[1] + e[1]) for v, e in zip(report.views, errors)]
        check(f"{how}: each view's pointing found back to {within} pixel", max(off) < within, f"off by {numpy.round(off, 2).tolist()}")
        before, after = rms(start), rms(m.heights)
        check(f"{how}: the heights a tenth as far off the truth", after < before / 10, f"RMS {100 * before:.2f} -> {100 * after:.2f} cm")
        detail = lambda a: a - coarser(a, 0.30 / s)  # noqa: E731
        r = numpy.corrcoef(detail(m.heights)[4:-4, 4:-4].ravel(), detail(want)[4:-4, 4:-4].ravel())[0, 1]
        check(f"{how}: the relief under 30 cm the truth's", r > 0.99, f"correlation {r:.4f}")
        check(f"{how}: the residual a tenth of what it was", report.residual[-1] < report.residual[0] / 10,
              f"{report.residual[0]:.4f} -> {report.residual[-1]:.4f}")

    # Over a mesh of the truth: on it, and its facets found.
    vertices, facets = truth.mesh()
    check("mesh(): two facets a cell", facets.shape == ((nt - 1) ** 2 * 2, 3) and vertices.shape == (nt * nt, 3))
    a, b, c = (vertices[facets[:, i]] for i in range(3))
    normals = numpy.cross(b - a, c - a)
    check("mesh(): facing out", bool((normals[:, 2] > 0).all()))
    over = Maplet.from_surface(vertices, facets, [0.0, 0.0, 0.0], 41, 0.1, normal=[0.0, 0.0, 1.0])
    expect = truth.resampled(41, 0.1).heights
    worst = float(numpy.abs(over.heights - expect).max())
    check("from_surface: on the surface", worst < 0.01, f"worst {100 * worst:.2f} cm")
    check("from_surface: its reference its heights", numpy.allclose(over.reference, over.heights))
    check("from_surface: the facets under it", 0 < len(over.facets) < len(facets), f"{len(over.facets)} of {len(facets)}")

    rolls()
    turns()
    sessions()
    print("FAILED: " + ", ".join(failures) if failures else "all ok")
    return 1 if failures else 0


def rolls() -> None:
    rng = numpy.random.default_rng(5)
    focal, far = 5000.0, 20.0
    centres = rng.normal(size=(200, 3))
    centres /= numpy.linalg.norm(centres, axis=1)[:, None]
    looks = rng.normal(size=(40, 3))
    looks /= numpy.linalg.norm(looks, axis=1)[:, None]
    axes = []
    for d in looks:
        z = -d
        x = numpy.cross([0.0, 0.0, 1.0], z)
        x /= numpy.linalg.norm(x)
        axes.append(numpy.array([x, numpy.cross(z, x), z]))
    axes = numpy.array(axes)
    freed = numpy.arange(len(looks)) % 2 == 0
    truth = numpy.where(freed, rng.uniform(-0.035, 0.035, len(looks)), 0.0)
    image, tile, shift = [], [], []
    for k, (d, a) in enumerate(zip(looks, axes)):
        c, s_ = numpy.cos(truth[k]), numpy.sin(truth[k])
        turned = numpy.array([[c, s_, 0.0], [-s_, c, 0.0], [0.0, 0.0, 1.0]]) @ a
        for t, p in enumerate(centres):
            if p @ d < 0.5:
                continue
            q, r = a @ (p - far * d), turned @ (p - far * d)
            image.append(k), tile.append(t)
            shift.append(focal * (r[:2] / r[2] - q[:2] / q[2]) + rng.uniform(-0.2, 0.2, 2))
    fit, behind, _, _ = pointing(far * looks, axes, focal, centres, centres, image, tile, shift, numpy.ones(len(image)),
                                 offset_sigma=0.01, roll=freed)
    off = numpy.degrees(numpy.abs(fit[:, 2] - truth))[behind >= 3]
    check("pointing(roll=...): each freed camera's roll found back within 0.06 deg", fit.shape == (len(looks), 3) and off.max() < 0.06,
          f"worst {off.max():.3f} deg")
    check("pointing(roll=...): no roll where not freed", bool((fit[~freed, 2] == 0.0).all()))


def turns() -> None:
    """A body turned about its origin by up to 3 degrees in half the images: pointing(turn=...) gives each turn back."""
    rng = numpy.random.default_rng(9)
    focal, far = 5000.0, 20.0
    centres = rng.normal(size=(300, 3))
    centres /= numpy.linalg.norm(centres, axis=1)[:, None]
    looks = rng.normal(size=(60, 3))
    looks /= numpy.linalg.norm(looks, axis=1)[:, None]
    axes = []
    for d in looks:
        z = -d
        x = numpy.cross([0.0, 0.0, 1.0], z)
        x /= numpy.linalg.norm(x)
        axes.append(numpy.array([x, numpy.cross(z, x), z]))
    axes = numpy.array(axes)
    freed = numpy.arange(len(looks)) % 2 == 0
    truth = numpy.where(freed[:, None], rng.uniform(-0.03, 0.03, (len(looks), 3)), 0.0)

    def rotation(v):
        a = numpy.linalg.norm(v)
        if a == 0:
            return numpy.eye(3)
        k = v / a
        K = numpy.array([[0, -k[2], k[1]], [k[2], 0, -k[0]], [-k[1], k[0], 0]])
        return numpy.eye(3) + numpy.sin(a) * K + (1 - numpy.cos(a)) * K @ K

    image, tile, shift = [], [], []
    for k, (d, a) in enumerate(zip(looks, axes)):
        R = rotation(truth[k])
        for t, p in enumerate(centres):
            if p @ d < 0.5:
                continue
            q, r = a @ (p - far * d), a @ (R @ p - far * d)
            image.append(k), tile.append(t)
            shift.append(focal * (r[:2] / r[2] - q[:2] / q[2]) + rng.uniform(-0.2, 0.2, 2))
    fit, behind, _, _ = pointing(far * looks, axes, focal, centres, centres, image, tile, shift, numpy.ones(len(image)),
                                 offset_sigma=0.01, iterations=40, turn=freed)
    err = numpy.array([a @ (fit[k, 2:5] - truth[k]) for k, a in enumerate(axes)])  # in each camera's frame
    held = behind >= 3
    across, along = numpy.degrees(numpy.hypot(err[:, 0], err[:, 1]))[held], numpy.degrees(numpy.abs(err[:, 2]))[held]
    check("pointing(turn=...): each freed image's turn found back, 0.3 deg across the line of sight, 0.1 along it",
          fit.shape == (len(looks), 5) and across.max() < 0.3 and along.max() < 0.1,
          f"worst {across.max():.3f} across, {along.max():.3f} along")
    check("pointing(turn=...): no turn where not freed", bool((fit[~freed, 2:5] == 0.0).all()))


def sessions() -> None:
    """Images that register few maplets, in sessions whose body turns smoothly: pointing(group=..., time=...) shares one
    turn and its rate a session, closer than each image's own."""
    rng = numpy.random.default_rng(13)
    focal, far = 5000.0, 20.0
    centres = rng.normal(size=(300, 3))
    centres /= numpy.linalg.norm(centres, axis=1)[:, None]

    def rotation(v):
        a = numpy.linalg.norm(v)
        if a == 0:
            return numpy.eye(3)
        k = v / a
        K = numpy.array([[0, -k[2], k[1]], [k[2], 0, -k[0]], [-k[1], k[0], 0]])
        return numpy.eye(3) + numpy.sin(a) * K + (1 - numpy.cos(a)) * K @ K

    looks, axes, group, time, truth = [], [], [], [], []
    for g in range(4):
        lon0, lat = rng.uniform(-numpy.pi, numpy.pi), rng.uniform(-0.3, 0.3)
        a_, b_ = rng.uniform(-0.01, 0.01, 3), rng.uniform(-0.01, 0.01, 3)
        for k in range(8):
            lon = lon0 + numpy.radians(60) * k / 7
            d = numpy.array([numpy.cos(lat) * numpy.cos(lon), numpy.cos(lat) * numpy.sin(lon), numpy.sin(lat)])
            z = -d
            x = numpy.cross([0.0, 0.0, 1.0], z)
            x /= numpy.linalg.norm(x)
            looks.append(d), axes.append(numpy.array([x, numpy.cross(z, x), z])), group.append(g), time.append(10.0 * g + 0.25 * k)
            truth.append(a_ + (0.25 * k - 0.875) * b_)
    looks, axes, truth = numpy.array(looks), numpy.array(axes), numpy.array(truth)
    image, tile, shift = [], [], []
    for k, (d, a) in enumerate(zip(looks, axes)):
        R = rotation(truth[k])
        seen = [t for t, p in enumerate(centres) if p @ d >= 0.5]
        for t in seen[:: max(1, len(seen) // 10)][:10]:
            p = centres[t]
            q, r = a @ (p - far * d), a @ (R @ p - far * d)
            image.append(k), tile.append(t)
            shift.append(focal * (r[:2] / r[2] - q[:2] / q[2]) + rng.uniform(-0.2, 0.2, 2))
    freed = numpy.ones(len(looks), bool)
    args = (far * looks, axes, focal, centres, centres, image, tile, shift, numpy.ones(len(image)))
    alone = pointing(*args, offset_sigma=0.01, iterations=40, turn=freed)[0]
    shared = pointing(*args, offset_sigma=0.01, iterations=40, turn=freed, group=group, time=time)[0]
    worst = lambda fit: numpy.degrees(numpy.linalg.norm(fit[:, 2:5] - truth, axis=1).max())  # noqa: E731
    check("pointing(group=..., time=...): one turn and rate a session, within 0.2 deg and closer than each image alone",
          shared.shape == (len(looks), 5) and worst(shared) < 0.2 and worst(shared) < 0.7 * worst(alone),
          f"worst {worst(shared):.3f} deg shared, {worst(alone):.3f} alone")


if __name__ == "__main__":
    sys.exit(main())
