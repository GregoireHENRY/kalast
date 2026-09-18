#!/usr/bin/env python
"""The shadow array is sized to the scene and grows with it.

It used to be allocated at the cap of eight layers for every scene -- 2.1 GB
at the default 8192 before a mesh was loaded, which is what put a 16 GB
laptop out of memory. It is allocated at the body count now and grown when a
body arrives after the window exists. This loads one sphere, draws, adds a
second, and checks that the frame after the growth still renders the scene
and that mutual shadowing works across the new layer: the second sphere
stands between the Sun and the first, so the first gains occlusion it did not
have alone. (A lone sphere already reads about one half -- `facet_shadow`
counts the night side as blocked -- so it is the increase that matters.)

Opens a window, in the background.
"""

import glob
import os
import sys
import tempfile

import numpy
from _png import read_png

from kalast.app import App

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ICO = os.path.join(ROOT, "res", "ico3.obj")

failures: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def main() -> int:
    out = tempfile.mkdtemp(prefix="kalast_layers_")
    app = App()
    app.config.open_in_background = True
    app.config.width, app.config.height = 640, 480
    c = app.simulation.config
    c.export.dir, c.export.sync = out, True
    c.shadows.resolution = 1024
    c.shadows.access_shadow_map = True
    c.wireframe.mode = 0
    c.axes.style = "off"
    sim = app.simulation
    sim.load_mesh(path=ICO)
    sim.sun.pos = [10.0, 0.0, 0.0]
    sim.camera.pos = [3.0, -8.0, 4.0]
    sim.camera.look_anchor()

    def frame(name: str) -> numpy.ndarray:
        before = set(glob.glob(f"{out}/*.png"))
        sim.export_once()
        app.step()
        new = set(glob.glob(f"{out}/*.png")) - before
        assert len(new) == 1, new
        return read_png(new.pop()).max(axis=2)

    def occlusion(body: int) -> numpy.ndarray:
        v = None
        for _ in range(4):
            sim.request_facet_shadow(body)
            app.step()
            got = sim.facet_shadow(body)
            if got is not None:
                v = numpy.asarray(got, dtype=numpy.float64)
        assert v is not None, f"no facet_shadow for body {body}"
        return v

    for _ in range(2):
        app.step()  # the window, with a one-layer array
    one = frame("one")
    alone = occlusion(0)

    # A second body after the window exists: the array has to grow.
    small = numpy.eye(4)
    small[:3, :3] *= 0.4
    small[:3, 3] = [2.5, 0.0, 0.0]
    sim.load_mesh(path=ICO, mat=small)
    for _ in range(2):
        app.step()
    two = frame("two")
    shaded, other = occlusion(0), occlusion(1)
    app.close()
    app.step()

    lit_one, lit_two = int((one > 24).sum()), int((two > 24).sum())
    check("the first sphere renders", lit_one > 0.02 * one.size, f"{lit_one} px lit")
    check(
        "the second sphere renders after the array grew",
        lit_two > lit_one * 1.02,
        f"{lit_two} px lit against {lit_one}",
    )
    gain = shaded.mean() - alone.mean()
    check(
        "the first sphere gains the second's shadow through the new layer",
        gain > 0.02,
        f"mean occlusion {alone.mean():.3f} alone, {shaded.mean():.3f} with the second sphere",
    )
    check(
        "the second sphere, sunward, is only its own night side",
        abs(other.mean() - 0.5) < 0.1,
        f"mean occlusion {other.mean():.3f}",
    )

    if failures:
        print(f"\n{len(failures)} failure(s)")
        return 1
    print("\nall ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
