#!/usr/bin/env python
"""PCF does not shade lit ground.

Wavy ground, `z = A sin(2 pi x / L) sin(2 pi y / L)`, slopes of at most 11
deg, under a Sun 20 deg high: no slope rises fast enough to hide the Sun from
any point, so every point is lit, and a filter of the hard shadow has nothing
to filter. With `shadows.pcf` the kernel's taps used to be a square of texels
in the shadow map's view, each compared against the receiver's plane extended
to it. At a low Sun those reach far along the ground, the hollows rose above
the plane, and they came out darkened as if by ambient occlusion -- on
Dimorphos 45 % of the lit pixels by more than 5 % at pcf 16. Now the taps lie
on the ground around each point (`sun_lookup` in `mesh_shadow.wgsl`).

The ground is drawn with the shadow test off, then at pcf 0, 4 and 8, linear
(`shading.srgb_mode = 1`), and every lit pixel held to the first.

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

HALF, N = 20.0, 200
A, L = 0.3, 10.0
ELEVATION = math.radians(20.0)
TO_SUN = numpy.array([math.cos(ELEVATION), 0.3, math.sin(ELEVATION)])
W, H = 400, 300
LIT = 200.0


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def ground() -> str:
    """N x N cells of two facets each."""
    s = numpy.linspace(-HALF, HALF, N + 1)
    x, y = numpy.meshgrid(s, s)
    z = A * numpy.sin(2 * math.pi * x / L) * numpy.sin(2 * math.pi * y / L)
    v = numpy.stack([x.ravel(), y.ravel(), z.ravel()], 1)
    i = numpy.arange(N)
    a = (i[:, None] * (N + 1) + i[None, :]).ravel()
    b, c, d = a + 1, a + N + 1, a + N + 2
    f = numpy.concatenate([numpy.stack([a, b, d], 1), numpy.stack([a, d, c], 1)]) + 1
    return "".join("v %.6f %.6f %.6f\n" % tuple(p) for p in v) + "".join("f %d %d %d\n" % tuple(t) for t in f)


def main() -> int:
    tmp = tempfile.mkdtemp(prefix="kalast_pcf_relief_")
    path = os.path.join(tmp, "ground.obj")
    with open(path, "w") as fh:
        fh.write(ground())
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
    c.shadows.resolution = 1024
    c.light.exposure = LIT / 255.0
    sim = app.simulation
    sim.load_mesh(path=path)
    sim.sun.pos = (1e6 * TO_SUN / numpy.linalg.norm(TO_SUN)).tolist()
    cam = sim.camera
    eye, at = numpy.array([0.0, -24.0, 20.0]), numpy.array([0.0, 0.0, 0.0])
    d = (at - eye) / numpy.linalg.norm(at - eye)
    cam.pos, cam.dir, cam.up = eye.tolist(), d.tolist(), [0.0, 0.0, 1.0]

    def grab() -> numpy.ndarray:
        for _ in range(3):
            app.step()
        before = set(glob.glob(f"{out}/*.png"))
        sim.export_once()
        app.step()
        new = set(glob.glob(f"{out}/*.png")) - before
        assert len(new) == 1, new
        return read_png(new.pop())[..., 0].astype(float)

    c.shading.color_mode = 3
    flat = grab()
    c.shading.color_mode = 0
    images = {}
    for pcf in (0, 4, 8):
        c.shadows.pcf = pcf
        images[pcf] = grab()

    lit = flat > 10
    check("the ground fills the view", lit.mean() > 0.5, f"{100 * lit.mean():.0f} % of the image")
    worst0 = float((1 - images[0][lit] / flat[lit]).max())
    check("at pcf 0 every point is lit", worst0 <= 0.02, f"darkest {100 * worst0:.1f} % below the unshadowed image")
    for pcf in (4, 8):
        dark = 1 - images[pcf][lit] / flat[lit]
        check(
            f"at pcf {pcf}, no lit ground is darkened",
            float(numpy.percentile(dark, 99.9)) <= 0.02 and float(dark.mean()) <= 0.002,
            f"mean {100 * dark.mean():.2f} %, 99.9th percentile {100 * numpy.percentile(dark, 99.9):.1f} %, "
            f"darkened over 5 %: {100 * (dark > 0.05).mean():.2f} % of the pixels",
        )

    app.close()
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
