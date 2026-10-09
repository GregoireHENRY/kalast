#!/usr/bin/env python
"""With the Sun a disc, a body's own shadow has its penumbra, and where it
overlaps another body's, the two hide what both hide.

One body is a plate with a wall standing on it, 4 high and 0.2 thick, the
Sun 25 deg above the plate and square to the wall, its angular radius 0.02:
the wall's shadow ends 8.6 away in a penumbra about 0.9 wide. A second body,
a ball of radius 0.08, floats 6 toward the Sun from beside that edge, so
its antumbra falls across the penumbra. The plate is read back along a line
through both (`shading.srgb_mode = 1`, a pixel the lit fraction times 200)
and held to the fraction of the limb-darkened disc that rays from the plate
reach, cast here against the wall and the ball themselves: first with the
ball off to the side, the wall's shadow alone, then with both.

The ball sits first 0.15 to the lit side of the line from the shadow's edge
to the Sun, so the Sun sees all of it and all of the wall's top; then on
that line, half behind the wall's top as the Sun sees it. A shadow map holds
what the Sun sees first and nothing behind it, so with the wall and the ball
in one, the ball's hidden half was not in it and still hid part of the disc
from the plate: past the shadow's edge, 2.7 % more light than the rays give.
With the Sun a disc, a body and the others cast into slices of their own.

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

failures: list[str] = []

LIT = 200.0
SUN = 0.02
ELEVATION = math.radians(25.0)
HEIGHT, THICK, LENGTH = 4.0, 0.2, 30.0
BALL = 0.08
# The shader's: the Sun's limb darkening, linear in mu.
LIMB_DARKENING = 0.56
TO_SUN = numpy.array([math.cos(ELEVATION), 0.0, math.sin(ELEVATION)])
EDGE = -(HEIGHT / math.tan(ELEVATION) + THICK / 2)


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def box(lo, hi) -> tuple[list, list]:
    (x0, y0, z0), (x1, y1, z1) = lo, hi
    v = [(x0, y0, z0), (x1, y0, z0), (x1, y1, z0), (x0, y1, z0), (x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1)]
    f = [(0, 2, 1), (0, 3, 2), (4, 5, 6), (4, 6, 7), (0, 1, 5), (0, 5, 4),
         (1, 2, 6), (1, 6, 5), (2, 3, 7), (2, 7, 6), (3, 0, 4), (3, 4, 7)]
    return v, f


def plate_and_wall() -> str:
    """The plate, 40 square, and the wall on it, as one mesh: one body."""
    v = [(-20, -20, 0), (20, -20, 0), (20, 20, 0), (-20, 20, 0)]
    f = [(0, 1, 2), (0, 2, 3)]
    wv, wf = box((-THICK / 2, -LENGTH / 2, 0.0), (THICK / 2, LENGTH / 2, HEIGHT))
    f += [(a + 4, b + 4, c + 4) for a, b, c in wf]
    v += wv
    return "".join("v %.9f %.9f %.9f\n" % p for p in v) + "".join("f %d %d %d\n" % (a + 1, b + 1, c + 1) for a, b, c in f)


def ball(level: int = 4) -> str:
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
    return "".join("v %.9f %.9f %.9f\n" % (BALL * x, BALL * y, BALL * z) for x, y, z in v) + "".join(
        "f %d %d %d\n" % (a + 1, b + 1, c + 1) for a, b, c in f
    )


def seen(points: numpy.ndarray, centre) -> numpy.ndarray:
    """The limb-darkened disc's fraction rays from each point reach past the
    wall and, at `centre`, the ball."""
    s = numpy.linspace(-1, 1, 161)
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
        if centre is not None:
            oc = p - centre
            bb = d @ oc
            disc = bb * bb - (oc @ oc - BALL * BALL)
            hit |= (disc >= 0) & (-bb + numpy.sqrt(numpy.clip(disc, 0, None)) > 0)
        out.append(float((weight * ~hit).sum() / weight.sum()))
    return numpy.array(out)


def main() -> int:
    tmp = tempfile.mkdtemp(prefix="kalast_penumbra_self_")
    scene, sphere = os.path.join(tmp, "wall.obj"), os.path.join(tmp, "ball.obj")
    with open(scene, "w") as f:
        f.write(plate_and_wall())
    with open(sphere, "w") as f:
        f.write(ball())
    out = os.path.join(tmp, "frames")

    app = App()
    app.config.open_in_background = True
    c = app.simulation.config
    c.export.dir, c.export.sync, c.export.hud = out, True, False
    c.image.width = c.image.height = 101
    c.axes.style = "off"
    c.wireframe.mode = 0
    c.shading.msaa = 1
    c.shading.srgb_mode = 1
    c.light.exposure = LIT / 255.0 / math.sin(ELEVATION)
    c.light.sun_as_point = False
    sim = app.simulation
    sim.load_mesh(path=scene)
    sim.load_mesh(path=sphere)
    c.light.sun_radius = SUN * 1e6
    sim.sun.pos = (1e6 * TO_SUN).tolist()
    cam = sim.camera
    half = 2.0
    cam.projection.fovy = 2 * math.atan(half / 50.0)
    cam.pos, cam.dir, cam.up = [EDGE, 0.0, 50.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]
    xs = EDGE + (numpy.arange(101) - 50) * (2 * half / 101)
    row_points = numpy.stack([xs, numpy.zeros_like(xs), numpy.zeros_like(xs)], axis=1)

    def grab() -> numpy.ndarray:
        before = set(glob.glob(f"{out}/*.png"))
        sim.export_once()
        app.step()
        new = set(glob.glob(f"{out}/*.png")) - before
        assert len(new) == 1, new
        return read_png(new.pop())[..., 0].astype(float)

    def place_ball(at) -> None:
        sim.bodies[1].mat[:3, 3] = at
        for _ in range(3):
            app.step()

    def compare(name: str, centre, worst: float | None = 0.05) -> tuple[numpy.ndarray, numpy.ndarray]:
        row = grab()[50, :]
        want = LIT * seen(row_points, centre)
        err = numpy.abs(row - want)
        if os.environ.get("PROFILE"):
            for x, g, w_ in zip(xs, row, want):
                print("  x %+.3f got %5.1f want %5.1f  %+5.1f" % (x, g, w_, g - w_))
        rms = float(numpy.sqrt((err**2).mean()))
        check(
            name,
            rms <= 0.015 * LIT and (worst is None or err.max() <= worst * LIT),
            f"rms {100 * rms / LIT:.2f} %, worst {100 * err.max() / LIT:.1f} % at x {xs[err.argmax()] - EDGE:+.2f} from the edge",
        )
        umbra = want <= 0.0
        if umbra.any():
            check(f"{name}: the umbra is black", row[umbra].max() <= 1.0, f"brightest {row[umbra].max():.0f}")
        return row, want

    # The ball off to the side, its shadow far from the row.
    place_ball([EDGE + 6 * TO_SUN[0], 10.0, 6 * TO_SUN[2]])
    compare("the wall's own shadow has the disc's penumbra", None)

    # The ball 6 toward the Sun from the wall's shadow edge, 0.15 aside,
    # across the Sun's rays to the lit side.
    aside = numpy.array([-math.sin(ELEVATION), 0.0, math.cos(ELEVATION)])
    centre = numpy.array([EDGE, 0.0, 0.0]) + 6.0 * TO_SUN + 0.15 * aside
    place_ball(centre.tolist())
    compare("where the ball's shadow crosses it, the two hide what both hide", centre)

    # On that line, half behind the wall's top as the Sun sees it. Past the
    # shadow's edge, where that half hides what the wall does not, the rays'
    # light on average. The worst pixel is the hard edge's, as much with the
    # ball beside the line: there the lookup's normal offset moves both
    # shadows by part of a pixel, and the two add.
    centre = numpy.array([EDGE, 0.0, 0.0]) + 6.0 * TO_SUN
    place_ball(centre.tolist())
    row, want = compare("with the ball half behind the wall's top, the same", centre, worst=None)
    both = (xs > EDGE + 0.02) & (want > 0.0) & (want < LIT * seen(row_points, None) - 0.01 * LIT)
    mean = float((row - want)[both].mean())
    check(
        "past the shadow's edge, the ball's hidden half hides its part",
        abs(mean) <= 0.015 * LIT,
        f"mean {100 * mean / LIT:+.2f} % over {both.sum()} px",
    )

    app.close()
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
