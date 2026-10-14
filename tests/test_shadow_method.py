#!/usr/bin/env python
"""`shadows.method = "none"`: no shadows, in the image and in the per-facet
query alike, and the colour mode by name, its old numbers still taken.

A plate 40 wide of 3,200 facets, and a wall on it, a body of its own; the Sun
25 deg up and square to the wall. With the maps, the wall's shadow falls on
the plate, in the image and in `sim.facet_shadow`. With "none" the plate is
lit throughout -- the image the maps' outside the shadow, and as bright in it
-- and no facet facing the Sun is hidden; those facing away, the wall's back
and underside, are hidden as with the maps.

`shading.color_mode` reads back a name and takes one, "shaded" or "data"; 0, 1
and 3 still work with a `DeprecationWarning`, 3 as `shadows.method = "none"`
and 0 back to the maps; the flat mode is gone, by its name or as 2.

Opens a window, in the background.
"""

import glob
import math
import os
import sys
import tempfile
import warnings

import numpy
from _png import read_png

from kalast.app import App

failures: list[str] = []

N, HALF = 40, 20.0
HEIGHT, THICK, LENGTH = 2.0, 0.4, 10.0
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


def obj(v, f) -> str:
    return "".join("v %.6f %.6f %.6f\n" % tuple(p) for p in v) + "".join("f %d %d %d\n" % (a + 1, b + 1, c + 1) for a, b, c in f)


def wall() -> tuple[str, numpy.ndarray]:
    """The wall, and its facets' normals."""
    x0, y0, x1, y1 = -THICK / 2, -LENGTH / 2, THICK / 2, LENGTH / 2
    v = numpy.array([(x0, y0, 0), (x1, y0, 0), (x1, y1, 0), (x0, y1, 0),
                     (x0, y0, HEIGHT), (x1, y0, HEIGHT), (x1, y1, HEIGHT), (x0, y1, HEIGHT)], dtype=float)
    f = [(0, 2, 1), (0, 3, 2), (4, 5, 6), (4, 6, 7), (0, 1, 5), (0, 5, 4),
         (1, 2, 6), (1, 6, 5), (2, 3, 7), (2, 7, 6), (3, 0, 4), (3, 4, 7)]
    n = numpy.array([numpy.cross(v[b] - v[a], v[c] - v[a]) for a, b, c in f])
    return obj(v, f), n / numpy.linalg.norm(n, axis=1)[:, None]


def plate() -> tuple[str, numpy.ndarray]:
    """The plate, and its facets' centres."""
    s = numpy.linspace(-HALF, HALF, N + 1)
    x, y = numpy.meshgrid(s, s)
    v = numpy.stack([x.ravel(), y.ravel(), numpy.zeros(x.size)], 1)
    f = []
    for r in range(N):
        for c in range(N):
            a = r * (N + 1) + c
            f += [(a, a + 1, a + N + 2), (a, a + N + 2, a + N + 1)]
    centres = numpy.array([v[list(t)].mean(axis=0) for t in f])
    return obj(v, f), centres


def colour_mode(c) -> None:
    """The names, and the numbers of before with their warning."""
    check("the colour mode reads back a name", c.shading.color_mode == "shaded", repr(c.shading.color_mode))
    c.shading.color_mode = "data"
    check("and takes one", c.shading.color_mode == "data", repr(c.shading.color_mode))
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        c.shading.color_mode = 3
        three = (c.shading.color_mode, c.shadows.method)
        c.shading.color_mode = 0
        zero = (c.shading.color_mode, c.shadows.method)
        c.shading.color_mode = 1
        one = c.shading.color_mode
    deprecated = [str(w.message) for w in caught if issubclass(w.category, DeprecationWarning)]
    check("3 is shaded without shadows", three == ("shaded", "none"), repr(three))
    check("0 is shaded with them again", zero == ("shaded", "maps"), repr(zero))
    check("1 is the data", one == "data", repr(one))
    check("each number with a deprecation warning", len(deprecated) == 3 and 'shadows.method = "none"' in deprecated[0],
          repr(deprecated))
    for group, field, value in ((c.shading, "color_mode", "flat"), (c.shading, "color_mode", 2), (c.shadows, "method", "bogus")):
        try:
            setattr(group, field, value)
            check(f"{field} = {value!r} is refused", False)
        except ValueError as e:
            check(f"{field} = {value!r} is refused", True, str(e))
    check("and the flat colour is gone", not hasattr(c.shading, "color"))
    c.shading.color_mode = "shaded"


