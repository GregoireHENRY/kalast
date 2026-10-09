#!/usr/bin/env python
"""With `shading.srgb_mode = 1`, MSAA averages the values the image stores.

In that mode a stored value is the lit value itself (I/F times the exposure),
so a pixel a lit plate covers in k of its 4 samples must store k/4 of the
plate's value. The hardware resolve averaged the light the values decode to
instead, and a pixel half covered stored 117 of a plate's 160 rather than 80.

A plate lit and seen face on, turned in the image so its edges cross pixels
at every coverage, against a black sky: every pixel is within 2 of 0, 40,
80, 120 or 160, and the edges hold all three partial values.

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

PLATE = """v -1 -1 0
v 1 -1 0
v 1 1 0
v -1 1 0
f 1 2 3
f 1 3 4
"""

LIT = 160.0


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def main() -> int:
    tmp = tempfile.mkdtemp(prefix="kalast_msaa_")
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
    c.shading.msaa = 4
    c.shading.srgb_mode = 1
    c.light.exposure = LIT / 255.0
    sim = app.simulation
    sim.load_mesh(path=plate)
    sim.sun.pos = [0.0, 0.0, 1e6]
    cam = sim.camera
    cam.projection.fovy = math.radians(16.0)
    cam.pos, cam.dir = [0.0, 0.0, 10.0], [0.0, 0.0, -1.0]
    turn = math.radians(17.0)
    cam.up = [math.sin(turn), math.cos(turn), 0.0]

    def grab() -> numpy.ndarray:
        before = set(glob.glob(f"{out}/*.png"))
        sim.export_once()
        app.step()
        new = set(glob.glob(f"{out}/*.png")) - before
        assert len(new) == 1, new
        return read_png(new.pop())[..., 0].astype(float)

    for _ in range(3):
        app.step()
    image = grab()
    levels = LIT * numpy.arange(5) / 4.0
    off = numpy.abs(image[..., None] - levels).min(-1)
    check(
        "every pixel stores k/4 of the plate's value",
        off.max() <= 2.0,
        f"worst {off.max():.0f} away, at {image.flat[off.argmax()]:.0f}",
    )
    partial = [int((numpy.abs(image - v) <= 2).sum()) for v in levels[1:4]]
    check("the edges hold every partial coverage", min(partial) >= 1 and sum(partial) >= 20, f"pixels at 1/4, 1/2, 3/4: {partial}")
    check("the plate's middle is the lit value", abs(image[30, 30] - LIT) <= 1, f"{image[30, 30]:.0f}")

    app.close()
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
