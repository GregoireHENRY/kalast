#!/usr/bin/env python
"""A body with an atmosphere shadows another by its ellipsoid and its air.

A planet of radius 1000 carries an atmosphere, its dust 0.45 deep at the
surface with a scale height of 11; the Sun is far along +x. A plate 3000
behind the planet faces the Sun, set so the ray from its middle toward the
Sun passes the planet `z` above the surface. Its middle pixel is read back
(`shading.srgb_mode = 1`, a pixel the lit fraction times 200) and held, the
Sun a point, to `exp(-tau exp(-z / H) sqrt(2 pi (R + z) / H))`, the
transmission of a grazing ray through an exponential atmosphere, and to 0
under the limb -- the planet casts into no other body's shadow layer now,
so that is the ellipsoid's shadow; the Sun a disc 30 high where it passes,
to the mean over the limb-darkened disc of the same; and with the
atmosphere taken off, to the planet's shadow map again.

Opens a window, in the background.
"""

import glob
import math
import os
import sys
import tempfile

import numpy
from _png import read_png

from kalast.app import App
from kalast.scattering import Atmosphere

failures: list[str] = []

LIT = 200.0
RADIUS, TAU, H = 1000.0, 0.45, 11.0
BEHIND = 3000.0
HEIGHTS = [-20.0, 5.0, 15.0, 30.0, 50.0, 80.0]
# The Sun's angular radius in the disc's case: 30 across where it passes.
SUN = 0.01
LIMB_DARKENING = 0.56


def through(z: float) -> float:
    if z <= 0:
        return 0.0
    return math.exp(-TAU * math.exp(-z / H) * math.sqrt(2 * math.pi * (RADIUS + z) / H))


def disc_mean(z: float) -> float:
    """The limb-darkened disc's mean transmission from the plate's middle."""
    s = numpy.linspace(-1, 1, 201)
    a, b = numpy.meshgrid(s, s)
    inside = a * a + b * b <= 1
    a, b = a[inside], b[inside]
    w = 1 - LIMB_DARKENING * (1 - numpy.sqrt(numpy.clip(1 - a * a - b * b, 0, 1)))
    d = numpy.stack([numpy.ones_like(a), SUN * b, SUN * a], axis=1)
    d /= numpy.linalg.norm(d, axis=1)[:, None]
    p = numpy.array([-BEHIND, RADIUS + z, 0.0])
    q = p[None] - (d @ p)[:, None] * d
    t = numpy.array([through(r - RADIUS) for r in numpy.linalg.norm(q, axis=1)])
    return float((w * t).sum() / w.sum())


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def icosphere(level: int, radius: float) -> str:
    g = (1 + 5**0.5) / 2
    v = [(-1, g, 0), (1, g, 0), (-1, -g, 0), (1, -g, 0), (0, -1, g), (0, 1, g),
         (0, -1, -g), (0, 1, -g), (g, 0, -1), (g, 0, 1), (-g, 0, -1), (-g, 0, 1)]
    v = [tuple(numpy.array(p) / numpy.linalg.norm(p)) for p in v]
    f = [(0, 11, 5), (0, 5, 1), (0, 1, 7), (0, 7, 10), (0, 10, 11), (1, 5, 9), (5, 11, 4), (11, 10, 2),
         (10, 7, 6), (7, 1, 8), (3, 9, 4), (3, 4, 2), (3, 2, 6), (3, 6, 8), (3, 8, 9), (4, 9, 5),
         (2, 4, 11), (6, 2, 10), (8, 6, 7), (9, 8, 1)]
    for _ in range(level):
        mid, nf = {}, []

        def m(a, b):
            k = (min(a, b), max(a, b))
            if k not in mid:
                p = (numpy.array(v[a]) + numpy.array(v[b])) / 2
                v.append(tuple(p / numpy.linalg.norm(p)))
                mid[k] = len(v) - 1
            return mid[k]

        for a, b, c in f:
            ab, bc, ca = m(a, b), m(b, c), m(c, a)
            nf += [(a, ab, ca), (ab, b, bc), (ca, bc, c), (ab, bc, ca)]
        f = nf
    return "".join("v %.9f %.9f %.9f\n" % (radius * x, radius * y, radius * z) for x, y, z in v) + "".join(
        "f %d %d %d\n" % (a + 1, b + 1, c + 1) for a, b, c in f
    )


def main() -> int:
    tmp = tempfile.mkdtemp(prefix="kalast_air_shadow_")
    planet, plate = os.path.join(tmp, "planet.obj"), os.path.join(tmp, "plate.obj")
    with open(planet, "w") as f:
        f.write(icosphere(5, RADIUS))
    # A plate 2 square in y and z, facing +x, the Sun's way.
    with open(plate, "w") as f:
        f.write("v 0 -1 -1\nv 0 1 -1\nv 0 1 1\nv 0 -1 1\nf 1 2 3\nf 1 3 4\n")
    out = os.path.join(tmp, "frames")

    app = App()
    app.config.open_in_background = True
    c = app.simulation.config
    c.export.dir, c.export.sync, c.export.hud = out, True, False
    c.image.width = c.image.height = 41
    c.axes.style = "off"
    c.wireframe.mode = 0
    c.shading.msaa = 1
    c.shading.srgb_mode = 1
    c.light.exposure = LIT / 255.0
    sim = app.simulation
    sim.load_mesh(path=planet)
    sim.load_mesh(path=plate)
    sim.sun.pos = [1e9, 0.0, 0.0]
    planet_body, plate_body = sim.bodies
    planet_body.atmosphere = Atmosphere(tau=TAU, scale_height=H, radius=RADIUS, polar_radius=None)
    cam = sim.camera
    cam.projection.fovy = math.radians(10.0)

    def grab() -> float:
        before = set(glob.glob(f"{out}/*.png"))
        sim.export_once()
        app.step()
        new = set(glob.glob(f"{out}/*.png")) - before
        assert len(new) == 1, new
        return float(read_png(new.pop())[20, 20, 0])

    def place(z: float) -> None:
        centre = numpy.array([-BEHIND, RADIUS + z, 0.0])
        plate_body.mat[:3, 3] = centre
        cam.pos = (centre + numpy.array([10.0, 0.0, 0.0])).tolist()
        cam.dir, cam.up = [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]
        for _ in range(3):
            app.step()

    worst = 0.0
    for z in HEIGHTS:
        place(z)
        got = grab() / LIT
        want = through(z)
        err = abs(got - want)
        worst = max(worst, err)
        print("     %4.0f above the limb: %.3f, the formula %.3f" % (z, got, want))
    check("the plate behind the limb gets what the planet and its dust let through", worst <= 0.01, f"worst {100 * worst:.1f} % of the light")

    c.light.sun_as_point = False
    c.light.sun_radius = SUN * 1e9
    worst = 0.0
    for z in (-10.0, 10.0, 30.0, 50.0):
        place(z)
        got = grab() / LIT
        want = disc_mean(z)
        worst = max(worst, abs(got - want))
        print("     %4.0f above the limb, the Sun a disc: %.3f, the disc's mean %.3f" % (z, got, want))
    check("and the Sun a disc, the disc's mean", worst <= 0.02, f"worst {100 * worst:.1f} % of the light")
    c.light.sun_as_point = True

    planet_body.atmosphere = None
    place(5.0)
    got = grab()
    check("with no atmosphere, over the limb it is fully lit", abs(got - LIT) <= 1, f"{got:.0f}")
    place(-20.0)
    got = grab()
    check("and under it, in the planet's shadow map's shadow", got <= 1, f"{got:.0f}")

    app.close()
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
