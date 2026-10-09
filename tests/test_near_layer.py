#!/usr/bin/env python
"""A body seen up close gets a near shadow layer, finer where the camera looks
closest (`shadows.near_layer`), and from far it gets none.

A plate 200 wide of 524,288 facets, enough for level-of-detail patches a
few units across, and on it a wall 1 high and 0.2 thick, a body of its own:
part of the plate's mesh, simplified with it far from the camera, it came
out a tent over the plate with a shadow tens of units wide. The Sun
is a point 25 deg above the plate and square to the wall, so the wall's
shadow ends on a straight line, 2.245 from the wall's middle. The camera
stands over that line, 13 from the middle of it and looking at it, the
plate reaching the horizon behind: the body's own layer, fitted to every
patch the camera draws, has texels of several of the image's pixels there.
The middle row of the image (`shading.srgb_mode = 1`, a lit pixel 200) is
read across the line, with the near layer and without: with it, the
shadow's edge has to be where the line is and as sharp as a couple of
pixels; without, it is smeared over the coarse layer's texels and moved by
its bias. From 80 above the plate the near layer is
not wanted, and the image is the same with or without it.

Opens a window, in the background.
"""

import glob
import math
import os
import sys
import tempfile
import time

import numpy
from _png import read_png

from kalast.app import App

failures: list[str] = []

N = 512
HALF = 100.0
HEIGHT, THICK, LENGTH = 1.0, 0.2, 10.0
ELEVATION = math.radians(25.0)
TO_SUN = numpy.array([math.cos(ELEVATION), 0.0, math.sin(ELEVATION)])
EDGE = -(HEIGHT / math.tan(ELEVATION) + THICK / 2)
W, H = 400, 300
LIT = 200.0


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def plate() -> str:
    """The plate, N x N cells of two facets."""
    s = numpy.linspace(-HALF, HALF, N + 1)
    x, y = numpy.meshgrid(s, s)
    v = numpy.stack([x.ravel(), y.ravel(), numpy.zeros(x.size)], 1)
    i = numpy.arange(N)
    a = (i[:, None] * (N + 1) + i[None, :]).ravel()
    b, c, d = a + 1, a + N + 1, a + N + 2
    f = numpy.concatenate([numpy.stack([a, b, d], 1), numpy.stack([a, d, c], 1)])
    f = f + 1
    return "".join(["v %.6f %.6f %.6f\n" % tuple(p) for p in v]) + "".join(["f %d %d %d\n" % tuple(t) for t in f])


def wall() -> str:
    x0, x1, y0, y1 = -THICK / 2, THICK / 2, -LENGTH / 2, LENGTH / 2
    v = [(x0, y0, 0), (x1, y0, 0), (x1, y1, 0), (x0, y1, 0), (x0, y0, HEIGHT), (x1, y0, HEIGHT), (x1, y1, HEIGHT), (x0, y1, HEIGHT)]
    f = [(0, 2, 1), (0, 3, 2), (4, 5, 6), (4, 6, 7), (0, 1, 5), (0, 5, 4),
         (1, 2, 6), (1, 6, 5), (2, 3, 7), (2, 7, 6), (3, 0, 4), (3, 4, 7)]
    return "".join("v %.6f %.6f %.6f\n" % p for p in v) + "".join("f %d %d %d\n" % (a + 1, b + 1, c + 1) for a, b, c in f)


def main() -> int:
    tmp = tempfile.mkdtemp(prefix="kalast_near_layer_")
    path, path_wall = os.path.join(tmp, "plate.obj"), os.path.join(tmp, "wall.obj")
    with open(path, "w") as fh:
        fh.write(plate())
    with open(path_wall, "w") as fh:
        fh.write(wall())
    out = os.path.join(tmp, "frames")

    app = App()
    app.config.open_in_background = True
    c = app.simulation.config
    c.export.dir, c.export.sync, c.export.hud = out, True, False
    c.image.width, c.image.height = W, H
    c.axes.style = "off"
    c.wireframe.mode = 0
    c.shading.msaa = 1
    c.shading.srgb_mode = 1
    # The map's own edge, unfiltered: the test is of its texels.
    c.shadows.pcf = 0
    c.light.exposure = LIT / 255.0 / math.sin(ELEVATION)
    sim = app.simulation
    sim.load_mesh(path=path)
    sim.load_mesh(path=path_wall)
    sim.sun.pos = (1e6 * TO_SUN).tolist()
    cam = sim.camera
    # Pinned: fitted to the scene, the near plane cut the plate under the camera.
    cam.projection.near = 0.01

    def look(pos, at) -> None:
        d = numpy.subtract(at, pos)
        d = d / numpy.linalg.norm(d)
        cam.pos, cam.dir, cam.up = list(pos), d.tolist(), [0.0, 0.0, 1.0]

    def grab(near: bool) -> numpy.ndarray:
        c.shadows.near_layer = near
        for _ in range(3):
            app.step()
        before = set(glob.glob(f"{out}/*.png"))
        sim.export_once()
        app.step()
        new = set(glob.glob(f"{out}/*.png")) - before
        assert len(new) == 1, new
        return read_png(new.pop())[..., 0].astype(float)

    # The level-of-detail tree is built on a thread once the mesh is loaded,
    # and the near layer needs its patches: wait for the image to change.
    look([EDGE, -12.0, 6.0], [EDGE, 0.0, 0.0])
    t0 = time.time()
    while time.time() - t0 < 2.0:
        app.step()
    near, plain = grab(True), grab(False)
    for _ in range(10):
        if not numpy.array_equal(near, plain):
            break
        t1 = time.time()
        while time.time() - t1 < 1.0:
            app.step()
        near, plain = grab(True), grab(False)
    check("up close, the near layer changes the image", not numpy.array_equal(near, plain))

    def edge(row: numpy.ndarray) -> tuple[float, float]:
        """Where the row, lit on the left of the line and in the wall's shadow
        on its right, first falls below half the lit value, and over how many
        pixels it falls from nine tenths of it to a tenth."""
        lit = numpy.median(row[W // 2 - 80:W // 2 - 40])
        xs = numpy.arange(W // 2 - 60, W // 2 + 60)
        r = numpy.clip(row[xs] / max(lit, 1.0), 0.0, 1.0)

        def below(level: float) -> float:
            k = int(numpy.argmax(r < level))
            if k == 0:
                return float(xs[0])
            return float(xs[k - 1] + (r[k - 1] - level) / max(r[k - 1] - r[k], 1e-9))

        return below(0.5), below(0.1) - below(0.9)

    y = H // 2
    with_near, without = edge(near[y]), edge(plain[y])
    # The line runs up the image's middle: the camera stands over it.
    check(
        "with it, the shadow's edge is where it falls",
        abs(with_near[0] - W / 2) <= 1.0,
        f"half light at x {with_near[0]:.1f}, the line at {W / 2:.0f}; without it {without[0]:.1f}",
    )
    check(
        "and sharper than without",
        with_near[1] <= 2.0 and with_near[1] * 1.5 <= without[1],
        f"nine tenths to a tenth over {with_near[1]:.1f} px, without it {without[1]:.1f} px",
    )

    # From far, the body's own layer is fine enough: no near layer.
    look([EDGE, -60.0, 80.0], [EDGE, 0.0, 0.0])
    for _ in range(5):
        app.step()
    far_near, far_plain = grab(True), grab(False)
    check("from far, none: the image is the same", numpy.array_equal(far_near, far_plain),
          f"{int((far_near != far_plain).sum())} pixels differ")

    app.close()
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
