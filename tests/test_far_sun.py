#!/usr/bin/env python
"""The Sun at its true distance lights the scene like a Sun nearby.

`sun.pos = p_sun` straight from SPICE -- 1.5e8 km -- rendered Didymos black,
and the Hera examples carried `p_sun / AU_KM * 500.0` to work around it. The
light's view matrix had its eye at the Sun, so it held a translation of
1.5e8 km, which the GPU applies in f32 with a precision of about 9 km: a
2 km scene collapsed into one quantum and every shadow depth was noise. The
eye now sits just outside the scene on the line from the Sun, which is all an
orthographic light needs.

Renders two spheres twice, the Sun at 50 units and at 1.496e8 along the same
direction, and compares the frames: the same pixels lit, to a few percent,
and the same brightness to within the difference a 1-degree change in light
direction makes.

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

failures: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def main() -> int:
    out = tempfile.mkdtemp(prefix="kalast_far_sun_")
    app = App()
    app.config.open_in_background = True
    app.config.width, app.config.height = 800, 600
    c = app.simulation.config
    c.export.dir, c.export.sync = out, True
    c.wireframe.mode = 0
    c.axes.style = "off"
    sim = app.simulation
    sim.load_mesh(path=os.path.join(ROOT, "res", "ico3.obj"))
    small = numpy.eye(4)
    small[:3, :3] *= 0.2
    small[:3, 3] = [0.0, 3.0, 0.0]
    sim.load_mesh(path=os.path.join(ROOT, "res", "ico3.obj"), mat=small)
    sim.camera.pos = [-6.0, -8.0, 4.0]
    sim.camera.look_anchor()
    direction = numpy.array([1.0, -2.0, 1.0])
    direction /= numpy.linalg.norm(direction)

    def render(distance: float) -> numpy.ndarray:
        sim.sun.pos = direction * distance
        for _ in range(2):
            app.step()
        before = set(glob.glob(f"{out}/*.png"))
        sim.export_once()
        app.step()
        new = set(glob.glob(f"{out}/*.png")) - before
        assert len(new) == 1, new
        return read_png(new.pop()).max(axis=2).astype(numpy.float64)

    near = render(50.0)
    far = render(1.496e8)
    app.close()
    app.step()

    lit_near, lit_far = int((near > 24).sum()), int((far > 24).sum())
    check("the near Sun lights something", lit_near > 0.02 * near.size, f"{lit_near} px lit")
    check(
        "the Sun at 1 AU lights the same pixels",
        abs(lit_far - lit_near) <= 0.03 * lit_near,
        f"{lit_far} px lit against {lit_near}",
    )
    both = (near > 24) & (far > 24)
    diff = float(numpy.abs(near[both] - far[both]).mean()) if both.any() else 255.0
    check("and to the same brightness", diff < 8.0, f"mean |difference| {diff:.2f}/255 on lit pixels")

    if failures:
        print(f"\n{len(failures)} failure(s)")
        return 1
    print("\nall ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