def main() -> int:
    tmp = tempfile.mkdtemp(prefix="kalast_shadow_method_")
    text, centres = plate()
    path_plate, path_wall = os.path.join(tmp, "plate.obj"), os.path.join(tmp, "wall.obj")
    with open(path_plate, "w") as fh:
        fh.write(text)
    text, wall_normals = wall()
    with open(path_wall, "w") as fh:
        fh.write(text)
    out = os.path.join(tmp, "frames")

    app = App()
    app.config.open_in_background = True
    c = app.simulation.config
    colour_mode(c)
    c.export.dir, c.export.sync, c.export.hud = out, True, False
    c.image.width, c.image.height = W, H
    c.axes.style = "off"
    c.wireframe.mode = 0
    c.shading.msaa = 1
    c.shading.srgb_mode = "linear"
    c.shadows.pcf = 0
    sim = app.simulation
    sim.load_mesh(path=path_plate)
    sim.load_mesh(path=path_wall)
    sim.sun.pos = (1e6 * TO_SUN).tolist()
    cam = sim.camera
    cam.pos, cam.dir, cam.up = [EDGE / 2, 0.0, 30.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]

    def grab() -> numpy.ndarray:
        for _ in range(4):
            app.step()
        before = set(glob.glob(f"{out}/*.png"))
        sim.export_once()
        app.step()
        new = set(glob.glob(f"{out}/*.png")) - before
        assert len(new) == 1, new
        return read_png(new.pop())[..., 0].astype(float)

    def hidden(body: int) -> numpy.ndarray:
        for _ in range(3):
            sim.request_facet_shadow(body)
            app.step()
        return numpy.asarray(sim.facet_shadow(body), dtype=float)

    x, y = centres[:, 0], centres[:, 1]
    behind = (x > EDGE + 0.5) & (x < -THICK - 0.5) & (numpy.abs(y) < LENGTH / 2 - 1.0)
    facing = wall_normals @ TO_SUN > 1e-6

    c.shadows.method = "maps"
    maps = grab()
    maps_plate, maps_wall = hidden(0), hidden(1)
    c.shadows.method = "none"
    none = grab()
    none_plate, none_wall = hidden(0), hidden(1)

    check("with the maps, the plate behind the wall is hidden",
          behind.sum() > 0 and maps_plate[behind].min() >= 0.75, f"{behind.sum()} facets, least hidden {maps_plate[behind].min():.2f}")
    check("with none, no facet of the plate is", none_plate.max() <= 1e-6, f"most hidden {none_plate.max():.3g}")
    check("the wall's facets facing away are hidden either way, the others not",
          numpy.all(maps_wall[~facing] == 1.0) and numpy.all(none_wall[~facing] == 1.0)
          and numpy.all(none_wall[facing] == 0.0),
          f"maps {maps_wall.round(2).tolist()}, none {none_wall.round(2).tolist()}")

    lit = numpy.median(none[none > 0])
    shadowed = (maps < 0.1 * lit) & (none > 0.9 * lit)
    elsewhere = maps > 0.9 * lit
    check("the image: the maps' shadow is lit with none", shadowed.sum() >= 300, f"{shadowed.sum()} pixels, lit at {lit:.0f}")
    check("and the rest as the maps have it", numpy.abs(none - maps)[elsewhere].max() <= 1.0,
          f"most apart {numpy.abs(none - maps)[elsewhere].max():.0f}")
    check("no pixel of it dim", not numpy.any((none > 0.05 * lit) & (none < 0.9 * lit)),
          f"{int(((none > 0.05 * lit) & (none < 0.9 * lit)).sum())} pixels")

    app.close()
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
