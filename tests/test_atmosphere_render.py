#!/usr/bin/env python
"""The renderer's atmosphere gives the I/F `kalast.scattering.Atmosphere` gives.

`body.atmosphere` makes a lit pixel `exposure * I/F`, the I/F the dust and
the surface under it send back. The shader writes the model again, in f32;
this holds the two to each other. A plate of albedo 0.2 sits 1000 from its
body's centre, so the local vertical is its normal, under an atmosphere of
Mars's radius over scale height; seen and lit at chosen angles, its middle
pixel is read back as I/F (`shading.srgb_mode = 1`, value / 255 / exposure)
against `Atmosphere.iof` at the same angles -- past the terminator too, in
twilight, where the dust alone is left.

The exposure puts each expected pixel near 200 of 255; the tolerance is
1.5 %.

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
from kalast.scattering import Atmosphere

failures: list[str] = []

RADIUS = 1000.0
PLATE = "v -1 -1 {z}\nv 1 -1 {z}\nv 1 1 {z}\nv -1 1 {z}\nf 1 2 3\nf 1 3 4\n".format(z=RADIUS)
ALBEDO = 0.2
AIR = Atmosphere(tau=0.45, scale_height=RADIUS * 11.0 / 3390.0, radius=RADIUS, polar_radius=None)

# (incidence, emission, azimuth between them), degrees; past 90 the Sun is
# below the horizon.
ANGLES = [(0, 0, 0), (30, 0, 0), (0, 60, 0), (50, 30, 90), (70, 20, 120), (60, 80, 180), (85, 40, 0), (93, 30, 0)]


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def main() -> int:
    tmp = tempfile.mkdtemp(prefix="kalast_atmosphere_")
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
    body.mesh.colors[:] = ALBEDO
    body.mesh.update_gpu_colors()
    body.atmosphere = AIR
    cam = sim.camera
    cam.projection.fovy = math.radians(10.0)
    centre = numpy.array([0.0, 0.0, RADIUS])

    def grab() -> float:
        before = set(glob.glob(f"{out}/*.png"))
        sim.export_once()
        app.step()
        new = set(glob.glob(f"{out}/*.png")) - before
        assert len(new) == 1, new
        return float(read_png(new.pop())[20, 20, 0])

    for _ in range(3):
        app.step()
    worst = 0.0
    for di, de, dpsi in ANGLES:
        i, e, psi = map(math.radians, (di, de, dpsi))
        sun = numpy.array([math.sin(i), 0.0, math.cos(i)])
        eye = numpy.array([math.sin(e) * math.cos(psi), math.sin(e) * math.sin(psi), math.cos(e)])
        alpha = math.acos(float(numpy.clip(sun @ eye, -1, 1)))
        want = AIR.iof(math.cos(i), math.cos(i), math.cos(e), alpha, ALBEDO)
        c.light.exposure = 200.0 / 255.0 / want
        sim.sun.pos = (centre + 1e6 * sun).tolist()
        cam.pos = (centre + 10.0 * eye).tolist()
        cam.dir = (-eye).tolist()
        cam.up = [0.0, 0.0, 1.0] if abs(eye[2]) < 0.99 else [1.0, 0.0, 0.0]
        app.step()
        got = grab() / 255.0 / c.light.exposure
        err = abs(got / want - 1)
        worst = max(worst, err)
        print(f"     i {di:3d} e {de:3d} psi {dpsi:3d}: I/F {got:.5f}, the model {want:.5f}")
    check("the shader's atmosphere gives the model's I/F", worst < 0.015, f"worst {100 * worst:.2f} % over {len(ANGLES)} geometries")

    # The plate is on the body's own z axis, its pole: three units above an
    # ellipsoid whose polar radius is RADIUS - 3 and whose equator is far
    # below, so only the ellipsoid's pole can put the air where it is.
    high = Atmosphere(tau=0.45, scale_height=AIR.scale_height, radius=RADIUS - 40.0, polar_radius=RADIUS - 3.0)
    body.atmosphere = high
    i, e, psi = math.radians(30), math.radians(20), math.radians(60)
    sun = numpy.array([math.sin(i), 0.0, math.cos(i)])
    eye = numpy.array([math.sin(e) * math.cos(psi), math.sin(e) * math.sin(psi), math.cos(e)])
    alpha = math.acos(float(numpy.clip(sun @ eye, -1, 1)))
    want = high.iof(math.cos(i), math.cos(i), math.cos(e), alpha, ALBEDO, 1.0, 3.0)
    level = AIR.iof(math.cos(i), math.cos(i), math.cos(e), alpha, ALBEDO)
    c.light.exposure = 200.0 / 255.0 / want
    sim.sun.pos = (centre + 1e6 * sun).tolist()
    cam.pos = (centre + 10.0 * eye).tolist()
    cam.dir = (-eye).tolist()
    cam.up = [0.0, 0.0, 1.0]
    app.step()
    got = grab() / 255.0 / c.light.exposure
    check(
        "three units up at the ellipsoid's pole, the air is as thin as the model's",
        abs(got / want - 1) < 0.015,
        f"I/F {got:.5f}, the model {want:.5f}; at the level {level:.5f}",
    )

    body.atmosphere = None
    check("no atmosphere reads back as None", body.atmosphere is None)
    for bad in (Atmosphere(tau=-1.0), Atmosphere(omega=1.5), Atmosphere(g1=1.0), Atmosphere(scale_height=0.0)):
        try:
            body.atmosphere = bad
            check(f"{bad!r} is refused", False)
        except ValueError:
            check(f"{bad!r} is refused", True)

    app.close()
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
