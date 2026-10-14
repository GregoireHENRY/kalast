#!/usr/bin/env python
"""`shading.srgb_mode`: how the lit image is stored, by name; colours stored
as given whatever it is.

A plate of two facets seen face on, lit face on, its facets 0.2 and 0.6:

- the data mode stores each facet's colour as given, `"screen"` or
  `"linear"` alike -- `"linear"` stored them lighter until 14 October;
- the shaded mode stores the lit value, `exposure * I/F`, as it is with
  `"linear"`, and through the sRGB curve with `"screen"`;
- a facet of its own colour on the lit plate (`mesh.color_modes`) is
  stored as given in both;
- the names are what the field reads and takes; 0 and 1 are still taken,
  with a deprecation warning, and anything else refused.

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

PLATE = """v -1 -1 0
v 1 -1 0
v 1 1 0
v -1 1 0
f 1 2 3
f 1 3 4
"""
COLOURS = (0.2, 0.6)
EXPOSURE = 0.5


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def encode(v: float) -> float:
    """sRGB's curve, encoding."""
    return 12.92 * v if v <= 0.0031308 else 1.055 * v ** (1 / 2.4) - 0.055


def main() -> int:
    tmp = tempfile.mkdtemp(prefix="kalast_srgb_")
    plate = os.path.join(tmp, "plate.obj")
    with open(plate, "w") as f:
        f.write(PLATE)
    out = os.path.join(tmp, "frames")

    app = App()
    app.config.open_in_background = True
    c = app.simulation.config
    c.export.dir, c.export.sync, c.export.hud = out, True, False
    c.image.width = c.image.height = 61
    c.axes.style = "off"
    c.wireframe.mode = 0
    c.light.exposure = EXPOSURE
    sim = app.simulation
    sim.load_mesh(path=plate)
    mesh = sim.bodies[0].mesh
    for k, v in enumerate(COLOURS):
        mesh.colors[k, :] = v
    mesh.update_gpu_colors()
    sim.sun.pos = [0.0, 0.0, 1e6]
    cam = sim.camera
    cam.projection.fovy = math.radians(16.0)
    cam.pos, cam.dir, cam.up = [0.0, 0.0, 10.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]

    def facets() -> tuple[float, float]:
        """Each facet's pixel as exported, 0 to 255: the middle of the lower
        right triangle, then of the upper left."""
        for _ in range(3):
            app.step()
        before = set(glob.glob(f"{out}/*.png"))
        sim.export_once()
        app.step()
        new = set(glob.glob(f"{out}/*.png")) - before
        assert len(new) == 1, new
        image = read_png(new.pop())[..., 0].astype(float)
        return float(image[41, 41]), float(image[19, 19])

    check("the default is \"screen\"", c.shading.srgb_mode == "screen", c.shading.srgb_mode)

    c.shading.color_mode = "data"
    for mode in ("screen", "linear"):
        c.shading.srgb_mode = mode
        got = facets()
        want = tuple(255 * v for v in COLOURS)
        check(f"data, {mode}: each colour as given", all(abs(g - w) <= 1 for g, w in zip(got, want)),
              f"{got[0]:.0f} and {got[1]:.0f} against {want[0]:.0f} and {want[1]:.0f}")

    c.shading.color_mode = "shaded"
    for mode, stored in (("linear", lambda v: v), ("screen", encode)):
        c.shading.srgb_mode = mode
        got = facets()
        want = tuple(255 * stored(EXPOSURE * v) for v in COLOURS)
        check(f"shaded, {mode}: the lit value {'as it is' if mode == 'linear' else 'through the curve'}",
              all(abs(g - w) <= 1 for g, w in zip(got, want)),
              f"{got[0]:.0f} and {got[1]:.0f} against {want[0]:.1f} and {want[1]:.1f}")

    # The lower right facet its own colour on the lit plate.
    mesh.color_modes[0] = 1
    mesh.update_gpu_colors()
    for mode in ("screen", "linear"):
        c.shading.srgb_mode = mode
        got = facets()
        check(f"shaded, {mode}: a facet of its own colour as given", abs(got[0] - 255 * COLOURS[0]) <= 1,
              f"{got[0]:.0f} against {255 * COLOURS[0]:.0f}")

    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        c.shading.srgb_mode = 1
        one = c.shading.srgb_mode
        c.shading.srgb_mode = 0
        zero = c.shading.srgb_mode
    deprecated = [w for w in caught if issubclass(w.category, DeprecationWarning)]
    check("1 and 0 still taken, as \"linear\" and \"screen\", each with a deprecation warning",
          (one, zero) == ("linear", "screen") and len(deprecated) == 2, f"{one}, {zero}; {len(deprecated)} warning(s)")
    refused = []
    for bad in ("srgb", 2):
        try:
            c.shading.srgb_mode = bad
        except ValueError:
            refused.append(bad)
    check("anything else refused", refused == ["srgb", 2], f"refused {refused}")

    app.close()
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
