#!/usr/bin/env python
"""With `shadows.rays`, the per-facet shadow query -- the thermophysical
model's shadows -- is traced with rays against the meshes themselves.

The scene of `tests/test_facet_shadow_disc.py`: a plate of 80,000 facets with
a wall standing on it, the Sun 25 deg up and square to the wall, its angular
radius 0.02, a penumbra about 0.9 wide. Each facet is held to what rays from
its corners and centre reach past the wall, worked out here in float64:

- with the Sun a point, exactly, point by point;
- with the Sun a disc, to a 401 x 401 grid over the limb-darkened disc,
  within what 256 rays a point resolve, and with no bias;
- the probe (`ray_probe`) moves no facet by more than a sliver of the
  disc's edge.

Skipped on a GPU without ray queries. Opens a window, in the background.
"""

import math
import os
import sys
import tempfile

import numpy

from kalast.app import App

failures: list[str] = []

SUN = 0.02
ELEVATION = math.radians(25.0)
TO_SUN = numpy.array([math.cos(ELEVATION), 0.0, math.sin(ELEVATION)])
HEIGHT, THICK, LENGTH = 4.0, 0.2, 30.0
EDGE = -(HEIGHT / math.tan(ELEVATION) + THICK / 2)
HALF, N = 20.0, 200
# The shaders': the Sun's limb darkening, linear in mu.
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


def seen(points: numpy.ndarray, grid: int) -> numpy.ndarray:
    """The share of the limb-darkened disc's light rays from each point reach
    past the wall; with `grid` 1, the Sun's centre alone."""
    s = numpy.linspace(-1, 1, grid) if grid > 1 else numpy.zeros(1)
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


def open_app(path: str) -> App:
    app = App()
    app.config.open_in_background = True
    c = app.simulation.config
    c.image.width = c.image.height = 200
    c.shadows.rays = True
    c.shadows.ray_samples = 256
    sim = app.simulation
    sim.load_mesh(path=path)
    c.light.sun_radius = SUN * 1e6
    sim.sun.pos = (1e6 * TO_SUN).tolist()
    sim.camera.pos, sim.camera.dir, sim.camera.up = [EDGE, 0.0, 50.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]
    return app


def query(app: App, point: bool, probe: int) -> numpy.ndarray | None:
    """One app for every query: a process has one window loop."""
    c = app.simulation.config
    c.light.sun_as_point = point
    c.shadows.ray_probe = probe
    hidden = None
    for _ in range(4):
        app.simulation.request_facet_shadow(0)
        app.step()
        got = app.simulation.facet_shadow(0)
        if got is not None:
            hidden = numpy.asarray(got, dtype=float).copy()
    return hidden


def main() -> int:
    text, v, f = scene()
    tmp = tempfile.mkdtemp(prefix="kalast_facet_shadow_rays_")
    path = os.path.join(tmp, "wall.obj")
    with open(path, "w") as fh:
        fh.write(text)

    app = open_app(path)
    point = query(app, True, 16)
    if not app.simulation.rays:
        print("skip: this GPU traces no rays here (sim.rays is False)")
        app.close()
        return 0
    check("the query answers", point is not None and len(point) == len(f), f"{None if point is None else len(point)} of {len(f)}")
    if point is None or len(point) != len(f):
        app.close()
        return 1

    # The plate's facets within 1.2 of the shadow's edge, away from the
    # wall's ends, and their corners and centre.
    corners = v[f[: 2 * N * N]]
    xs, ys = corners[..., 0], corners[..., 1]
    band = numpy.all(numpy.abs(xs - EDGE) < 1.2, axis=1) & numpy.all(numpy.abs(ys) < 8.0, axis=1)
    idx = numpy.nonzero(band)[0]
    points = numpy.concatenate([corners[idx], corners[idx].mean(axis=1, keepdims=True)], axis=1).reshape(-1, 3)

    want = 1.0 - seen(points, 1).reshape(len(idx), 4).mean(axis=1)
    err = numpy.abs(point[idx] - want)
    check("a point Sun: every facet as the rays have it, exactly", float(err.max()) == 0.0,
          f"{len(idx)} facets, {int((err > 0).sum())} off, worst {err.max():.3f}")

    disc = query(app, False, 16)
    disc_all = query(app, False, 0)
    app.close()
    want = 1.0 - seen(points, 401).reshape(len(idx), 4).mean(axis=1)
    err = disc[idx] - want
    partial = (want > 0.02) & (want < 0.98)
    rms = float(numpy.sqrt((err**2).mean()))
    check(
        "the Sun a disc: each facet hides what the disc's light says",
        partial.sum() > 100 and rms <= 0.003 and abs(float(err.mean())) <= 0.0005,
        f"{len(idx)} facets, {partial.sum()} in the penumbra: rms {100 * rms:.2f} %, mean {100 * err.mean():+.3f} %, "
        f"worst {100 * numpy.abs(err).max():.2f} %",
    )
    moved = numpy.abs(disc - disc_all)
    # Its rim rays miss only a sliver of the disc's edge: of eight on the
    # rim, one under about 0.6 % of the disc's light.
    check("the probe moves no facet by more than a sliver of the disc", float(moved.max()) <= 0.006,
          f"{int((moved > 0).sum())} facets moved, worst {100 * moved.max():.2f} %")
    plate = disc[: 2 * N * N]
    cx = corners.mean(axis=1)[:, 0]
    deep = (cx > EDGE + 1.5) & (cx < -0.5) & (numpy.abs(corners.mean(axis=1)[:, 1]) < 8.0)
    lit = (cx < EDGE - 1.5) & (numpy.abs(corners.mean(axis=1)[:, 1]) < 8.0)
    check("deep in the shadow, wholly hidden", float(plate[deep].min()) == 1.0, f"least hidden {plate[deep].min():.3f}")
    check("far from it, wholly lit", float(plate[lit].max()) == 0.0, f"most hidden {plate[lit].max():.3f}")

    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
