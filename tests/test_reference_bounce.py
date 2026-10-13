#!/usr/bin/env python
"""`reference.bounces`: sunlight bounced off the surfaces, against Ingersoll's
spherical bowl.

Inside a sphere, two points see each other's surfaces with
cos(a) cos(b) / r^2 = 1 / (4 R^2) wherever they are, so the light a Lambert
bowl scatters once lands everywhere in it alike -- lit or in its own shadow:
rho mu0 r_o^2 / (4 R^2) of the Sun's, mu0 the Sun's height over the opening,
r_o the opening's radius (Ingersoll, Svitek & Murray 1992). Each further
bounce is rho times the bowl's share of the sphere of the last.

A hemisphere of radius 1, white (rho 1), in a ring of ground, the Sun 30 deg
up (mu0 0.5), seen from above: in the bowl's shadow, one bounce is 1/8 of
the Sun's light and two are 3/16. The direct light alone puts the shadow
where the geometry says.

Then the bowl under a Hapke law, which each surface on the way reflects by:
one bounce at shadowed pixels against a float64 sum over the bowl's lit
part of IF_p IF_q dOmega / pi -- the law's I/F where the light lands, from
where it comes, times the I/F of the surface it comes from, lit by the Sun
and seen from there. Changing the law begins the sum again.

Skipped on a GPU without ray queries. Opens a window, in the background.
"""

import math
import os
import sys
import tempfile

import numpy

from kalast.app import App
from kalast.scattering import Hapke

failures: list[str] = []

ELEVATION = math.radians(30.0)
TO_SUN = numpy.array([math.cos(ELEVATION), 0.0, math.sin(ELEVATION)])
SIZE = 400
HEIGHT = 4.5  # the camera, straight above the bowl's centre
# Brighter than any asteroid, for the bounced light to show.
LAW = Hapke(w=0.6, b=0.3, c=0.4, b0=1.0, h=0.06, theta_bar=math.radians(20.0), k=1.0)


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def bowl() -> str:
    """The lower hemisphere of radius 1, facing in, and a ring of ground
    from 1 to 2.5 around its rim, facing up."""
    nt, nphi = 120, 240
    th = numpy.linspace(0.0, math.pi / 2, nt + 1)  # from the bottom up
    ph = numpy.linspace(0.0, 2 * math.pi, nphi + 1)[:-1]
    t, p = numpy.meshgrid(th, ph, indexing="ij")
    v = numpy.stack([numpy.sin(t) * numpy.cos(p), numpy.sin(t) * numpy.sin(p), -numpy.cos(t)], -1).reshape(-1, 3)
    faces = []
    for i in range(nt):
        for j in range(nphi):
            a, b = i * nphi + j, i * nphi + (j + 1) % nphi
            c, d = (i + 1) * nphi + j, (i + 1) * nphi + (j + 1) % nphi
            # Facing the centre: up and in.
            faces.append((a, d, b))
            faces.append((a, c, d))
    faces = numpy.array(faces)
    rim = len(v) - nphi
    rr = numpy.linspace(1.0, 2.5, 8)[1:]
    outer = numpy.concatenate([numpy.stack([r * numpy.cos(ph), r * numpy.sin(ph), numpy.zeros_like(ph)], -1) for r in rr])
    ring = []
    rows = [numpy.arange(rim, rim + nphi)] + [len(v) + k * nphi + numpy.arange(nphi) for k in range(len(rr))]
    for inner, out in zip(rows[:-1], rows[1:]):
        for j in range(nphi):
            a, b = inner[j], inner[(j + 1) % nphi]
            c, d = out[j], out[(j + 1) % nphi]
            ring.append((a, b, d))
            ring.append((a, d, c))
    v = numpy.concatenate([v, outer])
    f = numpy.concatenate([faces, numpy.array(ring)])
    return "".join("v %.9f %.9f %.9f\n" % tuple(q) for q in v) + "".join("f %d %d %d\n" % tuple(x + 1) for x in f)


def bowl_points() -> tuple[numpy.ndarray, numpy.ndarray]:
    """Each pixel's point on the bowl, from the camera, and whether there is
    one -- a ray from above meets the inside of the lower hemisphere at its
    far root."""
    tan = math.tan(math.radians(15.0))
    s = (numpy.arange(SIZE) + 0.5 - SIZE / 2) / (SIZE / 2) * tan
    x, y = numpy.meshgrid(s, -s)
    d = numpy.stack([x, y, -numpy.ones_like(x)], -1)
    d /= numpy.linalg.norm(d, axis=-1)[..., None]
    c = numpy.array([0.0, 0.0, HEIGHT])
    b = (d * c).sum(-1)
    disc = b * b - (c @ c - 1.0)
    t = -b + numpy.sqrt(numpy.clip(disc, 0, None))
    p = c + t[..., None] * d
    return p, (disc > 0) & (p[..., 2] < 0)


def law_if(mu0: float, mu: float, alpha: float) -> float:
    """`LAW`'s I/F, `pi r mu0`."""
    return math.pi * LAW.reflectance(mu0, mu, alpha) * mu0 if mu0 > 0.0 else 0.0


