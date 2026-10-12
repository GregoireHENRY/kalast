#!/usr/bin/env python
"""A slow traced frame in the UI app is drawn in bands -- a band a turn of the
loop, the panels drawn between them -- and comes out the image drawn whole.

The plate and wall of `tests/test_image_rays.py`, the Sun's disc traced by
256 rays a point: tens to hundreds of milliseconds a frame. Exported from the
UI app, where after its first frames each is cut into bands
(`Window::render`, said under `debug.window`), and from a plain window, which
draws it whole: the same image, to a level. And a reference image with two
bounces, summed to 32 samples a band at a time: the same mean. And the
per-facet shadows the thermophysical model reads, traced a range of facets a
turn in the UI app: the same answers. Two processes a case, a window loop
each.

Skipped on a GPU without ray queries. Opens windows, in the background.
"""

import glob
import math
import os
import subprocess
import sys
import tempfile

import numpy

failures: list[str] = []

SUN = 0.02
ELEVATION = math.radians(25.0)
HEIGHT, THICK, LENGTH = 4.0, 0.2, 30.0
EDGE = -(HEIGHT / math.tan(ELEVATION) + THICK / 2)
HALF, N = 20.0, 200
W, H = 600, 400

RUN = """
import glob, math, os, sys, numpy
from kalast.app import App
mode, path, out, method = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
app = App()
app.config.open_in_background = True
app.config.editor = mode == "editor"
c = app.simulation.config
c.debug.window = True
c.image.width, c.image.height = %d, %d
c.wireframe.mode = 0
c.axes.style = "off"
c.shading.lod = False
c.export.dir, c.export.sync, c.export.hud = out, True, False
c.shadows.method = method
c.shadows.ray_samples = 256
c.reference.bounces = 2
c.reference.max_samples = 32
c.light.sun_as_point = False
c.light.sun_radius = %r * 1e6
sim = app.simulation
sim.load_mesh(path=path)
e = math.radians(25.0)
sim.sun.pos = (1e6 * numpy.array([math.cos(e), 0.0, math.sin(e)])).tolist()
sim.camera.pos, sim.camera.dir, sim.camera.up = [%r, 0.0, 45.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]
sim.state.is_paused = False
for _ in range(8 if method == "rays" else 48):
    app.step()
print("RAYS", sim.rays, "SAMPLES", sim.reference_samples, flush=True)
sim.export_once()
sim.request_facet_shadow(0)
app.step()
os.makedirs(out, exist_ok=True)
numpy.save(os.path.join(out, "facets.npy"), numpy.asarray(sim.facet_shadow(0), float))
for _ in range(4):
    app.step()
app.close()
""" % (W, H, SUN, EDGE)


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
                      (1, 2, 6), (1, 6, 5), (2, 3, 7), (2, 7, 6), (3, 0, 4), (3, 4, 7)])
    v = numpy.concatenate([v, wv])
    f = numpy.concatenate([f, wf + (N + 1) ** 2])
    return "".join("v %.6f %.6f %.6f\n" % tuple(p) for p in v) + "".join("f %d %d %d\n" % tuple(t + 1) for t in f)


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def run(mode: str, path: str, tmp: str, method: str) -> tuple[numpy.ndarray | None, numpy.ndarray | None, str]:
    out = os.path.join(tmp, f"{mode}_{method}")
    script = os.path.join(tmp, "run.py")
    with open(script, "w") as fh:
        fh.write(RUN)
    done = subprocess.run([sys.executable, script, mode, path, out, method], capture_output=True, text=True, timeout=600)
    text = done.stdout + done.stderr
    facets_file = os.path.join(out, "facets.npy")
    facets = numpy.load(facets_file) if os.path.exists(facets_file) else None
    frames = sorted(glob.glob(os.path.join(out, "*.png")))
    if not frames:
        return None, facets, text
    from PIL import Image

    return numpy.asarray(Image.open(frames[-1]).convert("RGB"), float), facets, text


def main() -> int:
    tmp = tempfile.mkdtemp(prefix="kalast_banded_")
    path = os.path.join(tmp, "wall.obj")
    with open(path, "w") as fh:
        fh.write(scene())
    for method in ("rays", "reference"):
        whole, facets_whole, text_whole = run("plain", path, tmp, method)
        if "RAYS False" in text_whole:
            print("skip: this GPU traces no rays here (sim.rays is False)")
            return 0
        banded, facets_banded, text_banded = run("editor", path, tmp, method)
        bands = [line for line in text_banded.splitlines() if "bands," in line]
        check(f"{method}: the UI app drew traced frames in bands", len(bands) > 0, bands[-1] if bands else text_banded[-400:])
        if method == "reference":
            samples = [line for line in (text_whole + text_banded).splitlines() if line.startswith("RAYS")]
            check("reference: both summed to their 32 samples", all(line.endswith("SAMPLES 32") for line in samples), repr(samples))
        if method == "rays":
            same = facets_whole is not None and facets_banded is not None and facets_whole.shape == facets_banded.shape
            worst = float(numpy.abs(facets_whole - facets_banded).max()) if same else float("nan")
            check("rays: the per-facet shadows traced by ranges, the answers traced whole", same and worst == 0.0,
                  f"{0 if facets_whole is None else len(facets_whole)} facets, most apart {worst}")
        check(f"{method}: both exported a frame", whole is not None and banded is not None)
        if whole is None or banded is None:
            continue
        apart = numpy.abs(whole - banded).max(axis=2) > 1
        check(f"{method}: the banded image is the whole one, to a level", apart.sum() <= 0.0005 * apart.size,
              f"{int(apart.sum())} pixels apart of {apart.size}")
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
