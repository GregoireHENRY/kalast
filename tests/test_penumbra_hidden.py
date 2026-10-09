#!/usr/bin/env python
"""With the Sun a disc, relief the Sun cannot see still shades what lies
right behind it.

A plate with a ridge on it, 1 high and 0.2 thick, the Sun 25 deg above the
plate and square to the ridge, its angular radius 0.005: the ground within
2 behind the ridge is in its umbra. Far up the Sun's rays, 20 away, a bar 0.1
thick runs parallel to the ridge over the whole plate, and the ridge only
half of it, so the bar's shadow, a stripe of penumbra 0.8 wide, falls across
the ridge's umbra on one side and on open ground on the other. All of it one
body, in one shadow map.

The map holds what the Sun sees first. In the stripe, that is the bar: the
ridge, between the bar and the ground, is behind it, and is not in the map
where the ground's rays to the disc cross it. The disc's walk saw only the
bar and lit the stripe across the ridge's umbra, about 0.4 of the light --
the gaps users saw in the shadows on Dimorphos, there with rocks a metre or
two from the ground they shade behind two far ridges. The camera sees the
ridge, and the penumbra pass looks along each ray in its image
(`near_blocked` in `mesh_shadow.wgsl`).

Read from a camera beside the plate, looking at the ridge's shaded side: in
the stripe the ridge's umbra has to stay black, and on open ground the
stripe has to be the bar's penumbra, against rays from the ground to the
limb-darkened disc past the bar.

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

LIT = 200.0
SUN = 0.005
ELEVATION = math.radians(25.0)
TO_SUN = numpy.array([math.cos(ELEVATION), 0.0, math.sin(ELEVATION)])
# The ridge, on part of the plate's width only.
RIDGE_LO, RIDGE_HI = (0.0, -6.0, 0.0), (0.2, -0.5, 1.0)
# The bar, 20 up the Sun's rays from the middle of the stripe.
STRIPE = -1.0
BAR_AT = numpy.array([STRIPE, 0.0, 0.0]) + 20.0 * TO_SUN
BAR = 0.1
BAR_LO = (BAR_AT[0] - BAR / 2, -8.0, BAR_AT[2] - BAR / 2)
BAR_HI = (BAR_AT[0] + BAR / 2, 8.0, BAR_AT[2] + BAR / 2)
# The shader's: the Sun's limb darkening, linear in mu.
LIMB_DARKENING = 0.56
W, H = 300, 150
EYE, LOOK = numpy.array([-6.0, 0.0, 3.0]), numpy.array([-0.5, 0.0, 0.0])
FOVY = math.radians(40.0)


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def box(lo, hi) -> tuple[list, list]:
    (x0, y0, z0), (x1, y1, z1) = lo, hi
    v = [(x0, y0, z0), (x1, y0, z0), (x1, y1, z0), (x0, y1, z0), (x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1)]
    f = [(0, 2, 1), (0, 3, 2), (4, 5, 6), (4, 6, 7), (0, 1, 5), (0, 5, 4),
         (1, 2, 6), (1, 6, 5), (2, 3, 7), (2, 7, 6), (3, 0, 4), (3, 4, 7)]
    return v, f


def scene() -> str:
    """The plate, 40 square, the ridge and the bar, as one mesh: one body."""
    v = [(-20, -20, 0), (20, -20, 0), (20, 20, 0), (-20, 20, 0)]
    f = [(0, 1, 2), (0, 2, 3)]
    for lo, hi in ((RIDGE_LO, RIDGE_HI), (BAR_LO, BAR_HI)):
        bv, bf = box(lo, hi)
        f += [(a + len(v), b + len(v), c + len(v)) for a, b, c in bf]
        v += bv
    return "".join("v %.9f %.9f %.9f\n" % p for p in v) + "".join("f %d %d %d\n" % (a + 1, b + 1, c + 1) for a, b, c in f)


def ground() -> numpy.ndarray:
    """Where each pixel's ray meets the plate, x and y, NaN off it."""
    fwd = (LOOK - EYE) / numpy.linalg.norm(LOOK - EYE)
    right = numpy.cross(fwd, [0.0, 0.0, 1.0])
    right /= numpy.linalg.norm(right)
    up = numpy.cross(right, fwd)
    t = math.tan(FOVY / 2)
    i, j = numpy.meshgrid(numpy.arange(W), numpy.arange(H))
    x = ((i + 0.5) / W * 2 - 1) * t * W / H
    y = (1 - (j + 0.5) / H * 2) * t
    d = fwd + x[..., None] * right + y[..., None] * up
    with numpy.errstate(divide="ignore", invalid="ignore"):
        s = -EYE[2] / d[..., 2]
    p = EYE + s[..., None] * d
    p[s <= 0] = numpy.nan
    return p[..., :2]


