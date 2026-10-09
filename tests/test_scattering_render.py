#!/usr/bin/env python
"""The renderer's scattering laws give the I/F `kalast.scattering` gives.

`body.scattering` makes a lit pixel `exposure * colour * pi * r(i, e, alpha)
* cos(i)`, with `r` from the law. The shader writes each law again, in f32,
so this holds the two to each other: a flat plate, seen and lit at chosen
angles, its middle pixel read back as I/F (`shading.srgb_mode = 1`, so
value / 255 / exposure) against the CPU law at the same angles. And no law
at all must still be what the shading always was, `cos(i)`.

The exposure puts each expected pixel near 200 of 255, so the eight bits
cost under 0.3 %; the tolerance is 1 %.

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
from kalast.scattering import Hapke, LommelSeeligerLambert

failures: list[str] = []

# The plate: z = 0, normal +z, wound counter-clockwise from above.
PLATE = """v -1 -1 0
v 1 -1 0
v 1 1 0
v -1 1 0
f 1 2 3
f 1 3 4
"""

# Deimos as Wargnier et al. (2025) fitted it, smooth and rough.
LAWS = {
    "Lambert (none)": None,
    "Lommel-Seeliger": LommelSeeligerLambert(w=0.3, c=1.0),
    "mix c = 0.4": LommelSeeligerLambert(w=0.3, c=0.4),
    "Hapke, smooth": Hapke(w=0.068, b=0.275, c=1.0, b0=2.14, h=0.065, theta_bar=0.0),
    "Hapke, rough": Hapke(w=0.068, b=0.275, c=1.0, b0=2.14, h=0.065, theta_bar=math.radians(19.4)),
    "Hapke, bright": Hapke(w=0.6, b=0.2, c=0.7, b0=1.0, h=0.05, theta_bar=math.radians(25.0)),
    "Hapke, porous": Hapke(w=0.068, b=0.275, c=1.0, b0=2.14, h=0.065, theta_bar=math.radians(19.4), k=1.21),
}

# (incidence, emission, azimuth between them), degrees.
ANGLES = [(30, 0, 0), (0, 40, 0), (50, 30, 0), (50, 30, 90), (40, 60, 180), (70, 20, 120)]


def expected(law, i, e, alpha):
    mu0, mu = math.cos(i), math.cos(e)
    if law is None:
        return mu0
    if isinstance(law, Hapke):
        return math.pi * law.reflectance(mu0, mu, alpha) * mu0
    return math.pi * law.reflectance(mu0, mu) * mu0


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def main() -> int:
    tmp = tempfile.mkdtemp(prefix="kalast_scattering_")
    plate = os.path.join(tmp, "plate.obj")
    with open(plate, "w") as f:
        f.write(PLATE)
    out = os.path.join(tmp, "frames")

    app = App()
    app.config.open_in_background = True
    c = app.simulation.config
    c.export.dir, c.export.sync, c.export.hud = out, True, False
    c.image.width = c.image.height = 41
    c.axes.style = "off"
    c.wireframe.mode = 0
    c.shading.msaa = 1
    c.shading.srgb_mode = 1
    sim = app.simulation
    sim.load_mesh(path=plate)
    body = sim.bodies[0]
    cam = sim.camera
    cam.projection.fovy = math.radians(10.0)

    def grab() -> float:
        before = set(glob.glob(f"{out}/*.png"))
        sim.export_once()
        app.step()
        new = set(glob.glob(f"{out}/*.png")) - before
        assert len(new) == 1, new
        return float(read_png(new.pop())[20, 20, 0])

    for _ in range(3):
        app.step()
    for name, law in LAWS.items():
        body.scattering = law
        worst = 0.0
        for di, de, dpsi in ANGLES:
            i, e, psi = map(math.radians, (di, de, dpsi))
            sun = numpy.array([math.sin(i), 0.0, math.cos(i)])
            eye = numpy.array([math.sin(e) * math.cos(psi), math.sin(e) * math.sin(psi), math.cos(e)])
            alpha = math.acos(float(numpy.clip(sun @ eye, -1, 1)))
            want = expected(law, i, e, alpha)
            c.light.exposure = 200.0 / 255.0 / want
            sim.sun.pos = (1e6 * sun).tolist()
            cam.pos = (10.0 * eye).tolist()
            cam.dir = (-eye).tolist()
            cam.up = [0.0, 0.0, 1.0] if abs(eye[2]) < 0.99 else [1.0, 0.0, 0.0]
            app.step()
            got = grab() / 255.0 / c.light.exposure
            err = abs(got / want - 1)
            worst = max(worst, err)
            if err > 0.01:
                print(f"     {name} at i {di} e {de} psi {dpsi}: I/F {got:.5f}, the law {want:.5f}")
        check(f"{name} gives the law's I/F", worst < 0.01, f"worst {100 * worst:.2f} % over {len(ANGLES)} geometries")

    body.scattering = None
    check("no law reads back as None", body.scattering is None)
    body.scattering = LAWS["Hapke, rough"]
    back = body.scattering
    check("a law reads back as itself", isinstance(back, Hapke) and back.theta_bar == LAWS["Hapke, rough"].theta_bar)
    for bad, error in ((Hapke(theta_bar=2.0), ValueError), (Hapke(k=0.5), ValueError), (LommelSeeligerLambert(w=1.5), ValueError), ("hapke", TypeError)):
        try:
            body.scattering = bad
            check(f"{bad!r} is refused", False)
        except error:
            check(f"{bad!r} is refused", True)

    app.close()
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
