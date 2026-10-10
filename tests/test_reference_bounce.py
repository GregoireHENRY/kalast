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

Skipped on a GPU without ray queries. Opens a window, in the background.
"""

import math
import os
import sys
import tempfile

import numpy

from kalast.app import App

failures: list[str] = []

ELEVATION = math.radians(30.0)
TO_SUN = numpy.array([math.cos(ELEVATION), 0.0, math.sin(ELEVATION)])
SIZE = 400
HEIGHT = 4.5  # the camera, straight above the bowl's centre


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
    c.shadows.rays = True
    for _ in range(3):
        app.step()
    if not sim.rays:
        print("skip: this GPU traces no rays here (sim.rays is False)")
        app.close()
        return 0
    c.shadows.rays = False
    c.reference.enabled = True
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
    app.close()
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
