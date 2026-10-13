#!/usr/bin/env python
"""`shadows.ray_points`: a pixel's rays from several of its points, so a
shadow narrower than a pixel is the share of it the shadow covers.

A plate seen from above, with fins across it a fifth of a pixel thick whose
shadows, under a Sun 20 deg up, are half a pixel wide: from each pixel's
centre alone (`ray_points` 1) the shadows come out as whole dark pixels or
none, from 16 points as grey -- against the reference, which integrates
each pixel's area, a fraction of the error. A pixel away from every fin is
the same lit in all three. The Sun a point, so that only the pixel's area is
sampled.

Skipped on a GPU without ray queries. Opens a window, in the background.
"""

import math
import os
import sys
import tempfile

import numpy
from PIL import Image

from kalast.app import App

failures: list[str] = []

SIZE = 300  # pixels, the plate's 10 units across them
PIXEL = 10.0 / SIZE
ELEVATION = math.radians(20.0)
TO_SUN = numpy.array([math.cos(ELEVATION), 0.0, math.sin(ELEVATION)])
HEIGHT = 0.5 * PIXEL * math.tan(ELEVATION)  # a shadow half a pixel long
THICK = 0.2 * PIXEL
FINS = numpy.arange(-4.5, 4.6, 0.37)  # 11.1 pixels apart: every phase


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def scene() -> str:
    """The plate, two facets, and a box a fin across it at each of `FINS`."""
    v = [(-5.0, -5.0, 0.0), (5.0, -5.0, 0.0), (5.0, 5.0, 0.0), (-5.0, 5.0, 0.0)]
    f = [(0, 1, 2), (0, 2, 3)]
    box = [(0, 2, 1), (0, 3, 2), (4, 5, 6), (4, 6, 7), (0, 1, 5), (0, 5, 4),
           (1, 2, 6), (1, 6, 5), (2, 3, 7), (2, 7, 6), (3, 0, 4), (3, 4, 7)]
    for x in FINS:
        x0, x1, y0, y1 = x - THICK / 2, x + THICK / 2, -4.0, 4.0
        base = len(v)
        v += [(x0, y0, 0.0), (x1, y0, 0.0), (x1, y1, 0.0), (x0, y1, 0.0),
              (x0, y0, HEIGHT), (x1, y0, HEIGHT), (x1, y1, HEIGHT), (x0, y1, HEIGHT)]
        f += [(a + base, b + base, c + base) for a, b, c in box]
    return "".join("v %.9f %.9f %.9f\n" % p for p in v) + "".join("f %d %d %d\n" % (a + 1, b + 1, c + 1) for a, b, c in f)


def main() -> int:
    path = os.path.join(tempfile.mkdtemp(prefix="kalast_fins_"), "fins.obj")
    with open(path, "w") as fh:
        fh.write(scene())

    app = App()
    app.config.open_in_background = True
    c = app.simulation.config
    c.image.width = c.image.height = SIZE
    c.axes.style = "off"
    c.wireframe.mode = 0
    c.shading.srgb_mode = 1
    c.light.exposure = 1.0
    c.light.sun_as_point = True
    c.shadows.method = "rays"
    sim = app.simulation
    sim.load_mesh(path=path)
    sim.sun.pos = (1e6 * TO_SUN).tolist()
    sim.camera.pos, sim.camera.dir, sim.camera.up = [0.0, 0.0, 40.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]
    sim.camera.projection.fovy = 2.0 * math.atan(5.0 / 40.0)
    for _ in range(3):
        app.step()
    if not sim.rays:
        print("skip: this GPU traces no rays here (sim.rays is False)")
        app.close()
        return 0

    def rays(points: int) -> numpy.ndarray:
        """The rays' image from `points` points a pixel, as exported: the
        values the image stores, I/F, at an exposure of 1."""
        c.shadows.ray_points = points
        for _ in range(4):
            app.step()
        out = tempfile.mkdtemp(prefix="kalast_fins_frame_")
        c.export.dir = out
        c.export.sync = True
        c.export.hud = False
        sim.export_once()
        for _ in range(3):
            app.step()
        name = sorted(f for f in os.listdir(out) if f.endswith(".png"))[-1]
        return numpy.asarray(Image.open(os.path.join(out, name)).convert("L")).astype(float) / 255.0

    one = rays(1)
    sixteen = rays(16)
    c.shadows.method = "reference"
    c.reference.max_samples = 256
    c.reference.error = 1e-9
    app.step()
    app.step()
    for _ in range(2000):
        if sim.reference_done:
            break
        app.step()
    reference = sim.reference_image()[..., 0]
    app.close()

    # The fins' pixels: those the reference has darker than the plate.
    lit = float(numpy.median(reference))
    near = numpy.abs(reference - lit) > 0.01
    far = ~near
    for k in range(1, 3):
        far &= ~numpy.roll(near, k, 1) & ~numpy.roll(near, -k, 1)
    e1 = float(numpy.sqrt(numpy.mean((one[near] - reference[near]) ** 2)))
    e16 = float(numpy.sqrt(numpy.mean((sixteen[near] - reference[near]) ** 2)))
    check("16 points: the shadows' pixels near the reference's", e16 <= 0.5 * e1,
          f"rms {e16:.3f} against {e1:.3f} from the centre alone, over {int(near.sum())} pixels")
    check("16 points: grey where the centre alone is all or nothing",
          float(numpy.mean((sixteen[near] > 0.05 * lit) & (sixteen[near] < 0.95 * lit))) > 0.5,
          f"{100 * numpy.mean((sixteen[near] > 0.05 * lit) & (sixteen[near] < 0.95 * lit)):.0f} % of them in between")
    d = float(numpy.abs(sixteen[far] - one[far]).max())
    check("away from the fins, the same image", d <= 1.5 / 255.0, f"largest difference {d * 255:.1f} of 255")
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
