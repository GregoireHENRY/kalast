#!/usr/bin/env python
"""PCF is a filter, not a shift.

Percentage-closer filtering averages the shadow comparison over a kernel of
texels. That softens a shadow's edge; it must not move it. A blur conserves
both the integral of darkness over the image and its centroid, so either one
drifting with the kernel radius is a bias artefact, not filtering.

It happened: the normal offset used to grow with the radius, `lb.x (1 + N)`,
and lifting the lookup off the surface moves a grazing shadow's edge by far
more than the lift -- Dimorphos's shadow on Didymos detached from the
terminator at `pcf = 7` and all but vanished at 16. See
`notes/2026-09-17_pcf_erosion.md` for the measurements behind the thresholds.

This renders the crater from `res/` at shadow resolution 1024 with the Sun at
33 degrees, exports frames with the shadow test off and at pcf 0 and 4, and
over the surface lit without shadows compares pcf 4 to pcf 0:

- the darkness-weighted **centroid** moves less than 0.5 % of the image width
  (the fixed shader: 0.17 %; the old one: 1.1 %);
- the darkness **integral** is conserved within 5 % (fixed: -1.8 %; old: +4.7 %,
  false darkening from far taps the clamped receiver-plane term let flip).

The frames are decoded here rather than through Pillow, which kalast does not
depend on: eight-bit RGB/RGBA, non-interlaced, is all the exporter writes.
"""

import glob
import os
import struct
import sys
import tempfile
import zlib

import numpy

from kalast.app import App

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WIDTH, HEIGHT = 800, 600

failures: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def read_png(path: str) -> numpy.ndarray:
    """Decode an 8-bit RGB/RGBA non-interlaced PNG to (h, w, 3) uint8."""
    with open(path, "rb") as f:
        data = f.read()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", path
    pos, idat = 8, []
    while pos < len(data):
        (n,) = struct.unpack(">I", data[pos : pos + 4])
        kind, body = data[pos + 4 : pos + 8], data[pos + 8 : pos + 8 + n]
        pos += 12 + n
        if kind == b"IHDR":
            w, h, depth, ctype, _, _, interlace = struct.unpack(">IIBBBBB", body)
            assert (depth, interlace) == (8, 0) and ctype in (2, 6), (depth, ctype, interlace)
            ch = 3 if ctype == 2 else 4
        elif kind == b"IDAT":
            idat.append(body)
        elif kind == b"IEND":
            break
    raw = numpy.frombuffer(zlib.decompress(b"".join(idat)), dtype=numpy.uint8)
    stride = w * ch
    rows = raw.reshape(h, stride + 1)
    out = numpy.zeros((h, stride), dtype=numpy.int32)
    prev = numpy.zeros(stride, dtype=numpy.int32)
    for y in range(h):
        ft, line = int(rows[y, 0]), rows[y, 1:].astype(numpy.int32)
        if ft == 0:
            cur = line
        elif ft == 2:  # Up
            cur = (line + prev) & 255
        elif ft == 1:  # Sub: a running sum per channel
            cur = line.reshape(w, ch).cumsum(axis=0).reshape(stride) & 255
        else:  # Average (3) and Paeth (4) recurse on the decoded left pixel
            cur = numpy.empty(stride, dtype=numpy.int32)
            for x in range(stride):
                a = int(cur[x - ch]) if x >= ch else 0
                b = int(prev[x])
                c = int(prev[x - ch]) if x >= ch else 0
                if ft == 3:
                    p = (a + b) >> 1
                else:
                    pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
                    p = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                cur[x] = (int(line[x]) + p) & 255
        out[y] = cur
        prev = cur
    return out.reshape(h, w, ch)[..., :3].astype(numpy.uint8)


def main() -> int:
    out = tempfile.mkdtemp(prefix="kalast_pcf_")

    app = App()
    app.config.open_in_background = True
    app.config.width, app.config.height = WIDTH, HEIGHT
    c = app.simulation.config
    c.export.dir, c.export.sync = out, True
    c.shadows.resolution = 1024
    c.shading.render_back_face = True
    c.wireframe.mode = 0
    c.axes.style = "off"
    sim = app.simulation
    a = 1.0
    sim.sun.pos = [0.0, 20.0 * numpy.sin(a), 20.0 * numpy.cos(a)]
    sim.camera.pos = [1.5778934, 1.9384689, 1.5082116]
    sim.camera.up = [-0.3261482, -0.40068075, 0.85620236]
    sim.camera.dir = [-0.54051036, -0.6640262, -0.5166407]
    sim.load_mesh(
        path=os.path.join(ROOT, "res", "plane_crater_1024-5000_h=0.437.obj"),
        mat=numpy.eye(4),
    )

    def grab(name: str) -> numpy.ndarray:
        before = set(glob.glob(f"{out}/*.png"))
        sim.export_once()
        app.step()
        new = set(glob.glob(f"{out}/*.png")) - before
        assert len(new) == 1, new
        path = new.pop()
        return read_png(path).max(axis=2).astype(numpy.float64)

    for _ in range(3):
        app.step()
    c.shading.color_mode = 3  # shadow test off, shading otherwise the same
    app.step()
    ref = grab("noshadow")
    c.shading.color_mode = 0
    lit = ref > 24.0
    yy, xx = numpy.mgrid[0 : ref.shape[0], 0 : ref.shape[1]]

    def darkness(pcf: int):
        c.shadows.pcf = pcf
        app.step()
        v = grab(f"pcf{pcf}")
        d = numpy.where(lit, 1.0 - numpy.clip(v / numpy.maximum(ref, 1.0), 0.0, 1.0), 0.0)
        w = d.sum()
        return w, (d * xx).sum() / w, (d * yy).sum() / w

    w0, x0, y0 = darkness(0)
    w4, x4, y4 = darkness(4)
    app.close()
    app.step()

    check("a shadow exists to measure", w0 > 0.02 * lit.sum(), f"{w0:.0f} px-equivalents dark")
    shift = numpy.hypot(x4 - x0, y4 - y0) / WIDTH * 100.0
    check(
        "pcf 4 does not move the shadow",
        shift < 0.5,
        f"centroid moved {shift:.2f} % of the width (old shader: 1.1 %)",
    )
    drift = (w4 - w0) / w0 * 100.0
    check(
        "pcf 4 conserves the darkness integral",
        abs(drift) < 5.0,
        f"{drift:+.1f} % (old shader: +4.7 %)",
    )

    if failures:
        print(f"\n{len(failures)} failure(s)")
        return 1
    print("\nall ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
