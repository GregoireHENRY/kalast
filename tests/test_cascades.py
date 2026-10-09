#!/usr/bin/env python
"""Cascaded shadow maps (`shadows.cascades`): finer where the camera looks
closest, and every facet still in one.

A plate 200 wide of 20,000 facets, and a wall on it, a body of its own (in
the plate's mesh, its level of detail would fold the wall into a tent over
the plate, as `tests/test_near_layer.py` found); the Sun 25 deg
up and square to the wall, so its shadow ends on a straight line; the camera
over that line, 13 from it, looking along the plate to its far end. At
`shadows.resolution = 1024`, against one layer over the scene
(`shadows.per_body = False`, texels 0.2 wide), three cascades must draw the
shadow's edge sharper across the middle row of the image; and the per-facet
query, which reads each facet's cascade, must still find the facets in the
wall's shadow and the ones out of it, the far ones from the scene's layer.

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

N, HALF = 100, 100.0
HEIGHT, THICK, LENGTH = 1.0, 0.2, 10.0
ELEVATION = math.radians(25.0)
TO_SUN = numpy.array([math.cos(ELEVATION), 0.0, math.sin(ELEVATION)])
EDGE = -(HEIGHT / math.tan(ELEVATION) + THICK / 2)
W, H = 400, 300


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def wall() -> str:
    x0, y0, x1, y1 = -THICK / 2, -LENGTH / 2, THICK / 2, LENGTH / 2
    v = [(x0, y0, 0), (x1, y0, 0), (x1, y1, 0), (x0, y1, 0), (x0, y0, HEIGHT), (x1, y0, HEIGHT), (x1, y1, HEIGHT), (x0, y1, HEIGHT)]
    f = [(0, 2, 1), (0, 3, 2), (4, 5, 6), (4, 6, 7), (0, 1, 5), (0, 5, 4),
         (1, 2, 6), (1, 6, 5), (2, 3, 7), (2, 7, 6), (3, 0, 4), (3, 4, 7)]
    return "".join("v %.6f %.6f %.6f\n" % p for p in v) + "".join("f %d %d %d\n" % (a + 1, b + 1, c + 1) for a, b, c in f)


def scene() -> tuple[str, numpy.ndarray]:
    s = numpy.linspace(-HALF, HALF, N + 1)
    x, y = numpy.meshgrid(s, s)
    v = [tuple(p) for p in numpy.stack([x.ravel(), y.ravel(), numpy.zeros(x.size)], 1)]
    f = []
    for r in range(N):
        for c in range(N):
            a = r * (N + 1) + c
            f += [(a, a + 1, a + N + 2), (a, a + N + 2, a + N + 1)]
    centres = numpy.array([numpy.mean([v[i] for i in t], axis=0) for t in f])
    text = "".join("v %.6f %.6f %.6f\n" % p for p in v) + "".join("f %d %d %d\n" % (a + 1, b + 1, c + 1) for a, b, c in f)
    return text, centres


def main() -> int:
    text, centres = scene()
    tmp = tempfile.mkdtemp(prefix="kalast_cascades_")
    path, path_wall = os.path.join(tmp, "plate.obj"), os.path.join(tmp, "wall.obj")
    with open(path, "w") as fh:
        fh.write(text)
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
    c.shadows.pcf = 0
    c.shadows.resolution = 1024
    sim = app.simulation
    sim.load_mesh(path=path)
    sim.load_mesh(path=path_wall)
    sim.sun.pos = (1e6 * TO_SUN).tolist()
    cam = sim.camera
    cam.projection.near = 0.01
    eye, at = numpy.array([EDGE, -12.0, 6.0]), numpy.array([EDGE, 0.0, 0.0])
    d = (at - eye) / numpy.linalg.norm(at - eye)
    cam.pos, cam.dir, cam.up = eye.tolist(), d.tolist(), [0.0, 0.0, 1.0]

    def grab() -> numpy.ndarray:
        for _ in range(4):
            app.step()
        before = set(glob.glob(f"{out}/*.png"))
        sim.export_once()
        app.step()
        new = set(glob.glob(f"{out}/*.png")) - before
        assert len(new) == 1, new
        return read_png(new.pop())[..., 0].astype(float)

    def edge_width(row: numpy.ndarray) -> float:
        """Pixels over which the row falls from nine tenths of the lit level
        to a tenth, across the shadow's edge in the image's middle."""
        lit = numpy.median(row[W // 2 - 80:W // 2 - 40])
        xs = numpy.arange(W // 2 - 60, W // 2 + 60)
        r = numpy.clip(row[xs] / max(lit, 1.0), 0.0, 1.0)

        def below(level: float) -> float:
            k = int(numpy.argmax(r < level))
            return float(xs[k - 1] + (r[k - 1] - level) / max(r[k - 1] - r[k], 1e-9)) if k > 0 else float(xs[0])

        return below(0.1) - below(0.9)

    c.shadows.per_body = False
    shared = edge_width(grab()[H // 2])
    c.shadows.per_body = True
    c.shadows.cascades = 3
    cascaded = edge_width(grab()[H // 2])
    check("three cascades draw the near edge sharper than one layer",
          cascaded <= 2.5 and cascaded * 1.5 <= shared,
          f"nine tenths to a tenth over {cascaded:.1f} px, one layer {shared:.1f} px")

    for _ in range(3):
        sim.request_facet_shadow(0)
        app.step()
    hidden = numpy.asarray(sim.facet_shadow(0), dtype=float)
    x, y = centres[:, 0], centres[:, 1]
    behind = (x > EDGE + 0.5) & (x < -THICK - 0.5) & (numpy.abs(y) < LENGTH / 2 - 1.0)
    clear = (numpy.abs(y) > LENGTH / 2 + 3.0) | (x < EDGE - 3.0) | (x > 3.0)
    check("each facet's cascade: behind the wall hidden, the rest lit, far ones too",
          behind.sum() > 0 and hidden[behind].min() >= 0.75 and hidden[clear].max() <= 0.25,
          f"{behind.sum()} facets behind it, least hidden {hidden[behind].min():.2f}; "
          f"{clear.sum()} clear of it, most hidden {hidden[clear].max():.2f}")

    app.close()
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
