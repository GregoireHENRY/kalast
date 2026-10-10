#!/usr/bin/env python
"""With `shadows.rays`, the image is shaded by rays too, and it agrees with
the per-facet query's rays (`tests/test_facet_shadow_rays.py`), pixel by
pixel through the facet map.

The wall of `tests/test_facet_shadow_disc.py` on its plate, seen from above
and obliquely, the Sun a point and a disc: every pixel of a facet whose
corners and centre all see the whole Sun is lit -- no acne, the specks of
black that rays from the rasterised position itself left on sunlit ground --
and every pixel of a facet whose points all see none of it is dark.

Skipped on a GPU without ray queries. Opens a window, in the background.
"""

import math
import os
import sys
import tempfile
import time

import numpy

from kalast.app import App

failures: list[str] = []

SUN = 0.02
ELEVATION = math.radians(25.0)
TO_SUN = numpy.array([math.cos(ELEVATION), 0.3, math.sin(ELEVATION)])
TO_SUN /= numpy.linalg.norm(TO_SUN)
HEIGHT, THICK, LENGTH = 4.0, 0.2, 30.0
HALF, N = 20.0, 200


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def scene() -> tuple[str, int]:
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
    return text, 2 * N * N


def main() -> int:
    text, n_plate = scene()
    tmp = tempfile.mkdtemp(prefix="kalast_image_rays_")
    path = os.path.join(tmp, "wall.obj")
    with open(path, "w") as fh:
        fh.write(text)

    app = App()
    app.config.open_in_background = True
    c = app.simulation.config
    c.image.width, c.image.height = 600, 400
    c.wireframe.mode = 0
    c.axes.style = "off"
    c.shading.lod = False
    c.shadows.rays = True
    c.shadows.ray_samples = 256
    sim = app.simulation
    sim.load_mesh(path=path)
    c.light.sun_radius = SUN * 1e6
    sim.sun.pos = (1e6 * TO_SUN).tolist()

    views = {
        "above": ([-4.0, 0.0, 45.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
        "oblique": ([-26.0, -14.0, 9.0], None, None),
    }
    for point in [True, False]:
        c.light.sun_as_point = point
        for view, (pos, direction, up) in views.items():
            sim.camera.pos = pos
            if direction is None:
                sim.camera.look_anchor()
            else:
                sim.camera.dir, sim.camera.up = direction, up
            c.export.dir = os.path.join(tmp, f"{view}_{point}")
            for _ in range(3):
                app.step()
            if not sim.rays:
                print("skip: this GPU traces no rays here (sim.rays is False)")
                app.close()
                return 0
            sim.request_facet_shadow(0)
            sim.request_facet_id()
            sim.export_once()
            app.step()
            hidden = numpy.asarray(sim.facet_shadow(0), float)
            ids, offsets = sim.facet_id_map()
            ids = numpy.asarray(ids).astype(numpy.int64)
            for _ in range(4):
                app.step()
            time.sleep(0.5)

            from PIL import Image

            frames = sorted(os.listdir(c.export.dir))
            img = numpy.asarray(Image.open(os.path.join(c.export.dir, frames[-1])).convert("L"), float)
            facet = ids - 1 - int(offsets[0])
            on_plate = (ids > 0) & (facet < n_plate)
            f = numpy.where(on_plate, facet, 0)
            lit_facet = on_plate & (hidden[f] == 0.0)
            dark_facet = on_plate & (hidden[f] == 1.0)
            # The plate faces up and the Sun is 24 deg up: lit, it is far
            # brighter than this.
            specks = lit_facet & (img < 20)
            glow = dark_facet & (img > 3)
            sun = "point Sun" if point else "the disc"
            check(f"{view}, {sun}: every pixel of a wholly lit facet lit", specks.sum() == 0 and lit_facet.sum() > 1000,
                  f"{int(specks.sum())} dark of {int(lit_facet.sum())}")
            check(f"{view}, {sun}: every pixel of a wholly hidden facet dark", glow.sum() == 0 and dark_facet.sum() > 500,
                  f"{int(glow.sum())} lit of {int(dark_facet.sum())}")
            if not point:
                partial = on_plate & (hidden[f] > 0.05) & (hidden[f] < 0.95)
                grey = partial & (img > 3) & (img < 160)
                check(f"{view}, the disc: the penumbra graded in the image", grey.sum() > 50,
                      f"{int(grey.sum())} grey pixels of {int(partial.sum())} on penumbra facets")
    app.close()
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
