#!/usr/bin/env python
"""`light.sun_as_point = False` gives a shadow the penumbra the Sun's disc gives it.

A sphere of radius 1 hangs 100 above a plate, the Sun straight above it,
its angular radius 0.02 against the sphere's 0.01: no point of the plate
sees the whole Sun hidden, and under the sphere a quarter of the disc is,
more with the limb darkening -- Phobos's shadow on Mars, which is at most
22 % deep. The plate is read back along a line through the shadow's middle
(`shading.srgb_mode = 1`, so a pixel is the lit fraction times 200) and
held to the fraction of the limb-darkened disc the sphere leaves visible,
integrated here: 1.5 % rms, 4 % at worst, the shader sampling the disc at
128 points.

With the Sun a point, the same shadow is a black disc.

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
HEIGHT, RADIUS, SUN = 100.0, 1.0, 0.02
# The shader's: the Sun's limb darkening, linear in mu.
LIMB_DARKENING = 0.56


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def icosphere(level: int) -> str:
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
    lines = ["v %.9f %.9f %.9f" % (RADIUS * x, RADIUS * y, RADIUS * z + HEIGHT) for x, y, z in v]
    lines += ["f %d %d %d" % (a + 1, b + 1, c + 1) for a, b, c in f]
    return "\n".join(lines) + "\n"


def visible(x: float) -> float:
    """The limb-darkened Sun's visible fraction seen from (x, 0, 0)."""
    n = 801
    s = numpy.linspace(-1, 1, n)
    u, w = numpy.meshgrid(s, s)
    r2 = u * u + w * w
    disc = r2 <= 1
    weight = numpy.where(disc, 1 - LIMB_DARKENING * (1 - numpy.sqrt(numpy.clip(1 - r2, 0, 1))), 0)
    # The sphere's centre as seen from the point, and its angular radius.
    d = math.hypot(x, HEIGHT)
    centre = (-x / d) / SUN
    size = math.asin(RADIUS / d) / SUN
    hidden = (u - centre) ** 2 + w**2 <= size**2
    return float((weight * ~hidden).sum() / weight.sum())


def main() -> int:
    tmp = tempfile.mkdtemp(prefix="kalast_penumbra_")
    plate, ball = os.path.join(tmp, "plate.obj"), os.path.join(tmp, "ball.obj")
    with open(plate, "w") as f:
        f.write("v -20 -20 0\nv 20 -20 0\nv 20 20 0\nv -20 20 0\nf 1 2 3\nf 1 3 4\n")
    with open(ball, "w") as f:
        f.write(icosphere(4))
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
    c.light.exposure = LIT / 255.0
    sim = app.simulation
    sim.load_mesh(path=plate)
    sim.load_mesh(path=ball)
    sim.sun.pos = [0.0, 0.0, 1e6]
    cam = sim.camera
    half = 4.0
    cam.projection.fovy = 2 * math.atan(half / 50.0)
    cam.pos, cam.dir, cam.up = [0.0, 0.0, 50.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]

    def grab() -> numpy.ndarray:
        before = set(glob.glob(f"{out}/*.png"))
        sim.export_once()
        app.step()
        new = set(glob.glob(f"{out}/*.png")) - before
        assert len(new) == 1, new
        return read_png(new.pop())[..., 0].astype(float)

    for _ in range(3):
        app.step()
    hard = grab()
    check("a point Sun casts a black disc", hard[50, 50] <= 1, f"middle {hard[50, 50]:.0f}")

    c.light.sun_as_point = False
    c.light.sun_radius = SUN * 1e6
    app.step()
    soft = grab()
    row = soft[50, :]
    xs = (numpy.arange(101) - 50) * (2 * half / 101)
    want = numpy.array([LIT * visible(abs(x)) for x in xs])
    err = numpy.abs(row - want)
    if os.environ.get("PROFILE"):
        for x, g, w_ in zip(xs, row, want):
            print("  x %+.2f got %5.1f want %5.1f  %+5.1f" % (x, g, w_, g - w_))
    # 128 points of the disc: a fraction good to about one of them.
    rms = float(numpy.sqrt((err**2).mean()))
    check(
        "the penumbra is the disc's visible fraction",
        rms <= 0.015 * LIT and err.max() <= 0.04 * LIT,
        f"rms {100 * rms / LIT:.2f} %, worst {100 * err.max() / LIT:.1f} % at x {xs[err.argmax()]:+.2f}",
    )
    check(
        "the middle is a quarter of the disc and the limb's darkening dark",
        abs(row[50] - want[50]) <= 0.02 * LIT,
        f"{row[50]:.0f} for {want[50]:.0f}, {100 * (1 - want[50] / LIT):.1f} % hidden",
    )
    check("and the plate past the penumbra fully lit", abs(soft[50, 0] - LIT) <= 1, f"{soft[50, 0]:.0f}")

    app.close()
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
