#!/usr/bin/env python
"""`config.response`: a camera's response applied to the image by FFT on the
GPU, against `examples/phobos_shadows_fit/hrsc.py`'s, worked out here in
float64 with numpy.

A sphere lit from the side, 300 x 200 pixels, its reference image summed
with the response off; then on, which carries the sum on, the image the mean
through the response:

- with nothing set, the image unchanged;
- with the HRSC SRC's fitted response -- a core of 2.19 pixels, halos of
  1.2 % over 53 and 28.2 % over 434 -- and a smear of 4.9 along the rows,
  `hrsc.response`'s image within float32's rounding;
- the smear turned 90 deg, the same with the smear down the columns;
- off again, the mean itself.

Then the image drawn without a reference, the shadows from the maps,
through the response: the scene within a pixel of the reference's, through
the same. Exported, both opaque: the response spreads light, not coverage.

Opens a window, in the background.
"""

import math
import os
import sys
import tempfile

import numpy
from PIL import Image

from kalast.app import App

failures: list[str] = []
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
W, H = 300, 200


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def response(img: numpy.ndarray, core: float, halos: list[tuple[float, float]], smear: float, down: bool = False) -> numpy.ndarray:
    """`hrsc.response`, its parameters free: each part over the offsets of
    the image padded to twice its size, normalised there; the smear a box
    with partial ends, along the rows or down the columns."""
    h, w = img.shape
    y, x = numpy.meshgrid(numpy.fft.fftfreq(2 * h) * 2 * h, numpy.fft.fftfreq(2 * w) * 2 * w, indexing="ij")
    r = numpy.hypot(x, y)

    def spectrum(k: numpy.ndarray) -> numpy.ndarray:
        return numpy.fft.rfft2(k / k.sum())

    k = (1 - sum(f for f, _ in halos)) * spectrum(numpy.exp(-0.5 * (r / core) ** 2))
    for f, rho in halos:
        k = k + f * spectrum(numpy.exp(-r / rho))
    along, across = (y, x) if down else (x, y)
    k = k * spectrum(numpy.clip(smear / 2 + 0.5 - numpy.abs(along), 0, 1) * (across == 0))
    return numpy.fft.irfft2(numpy.fft.rfft2(numpy.pad(img, ((0, h), (0, w)))) * k, s=(2 * h, 2 * w))[:h, :w]


def opaque(folder: str) -> int:
    """The least alpha of the last frame exported to `folder`."""
    name = sorted(f for f in os.listdir(folder) if f.endswith(".png"))[-1]
    return int(numpy.asarray(Image.open(os.path.join(folder, name)).convert("RGBA"))[..., 3].min())


def main() -> int:
    app = App()
    app.config.open_in_background = True
    c = app.simulation.config
    c.image.width, c.image.height = W, H
    c.axes.style = "off"
    c.wireframe.mode = 0
    c.shading.srgb_mode = 1
    c.light.exposure = 1.0
    c.shadows.method = "reference"
    c.reference.max_samples = 64
    c.reference.error = 1e-9
    sim = app.simulation
    sim.load_mesh(path=os.path.join(ROOT, "res", "ico5.obj"))
    sim.sun.pos = [1e6, 2e5, 4e5]
    sim.camera.pos, sim.camera.dir, sim.camera.up = [0.0, 0.0, 4.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]
    sim.camera.projection.fovy = 2.0 * math.atan(1.3 / 4.0)

    def settle() -> None:
        for _ in range(3):
            app.step()
        for _ in range(500):
            if sim.reference_done:
                break
            app.step()

    settle()
    raw = sim.reference_image()[..., 0].astype(float)
    samples = sim.reference_samples
    check("the mean summed", sim.reference_done and raw.max() > 0.1, f"{samples} samples, brightest {raw.max():.3f}")

    c.response.enabled = True
    settle()
    same = sim.reference_image()[..., 0].astype(float)
    d = float(numpy.abs(same - raw).max())
    # The transform's float32 rounding, a millionth of the brightest.
    check("on, nothing set: the image unchanged, the sum kept", d <= 1e-5 * float(raw.max()) and sim.reference_samples == samples,
          f"max |diff| {d:.1e}, {sim.reference_samples} samples")

    c.response.core = 2.19
    c.response.halo_share, c.response.halo_scale = 0.012, 53.0
    c.response.halo2_share, c.response.halo2_scale = 0.282, 434.0
    c.response.smear = 4.9
    settle()
    got = sim.reference_image()[..., 0].astype(float)
    want = response(raw, 2.19, [(0.012, 53.0), (0.282, 434.0)], 4.9)
    d = float(numpy.abs(got - want).max())
    check("the HRSC SRC's response, as hrsc.py has it", d <= 2e-5 * float(want.max()),
          f"max |diff| {d:.1e}, brightest {want.max():.3f}; {100 * float(numpy.abs(want - raw).max() / raw.max()):.0f} % of it moved")
    check("the light kept within the padded image", abs(got.sum() / raw.sum() - want.sum() / raw.sum()) < 1e-4,
          f"{got.sum() / raw.sum():.5f} of it in the frame against {want.sum() / raw.sum():.5f}")
    out = tempfile.mkdtemp(prefix="kalast_response_")
    c.export.dir, c.export.sync, c.export.hud = out, True, False
    sim.export_once()
    for _ in range(3):
        app.step()
    alpha = opaque(out)
    check("the reference through it, exported opaque", alpha == 255, f"least alpha {alpha}")

    c.response.smear_angle = 90.0
    settle()
    got = sim.reference_image()[..., 0].astype(float)
    want = response(raw, 2.19, [(0.012, 53.0), (0.282, 434.0)], 4.9, down=True)
    d = float(numpy.abs(got - want).max())
    check("the smear down the columns at 90 deg", d <= 2e-5 * float(want.max()), f"max |diff| {d:.1e}")

    c.response.enabled = False
    settle()
    back = sim.reference_image()[..., 0].astype(float)
    check("off again, the mean itself", float(numpy.abs(back - raw).max()) <= 1e-6 and sim.reference_samples == samples,
          f"{sim.reference_samples} samples")

    # Without a reference: the frame drawn into floats and through the
    # response, exported.
    c.response.enabled = True
    c.response.smear_angle = 0.0
    c.shadows.method = "maps"
    out = tempfile.mkdtemp(prefix="kalast_response_")
    c.export.dir = out
    for _ in range(10):
        app.step()
    sim.export_once()
    for _ in range(3):
        app.step()
    name = sorted(f for f in os.listdir(out) if f.endswith(".png"))[-1]
    frame = numpy.asarray(Image.open(os.path.join(out, name)).convert("L")).astype(float) / 255.0
    want = response(raw, 2.19, [(0.012, 53.0), (0.282, 434.0)], 4.9)
    d = numpy.abs(frame - want)
    check("without a reference, the maps' frame through it", float(numpy.percentile(d, 99)) <= 2.5 / 255.0,
          f"99 % of pixels within {255 * numpy.percentile(d, 99):.1f} of 255 of the reference's through hrsc.py's")
    alpha = opaque(out)
    check("and exported opaque", alpha == 255, f"least alpha {alpha}")
    app.close()
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