def one_bounce(p: numpy.ndarray, v: numpy.ndarray, n_t: int = 160, n_p: int = 320) -> float:
    """The I/F at `p` on the bowl, seen along `v`, of the sunlight `LAW`'s
    bowl bounces there once: the bowl's lit part summed in float64. Inside
    a sphere every point sees every other."""
    th = (numpy.arange(n_t) + 0.5) / n_t * (math.pi / 2)
    ph = (numpy.arange(n_p) + 0.5) / n_p * 2 * math.pi
    t, f = numpy.meshgrid(th, ph, indexing="ij")
    q = numpy.stack([numpy.sin(t) * numpy.cos(f), numpy.sin(t) * numpy.sin(f), -numpy.cos(t)], -1).reshape(-1, 3)
    area = (numpy.sin(t) * (math.pi / 2 / n_t) * (2 * math.pi / n_p)).reshape(-1)
    s_dot = q @ TO_SUN
    lit = (s_dot < 0) & ((q - 2 * s_dot[:, None] * TO_SUN)[:, 2] >= 0)
    total = 0.0
    for qi, a in zip(q[lit], area[lit]):
        d = qi - p
        r2 = d @ d
        w = d / math.sqrt(r2)
        out = -qi @ -w  # the bowl's normals face its centre
        if out <= 0.0 or -p @ w <= 0.0:
            continue
        here = law_if(-p @ w, -p @ v, math.acos(min(max(w @ v, -1.0), 1.0)))
        there = law_if(-qi @ TO_SUN, out, math.acos(min(max(-(TO_SUN @ w), -1.0), 1.0)))
        total += here * there * out * a / r2
    return total / math.pi


def run(app: App, bounces: int) -> numpy.ndarray | None:
    c = app.simulation.config
    c.reference.bounces = bounces
    # Two frames for the change to begin a new sum, and say so.
    app.step()
    app.step()
    for _ in range(20000):
        if app.simulation.reference_done:
            break
        app.step()
    return app.simulation.reference_image()


def main() -> int:
    path = os.path.join(tempfile.mkdtemp(prefix="kalast_bowl_"), "bowl.obj")
    with open(path, "w") as fh:
        fh.write(bowl())

    app = App()
    app.config.open_in_background = True
    c = app.simulation.config
    c.image.width = c.image.height = SIZE
    c.wireframe.mode = 0
    c.axes.style = "off"
    sim = app.simulation
    sim.load_mesh(path=path)
    sim.sun.pos = (1e6 * TO_SUN).tolist()
    sim.camera.pos, sim.camera.dir, sim.camera.up = [0.0, 0.0, HEIGHT], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]
    c.shadows.method = "rays"
    for _ in range(3):
        app.step()
    if not sim.rays:
        print("skip: this GPU traces no rays here (sim.rays is False)")
        app.close()
        return 0
    c.shadows.method = "reference"
    c.reference.max_samples = 512

    p, on_bowl = bowl_points()
    # Lit: facing the Sun, and the ray to it leaving through the opening --
    # its far root on the sphere above the rim.
    s_dot = (p * TO_SUN).sum(-1)
    exit_z = (p - 2 * s_dot[..., None] * TO_SUN)[..., 2]
    lit = on_bowl & (s_dot < 0) & (exit_z >= 0)
    deep = on_bowl & (exit_z < -0.1) & (numpy.linalg.norm(p[..., :2], axis=-1) < 0.9)

    direct = run(app, 0)
    if direct is None:
        check("the direct image is done", False)
        app.close()
        return 1
    seen = direct[..., 0] > 1e-3
    agree = (seen == lit)[on_bowl & (numpy.abs(exit_z) > 0.03)]
    check("the direct light alone: the shadow where the geometry puts it", agree.mean() > 0.999,
          f"{100 * agree.mean():.2f} % of {agree.size} pixels")
    check("the shadow wholly dark without bounces", float(direct[deep, 0].max()) == 0.0, f"brightest {direct[deep, 0].max():.4f}")

    for bounces, want in [(1, 0.125), (2, 0.1875)]:
        image = run(app, bounces)
        if image is None:
            check(f"{bounces} bounce(s): done", False)
            continue
        got = float(image[deep, 0].mean())
        halves = [float(image[deep & (p[..., 0] < 0), 0].mean()), float(image[deep & (p[..., 0] >= 0), 0].mean())]
        check(f"{bounces} bounce(s): the shadowed floor at Ingersoll's {want}", abs(got - want) <= 0.02 * want,
              f"{got:.5f} over {int(deep.sum())} pixels, {100 * (got / want - 1):+.2f} %")
        check(f"{bounces} bounce(s): alike across the shadow", abs(halves[0] - halves[1]) <= 0.02 * want,
              f"{halves[0]:.5f} and {halves[1]:.5f}")

    # The bowl under a law: its light is a hundredth of the white bowl's,
    # so the sum is taken on to its sample limit.
    app.simulation.bodies[0].scattering = LAW
    c.reference.error = 1e-6
    c.reference.max_samples = 1024
    image = run(app, 1)
    if image is None:
        check("Hapke, 1 bounce: done", False)
    else:
        # Each pixel's light as the mean of the 5 x 5 about it, all in the
        # shadow, where it changes smoothly: a fifth of a pixel's noise.
        inner = deep.copy()
        for dy in range(-2, 3):
            for dx in range(-2, 3):
                inner &= numpy.roll(deep, (dy, dx), axis=(0, 1))
        ys, xs = numpy.nonzero(inner)
        pick = numpy.random.default_rng(1).choice(len(ys), 24, replace=False)
        camera = numpy.array([0.0, 0.0, HEIGHT])
        got, want = [], []
        for y, x in zip(ys[pick], xs[pick]):
            v = (camera - p[y, x]) / numpy.linalg.norm(camera - p[y, x])
            got.append(float(image[y - 2:y + 3, x - 2:x + 3, 0].mean()))
            want.append(one_bounce(p[y, x], v))
        got, want = numpy.array(got), numpy.array(want)
        check("Hapke, 1 bounce: the shadowed floor at the bowl's sum", abs(got.mean() / want.mean() - 1) <= 0.02,
              f"{got.mean():.5f} against {want.mean():.5f} over {len(got)} pixels, {100 * (got.mean() / want.mean() - 1):+.2f} %")
    app.close()
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
