#!/usr/bin/env python
"""`shadows.cache` keeps the shadow layers from frame to frame, and draws one
again only when the Sun or another body has moved about its body past
`shadows.cache_degrees`.

A plate with a wall on it, one body, and a ball floating over the plate, the
other; the Sun 25 deg up. With the cache on:

1. Nothing moving, the layers are drawn once and then kept: no time in the
   shadow pass (`sim.gpu_timings()["shadow"]`).
2. The camera moved away and back: still kept, the image the same to the bit.
3. The plate turned 0.5 deg about its axis: drawn again at `cache_degrees`
   0; kept at 1, its shadows turned with it -- the image as with the cache off
   but for the shadows' edges, the Sun half a degree behind in the kept layer.
4. The per-facet query reads the kept layers: with nothing moved, the same as
   from a layer drawn that frame.

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

ELEVATION = math.radians(25.0)
TO_SUN = numpy.array([math.cos(ELEVATION), 0.2, math.sin(ELEVATION)])
HEIGHT, THICK, LENGTH = 3.0, 0.2, 12.0
BALL = 1.0


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def box(lo, hi):
    (x0, y0, z0), (x1, y1, z1) = lo, hi
    v = [(x0, y0, z0), (x1, y0, z0), (x1, y1, z0), (x0, y1, z0), (x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1)]
    f = [(0, 2, 1), (0, 3, 2), (4, 5, 6), (4, 6, 7), (0, 1, 5), (0, 5, 4),
         (1, 2, 6), (1, 6, 5), (2, 3, 7), (2, 7, 6), (3, 0, 4), (3, 4, 7)]
    return v, f


def plate_and_wall() -> str:
    """A plate 20 square of 50 x 50 cells, and the wall on it."""
    n, half = 50, 10.0
    s = numpy.linspace(-half, half, n + 1)
    x, y = numpy.meshgrid(s, s)
    v = [tuple(p) for p in numpy.stack([x.ravel(), y.ravel(), numpy.zeros(x.size)], 1)]
    f = []
    for r in range(n):
        for c in range(n):
            a = r * (n + 1) + c
            f += [(a, a + 1, a + n + 2), (a, a + n + 2, a + n + 1)]
    wv, wf = box((-THICK / 2, -LENGTH / 2, 0.0), (THICK / 2, LENGTH / 2, HEIGHT))
    f += [(a + len(v), b + len(v), c + len(v)) for a, b, c in wf]
    v += wv
    return "".join("v %.6f %.6f %.6f\n" % p for p in v) + "".join("f %d %d %d\n" % (a + 1, b + 1, c + 1) for a, b, c in f)


def ball() -> str:
    v, f = box((-BALL, -BALL, -BALL), (BALL, BALL, BALL))
    return "".join("v %.6f %.6f %.6f\n" % p for p in v) + "".join("f %d %d %d\n" % (a + 1, b + 1, c + 1) for a, b, c in f)


def turned(deg: float) -> list:
    a = math.radians(deg)
    m = numpy.eye(4)
    m[:2, :2] = [[math.cos(a), -math.sin(a)], [math.sin(a), math.cos(a)]]
    return m.tolist()


def main() -> int:
    tmp = tempfile.mkdtemp(prefix="kalast_shadow_cache_")
    plate, cube = os.path.join(tmp, "plate.obj"), os.path.join(tmp, "ball.obj")
    with open(plate, "w") as fh:
        fh.write(plate_and_wall())
    with open(cube, "w") as fh:
        fh.write(ball())
    out = os.path.join(tmp, "frames")

    app = App()
    app.config.open_in_background = True
    c = app.simulation.config
    c.export.dir, c.export.sync, c.export.hud = out, True, False
    c.image.width, c.image.height = 300, 200
    c.axes.style = "off"
    c.wireframe.mode = 0
    c.shading.msaa = 1
    c.debug.gpu_timing = True
    c.shadows.cache = True
    sim = app.simulation
    sim.load_mesh(path=plate)
    sim.load_mesh(path=cube)
    sim.bodies[1].mat[:3, 3] = [-6.0, 0.0, 4.0]
    sim.sun.pos = (1e6 * TO_SUN / numpy.linalg.norm(TO_SUN)).tolist()
    cam = sim.camera
    eye = [-8.0, -18.0, 14.0]

    def look(at_eye) -> None:
        d = numpy.subtract([-4.0, 0.0, 0.0], at_eye)
        cam.pos, cam.dir, cam.up = list(at_eye), (d / numpy.linalg.norm(d)).tolist(), [0.0, 0.0, 1.0]

    def shadow_ms(frames: int = 12, peak: bool = False) -> float:
        """The shadow pass's time over the next frames, ms: the median of the
        later half, or with `peak` the most, for the one frame that draws."""
        seen = {}
        for _ in range(frames):
            app.step()
            g = sim.gpu_timings()
            if g:
                seen[g.get("frame")] = g.get("shadow", 0.0)
        if not seen:
            return float("nan")
        values = [seen[k] for k in sorted(seen)]
        return float(max(values)) if peak else float(numpy.median(values[len(values) // 2:]))

    def grab() -> numpy.ndarray:
        for _ in range(3):
            app.step()
        before = set(glob.glob(f"{out}/*.png"))
        sim.export_once()
        app.step()
        new = set(glob.glob(f"{out}/*.png")) - before
        assert len(new) == 1, new
        return read_png(new.pop())[..., :3].astype(int)

    look(eye)
    for _ in range(5):
        app.step()
    kept = shadow_ms()
    c.shadows.cache = False
    drawn = shadow_ms()
    c.shadows.cache = True
    check("nothing moving, the layers are kept", kept < 0.25 * drawn,
          f"shadow pass {kept:.3f} ms kept, {drawn:.3f} ms drawn every frame")

    first = grab()
    look([-20.0, 6.0, 10.0])
    moved = shadow_ms(8)
    look(eye)
    again = grab()
    check("the camera moved, still kept", moved < 0.25 * drawn, f"shadow pass {moved:.3f} ms")
    check("and back, the image the same", numpy.array_equal(first, again),
          f"{int((first != again).any(axis=2).sum())} pixels differ")

    # Turned half a degree: at cache_degrees 0 drawn again, as a layer drawn
    # that frame with the cache off; at 1 kept, its shadows turned with it.
    # (A redraw is one frame, which the GPU timer may not sample: told by the
    # image instead.)
    def turn_and_grab(deg: float, degrees: float) -> numpy.ndarray:
        c.shadows.cache_degrees = degrees
        sim.bodies[0].mat[:, :] = turned(0.0)
        for _ in range(4):
            app.step()
        sim.bodies[0].mat[:, :] = turned(deg)
        return grab()

    def fresh() -> numpy.ndarray:
        c.shadows.cache = False
        image = grab()
        c.shadows.cache = True
        return image

    redrawn = turn_and_grab(0.5, 0.0)
    check("turned 0.5 deg at cache_degrees 0, drawn again", numpy.array_equal(redrawn, fresh()),
          "the image as with the cache off")
    stale = turn_and_grab(0.5, 1.0)
    held = shadow_ms(6)
    differ = (numpy.abs(stale - fresh()).max(axis=2) > 8).mean()
    check("at cache_degrees 1, kept", held < 0.25 * drawn and differ > 0.0, f"shadow pass {held:.3f} ms")
    check("kept, its shadows turned with it", differ < 0.01,
          f"{100 * differ:.2f} % of pixels differ from a layer drawn that frame")

    c.shadows.access_shadow_map = True
    c.shadows.cache_degrees = 0.0
    for _ in range(6):
        app.step()
    from_kept = numpy.asarray(sim.facet_shadow(0), dtype=float).copy()
    c.shadows.cache = False
    for _ in range(6):
        app.step()
    from_drawn = numpy.asarray(sim.facet_shadow(0), dtype=float).copy()
    check("the per-facet query reads the kept layers", numpy.array_equal(from_kept, from_drawn),
          f"{int((from_kept != from_drawn).sum())} of {from_kept.size} facets differ")

    app.close()
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
