#!/usr/bin/env python
"""`config.reference`: the image integrated over each pixel and the Sun's
disc, progressively, to a stated error.

The wall of `tests/test_facet_shadow_disc.py` on its plate, seen from 30
above, the Sun 25 deg up with an angular radius of 0.02:

- the sum stops on its own, under `reference.error`;
- the lit plate is sin(25 deg) of the light, Lambert's;
- across the wall's penumbra, each pixel is the share of the limb-darkened
  disc its ground point sees, worked out here in float64, within the error
  the image states for itself;
- summed again from the start, the image is the same to the bit: fixed
  sequences, a plain mean.

Skipped on a GPU without ray queries. Opens a window, in the background.
"""

import math
import os
import sys
import tempfile

import numpy

from kalast.app import App

failures: list[str] = []

SUN = 0.02
ELEVATION = math.radians(25.0)
TO_SUN = numpy.array([math.cos(ELEVATION), 0.0, math.sin(ELEVATION)])
HEIGHT, THICK, LENGTH = 4.0, 0.2, 30.0
HALF, N = 20.0, 200
LIMB_DARKENING = 0.56
W, H_PX = 600, 400
CAMERA = (-9.0, 30.0)  # x and height, looking straight down


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def scene() -> str:
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
    return "".join("v %.9f %.9f %.9f\n" % tuple(p) for p in v) + "".join("f %d %d %d\n" % tuple(t + 1) for t in f)


def seen(x: float) -> float:
    """The share of the limb-darkened disc's light the ground at `x` sees."""
    s = numpy.linspace(-1, 1, 401)
    a, b = numpy.meshgrid(s, s)
    inside = a * a + b * b <= 1
    a, b = a[inside], b[inside]
    weight = 1 - LIMB_DARKENING * (1 - numpy.sqrt(numpy.clip(1 - a * a - b * b, 0, 1)))
    e1 = numpy.array([0.0, 1.0, 0.0])
    e2 = numpy.cross(TO_SUN, e1)
    d = TO_SUN[None, :] + SUN * (a[:, None] * e1 + b[:, None] * e2)
    d /= numpy.linalg.norm(d, axis=1)[:, None]
    lo = numpy.array([-THICK / 2, -LENGTH / 2, 0.0])
    hi = numpy.array([THICK / 2, LENGTH / 2, HEIGHT])
    p = numpy.array([x, 0.0, 0.0])
    with numpy.errstate(divide="ignore", invalid="ignore"):
        t0, t1 = (lo - p) / d, (hi - p) / d
    near = numpy.nanmax(numpy.minimum(t0, t1), axis=1)
    far = numpy.nanmin(numpy.maximum(t0, t1), axis=1)
    hit = (near <= far) & (far > 1e-9)
    return float((weight * ~hit).sum() / weight.sum())


def run(app: App) -> tuple[numpy.ndarray | None, numpy.ndarray | None, int, float | None]:
    sim = app.simulation
    for _ in range(20000):
        if sim.reference_done:
            break
        app.step()
    return sim.reference_image(), sim.reference_error_image(), sim.reference_samples, sim.reference_error


def main() -> int:
    path = os.path.join(tempfile.mkdtemp(prefix="kalast_reference_"), "wall.obj")
    with open(path, "w") as fh:
        fh.write(scene())

    app = App()
    app.config.open_in_background = True
    c = app.simulation.config
    c.image.width, c.image.height = W, H_PX
    c.wireframe.mode = 0
    c.axes.style = "off"
    c.light.sun_as_point = False
    sim = app.simulation
    sim.load_mesh(path=path)
    c.light.sun_radius = SUN * 1e6
    sim.sun.pos = (1e6 * TO_SUN).tolist()
    sim.camera.pos, sim.camera.dir, sim.camera.up = [CAMERA[0], 0.0, CAMERA[1]], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]
    for _ in range(3):
        app.step()
    c.shadows.rays = True
    app.step()
    if not sim.rays:
        print("skip: this GPU traces no rays here (sim.rays is False)")
        app.close()
        return 0
    c.shadows.rays = False
    c.reference.enabled = True
    c.reference.max_samples = 8192
    target = c.reference.error

    img, err, n, error = run(app)
    check("the sum stops under its error", sim.reference_done and error is not None and error <= target,
          f"{n} samples, error {error} against {target:.5f}")
    if img is None:
        app.close()
        return 1

    half = CAMERA[1] * math.tan(math.radians(15.0))
    xs = CAMERA[0] + (numpy.arange(W) + 0.5 - W / 2) * (2 * half / H_PX)
    row = img[H_PX // 2 - 2:H_PX // 2 + 2, :, 0].mean(axis=0)
    lit = float(numpy.median(row[xs < -11.0]))
    check("the lit plate is Lambert's sin(25 deg)", abs(lit - math.sin(ELEVATION)) < 1e-3, f"{lit:.5f}")
    want = numpy.array([seen(x) for x in xs]) * lit
    band = (want > 0.01 * lit) & (want < 0.99 * lit)
    e = row[band] - want[band]
    stated = float(err[H_PX // 2, band].max())
    check("across the penumbra, each pixel the disc's light its ground sees", band.sum() > 15 and float(abs(e).max()) <= 3 * max(stated, target),
          f"{band.sum()} pixels: rms {numpy.sqrt((e**2).mean()):.5f}, worst {abs(e).max():.5f}, the image's own error up to {stated:.5f}")

    # Again from the start: off and on.
    c.reference.enabled = False
    app.step()
    c.reference.enabled = True
    again, _, n2, _ = run(app)
    same = again is not None and img.shape == again.shape and bool((again == img).all())
    check("summed again, the same image to the bit", same and n2 == n,
          f"{n2} samples; {0 if again is None else int((again != img).sum())} values differ")

    app.close()
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
