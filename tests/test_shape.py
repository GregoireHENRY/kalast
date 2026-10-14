#!/usr/bin/env python
"""`kalast.shape`: a terrain's relief back from its images.

Rolling ground with boulders and a crater, 6.4 m square at 4 cm, drawn by
`Maplet.render` from eight cameras under eight Suns, 4.5 cm a pixel, each
camera pointed a pixel or two off. A maplet of the same 4 cm over the same
ground, started from the terrain smoothed by 30 cm, its plane kept -- a
shape model a few times coarser -- held to it above 10 m, where the
smoothing leaves it right, and solved:

- every view used, and its pointing error found back to a tenth of a
  pixel;
- the heights' RMS against the truth a tenth of what it was, the relief
  under 30 cm the truth's;
- the images' residual a tenth of what it was.

Then `Maplet.from_surface` over a mesh of the truth: on it, its facets
found; and `mesh()` facing out.

No window.
"""

import math
import sys

import numpy

from kalast.scattering import Hapke
from kalast.shape import Maplet, View
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
    m = Maplet([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], s, start)

    def rms(a: numpy.ndarray) -> float:
        d = a - want
        return float(numpy.sqrt(((d - d.mean()) ** 2).mean()))

    report = m.solve(views, law, iterations=12, anchor=10.0)
    used = [v.used for v in report.views]
    check("every view used", all(used), f"{sum(used)} of {len(used)}")
    off = [math.hypot(v.shift[0] + e[0], v.shift[1] + e[1]) for v, e in zip(report.views, errors)]
    check("each view's pointing found back to a tenth of a pixel", max(off) < 0.1, f"off by {numpy.round(off, 2).tolist()}")
    before, after = rms(start), rms(m.heights)
    check("the heights a tenth as far off the truth", after < before / 10, f"RMS {100 * before:.2f} -> {100 * after:.2f} cm")
    detail = lambda a: a - coarser(a, 0.30 / s)  # noqa: E731
    r = numpy.corrcoef(detail(m.heights)[4:-4, 4:-4].ravel(), detail(want)[4:-4, 4:-4].ravel())[0, 1]
    check("the relief under 30 cm the truth's", r > 0.99, f"correlation {r:.4f}")
    check("the residual a tenth of what it was", report.residual[-1] < report.residual[0] / 10,
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

    print("FAILED: " + ", ".join(failures) if failures else "all ok")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