def seen(points: numpy.ndarray) -> numpy.ndarray:
    """The limb-darkened disc's fraction rays from each point reach past the bar."""
    s = numpy.linspace(-1, 1, 81)
    a, b = numpy.meshgrid(s, s)
    inside = a * a + b * b <= 1
    a, b = a[inside], b[inside]
    weight = 1 - LIMB_DARKENING * (1 - numpy.sqrt(numpy.clip(1 - a * a - b * b, 0, 1)))
    e1 = numpy.array([0.0, 1.0, 0.0])
    e2 = numpy.cross(TO_SUN, e1)
    d = TO_SUN[None, :] + SUN * (a[:, None] * e1 + b[:, None] * e2)
    d /= numpy.linalg.norm(d, axis=1)[:, None]
    lo, hi = numpy.array(BAR_LO), numpy.array(BAR_HI)
    out = []
    for p in points:
        with numpy.errstate(divide="ignore", invalid="ignore"):
            t0, t1 = (lo - p) / d, (hi - p) / d
        near = numpy.nanmax(numpy.minimum(t0, t1), axis=1)
        far = numpy.nanmin(numpy.maximum(t0, t1), axis=1)
        hit = (near <= far) & (far > 1e-9)
        out.append(float((weight * ~hit).sum() / weight.sum()))
    return numpy.array(out)


def main() -> int:
    tmp = tempfile.mkdtemp(prefix="kalast_penumbra_hidden_")
    path = os.path.join(tmp, "ridge.obj")
    with open(path, "w") as f:
        f.write(scene())
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
    c.light.exposure = LIT / 255.0 / math.sin(ELEVATION)
    c.light.sun_as_point = False
    sim = app.simulation
    sim.load_mesh(path=path)
    c.light.sun_radius = SUN * 1e6
    sim.sun.pos = (1e6 * TO_SUN).tolist()
    cam = sim.camera
    cam.projection.fovy = FOVY
    d = (LOOK - EYE) / numpy.linalg.norm(LOOK - EYE)
    cam.pos, cam.dir, cam.up = EYE.tolist(), d.tolist(), [0.0, 0.0, 1.0]
    for _ in range(5):
        app.step()

    before = set(glob.glob(f"{out}/*.png"))
    sim.export_once()
    app.step()
    new = set(glob.glob(f"{out}/*.png")) - before
    assert len(new) == 1, new
    image = read_png(new.pop())[..., 0].astype(float)

    xy = ground()
    x, y = xy[..., 0], xy[..., 1]
    # The middle of the stripe, clear of the bar's own penumbra's edges, and of
    # the ridge's ends.
    band = numpy.abs(x - STRIPE) < 0.2
    umbra = band & (y > -4.5) & (y < -2.0)
    check(
        "across the ridge's umbra, the bar's stripe stays black",
        umbra.sum() > 50 and image[umbra].max() <= 2.0,
        f"brightest {image[umbra].max():.0f} of {LIT:.0f} over {umbra.sum()} px",
    )
    # On open ground the stripe is the bar's penumbra, as the rays have it:
    # its light on average, and nowhere taken for an umbra. Pixel by pixel it
    # is moved toward the ridge by about a fifth of its width, as the point
    # Sun's shadow of the bar is: the lookup's offset off the ground.
    open_ = band & (y > 2.0) & (y < 4.5)
    pts = numpy.stack([x[open_], y[open_], numpy.zeros(open_.sum())], axis=1)
    want = LIT * seen(pts)
    mean = float((image[open_] - want).mean())
    check(
        "on open ground, the stripe is the bar's penumbra",
        open_.sum() > 50 and want.max() < 0.8 * LIT and abs(mean) <= 0.02 * LIT and image[open_].min() >= 0.5 * want.min(),
        f"mean {100 * mean / LIT:+.1f} % over {open_.sum()} px, darkest {image[open_].min() / LIT:.2f} against "
        f"the rays' {want.min() / LIT:.2f}",
    )

    app.close()
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
