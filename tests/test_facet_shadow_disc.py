#!/usr/bin/env python
"""With the Sun a disc, the per-facet shadow query -- the thermophysical
model's shadows -- has the disc's penumbrae, as the image does.

A plate of 80,000 facets, 0.2 apart, with a wall standing on it, 4 high and
0.2 thick, one body; the Sun 25 deg above the plate and square to the wall,
its angular radius 0.02: the wall's shadow ends 8.68 from it in a penumbra
about 0.9 wide, some five facets across. `sim.facet_shadow` is held, facet by
facet across that edge, to the fraction of the limb-darkened disc that rays
from the facet's corners and centre do not reach past the wall -- the points
the query itself averages. With a point Sun the same query stays binary per
point, quarter steps per facet (`tests/test_facet_shadow.py`).

Opens a window, in the background.
"""

import math
import sys

import numpy

from kalast.app import App
import os
import tempfile

failures: list[str] = []

SUN = 0.02
ELEVATION = math.radians(25.0)
TO_SUN = numpy.array([math.cos(ELEVATION), 0.0, math.sin(ELEVATION)])
HEIGHT, THICK, LENGTH = 4.0, 0.2, 30.0
EDGE = -(HEIGHT / math.tan(ELEVATION) + THICK / 2)
HALF, N = 20.0, 200
# The shader's: the Sun's limb darkening, linear in mu.
LIMB_DARKENING = 0.56


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def scene() -> tuple[str, numpy.ndarray, numpy.ndarray]:
    """The plate's N x N cells of two facets, then the wall's box."""
    s = numpy.linspace(-HALF, HALF, N + 1)
    x, y = numpy.meshgrid(s, s)
    v = numpy.stack([x.ravel(), y.ravel(), numpy.zeros(x.size)], 1)
    i = numpy.arange(N)
    a = (i[:, None] * (N + 1) + i[None, :]).ravel()
    b, c, d = a + 1, a + N + 1, a + N + 2
    f = numpy.concatenate([numpy.stack([a, b, d], 1), numpy.stack([a, d, c], 1)])
    x0, y0, x1, y1 = -THICK / 2, -LENGTH / 2, THICK / 2, LENGTH / 2
    wv = numpy.array([(x0, y0, 0), (x1, y0, 0), (x1, y1, 0), (x0, y1, 0),
                      (x0, y0, HEIGHT), (x1, y0, HEIGHT), (x1, y1, HEIGHT), (x0, y1, HEIGHT)], float)
    wf = numpy.array([(0, 2, 1), (0, 3, 2), (4, 5, 6), (4, 6, 7), (0, 1, 5), (0, 5, 4),
                      (1, 2, 6), (1, 6, 5), (2, 3, 7), (2, 7, 6), (3, 0, 4), (3, 4, 7)]) + len(v)
    v = numpy.concatenate([v, wv])
    f = numpy.concatenate([f, wf])
    text = "".join("v %.9f %.9f %.9f\n" % tuple(p) for p in v) + "".join("f %d %d %d\n" % tuple(t + 1) for t in f)
    return text, v, f


def seen(points: numpy.ndarray) -> numpy.ndarray:
    """The limb-darkened disc's fraction rays from each point reach past the wall."""
    s = numpy.linspace(-1, 1, 81)
    a, b = numpy.meshgrid(s, s)
    inside = a * a + b * b <= 1
    a, b = a[inside], b[inside]
    weight = 1 - LIMB_DARKENING * (1 - numpy.sqrt(numpy.clip(1 - a * a - b * b, 0, 1)))
    e1 = numpy.array([0.0, 1.0, 0.0])
    e2 = numpy.cross(TO_SUN, e1)
    d = TO_SUN[None, :] + SUN * (a[:, None] * e1 + b[:, None] * e2)
    d /= numpy.linalg.norm(d, axis=1)[:, None]
    lo = numpy.array([-THICK / 2, -LENGTH / 2, 0.0])
    hi = numpy.array([THICK / 2, LENGTH / 2, HEIGHT])
    out = []
    for p in points:
        with numpy.errstate(divide="ignore", invalid="ignore"):
            t0, t1 = (lo - p) / d, (hi - p) / d
        near = numpy.nanmax(numpy.minimum(t0, t1), axis=1)
        far = numpy.nanmin(numpy.maximum(t0, t1), axis=1)
        hit = (near <= far) & (far > 1e-9)
        out.append(float((weight * ~hit).sum() / weight.sum()))
    return numpy.array(out)


