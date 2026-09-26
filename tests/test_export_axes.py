#!/usr/bin/env python
"""`export.axes = False` keeps the axes out of exported frames, and only out
of those.

The grid, the axes and the gizmo are drawn in the main pass, into the texture
the exporter copies, so they went into every exported frame. With
`export.axes` off, a frame that is exported draws them in a second pass after
the copy: the file has the scene alone, the window keeps them.

Exports the same frame with the gizmo, `export.axes` on and off, with MSAA and
without -- the second pass loads the first one's samples then, and resolves
again -- and checks the gizmo's corner and the rest of the frame. What the
window shows is not reachable from here; that it keeps the gizmo was checked
with a capture of the window, `notes/TIMELINE.md`.

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
W, H = 400, 300

failures: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def main() -> int:
    out = tempfile.mkdtemp(prefix="kalast_export_axes_")
    app = App()
    app.config.open_in_background = True
    app.config.width, app.config.height = W, H
    c = app.simulation.config
    c.export.dir, c.export.sync = out, True
    c.wireframe.mode = 0
    c.axes.style = "gizmo"
    sim = app.simulation
    sim.load_mesh(path=os.path.join(ROOT, "res", "ico3.obj"))
    sim.camera.pos = [-6.0, -8.0, 4.0]
    sim.camera.look_anchor()

    def export(axes: bool) -> numpy.ndarray:
        c.export.axes = axes
        for _ in range(2):
            app.step()
        before = set(glob.glob(f"{out}/*.png"))
        sim.export_once()
        app.step()
        new = set(glob.glob(f"{out}/*.png")) - before
        assert len(new) == 1, new
        return read_png(new.pop())[..., :3].astype(numpy.int16)

    # The gizmo's square: top-right, 20 px in, `2 * gizmo_size` across.
    size = int(c.axes.gizmo_size)
    corner = (slice(20, 20 + 2 * size), slice(W - 20 - 2 * size, W - 20))
    elsewhere = numpy.ones((H, W), bool)
    elsewhere[corner] = False

    for msaa in (4, 1):
        c.shading.msaa = msaa
        kept = export(True)
        left = export(False)
        again = export(True)
        background = numpy.round(numpy.array(c.shading.background[:3]) * 255).astype(numpy.int16)
        ink = lambda img: int((numpy.abs(img[corner] - background).max(axis=-1) > 8).sum())
        check(f"msaa {msaa}: export.axes = True keeps the gizmo", ink(kept) > 200, f"{ink(kept)} px in its corner")
        check(f"msaa {msaa}: export.axes = False leaves it out", ink(left) == 0, f"{ink(left)} px in its corner")
        diff = int(numpy.abs(kept[elsewhere] - left[elsewhere]).max())
        check(f"msaa {msaa}: and changes nothing else", diff <= 1, f"max difference {diff}/255 outside the corner")
        check(
            f"msaa {msaa}: the next export with axes has them again",
            ink(again) == ink(kept),
            f"{ink(again)} px against {ink(kept)}",
        )

    app.close()
    app.step()

    if failures:
        print(f"\n{len(failures)} failure(s)")
        return 1
    print("\nall ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