def main() -> int:
    text, v, f = scene()
    tmp = tempfile.mkdtemp(prefix="kalast_facet_shadow_disc_")
    path = os.path.join(tmp, "wall.obj")
    with open(path, "w") as fh:
        fh.write(text)

    app = App()
    app.config.open_in_background = True
    c = app.simulation.config
    c.image.width = c.image.height = 200
    c.light.sun_as_point = False
    sim = app.simulation
    sim.load_mesh(path=path)
    c.light.sun_radius = SUN * 1e6
    sim.sun.pos = (1e6 * TO_SUN).tolist()
    sim.camera.pos, sim.camera.dir, sim.camera.up = [EDGE, 0.0, 50.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]

    hidden = None
    for _ in range(8):
        sim.request_facet_shadow(0)
        app.step()
        got = sim.facet_shadow(0)
        if got is not None:
            hidden = numpy.asarray(got, dtype=float).copy()
    check("the query answers", hidden is not None and len(hidden) == len(f), f"{None if hidden is None else len(hidden)} of {len(f)} facets")
    if hidden is None or len(hidden) != len(f):
        app.close()
        return 1

    # The plate's facets whose corners all lie within 1.2 of the shadow's edge,
    # away from the wall's ends.
    corners = v[f[: 2 * N * N]]
    xs, ys = corners[..., 0], corners[..., 1]
    band = numpy.all(numpy.abs(xs - EDGE) < 1.2, axis=1) & numpy.all(numpy.abs(ys) < 8.0, axis=1)
    idx = numpy.nonzero(band)[0]
    points = numpy.concatenate([corners[idx], corners[idx].mean(axis=1, keepdims=True)], axis=1)
    want = 1.0 - seen(points.reshape(-1, 3)).reshape(len(idx), 4).mean(axis=1)
    got = hidden[idx]
    err = got - want
    partial = (want > 0.02) & (want < 0.98)
    rms = float(numpy.sqrt((err**2).mean()))
    check(
        "across the wall's shadow edge, each facet hides what rays say",
        partial.sum() > 100 and rms <= 0.03 and abs(float(err.mean())) <= 0.01,
        f"{len(idx)} facets, {partial.sum()} in the penumbra: rms {100 * rms:.2f} %, mean {100 * err.mean():+.2f} %, "
        f"worst {100 * numpy.abs(err).max():.1f} %",
    )
    levels = numpy.unique(numpy.round(got[partial] * 4, 6))
    check(
        "the penumbra is graded, not quarter steps",
        len(levels) > 20,
        f"{len(levels)} distinct values over the penumbra's facets",
    )
    plate = hidden[: 2 * N * N]
    cx = corners.mean(axis=1)[:, 0]
    deep = (cx > EDGE + 1.5) & (cx < -0.5) & (numpy.abs(corners.mean(axis=1)[:, 1]) < 8.0)
    lit = (cx < EDGE - 1.5) & (numpy.abs(corners.mean(axis=1)[:, 1]) < 8.0)
    check("deep in the shadow, wholly hidden", float(plate[deep].min()) >= 0.99, f"least hidden {plate[deep].min():.3f}")
    check("far from it, wholly lit", float(plate[lit].max()) <= 0.01, f"most hidden {plate[lit].max():.3f}")

    app.close()
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
