#!/usr/bin/env python
"""`config.stretch`: the image's values shown as SAO DS9 scales an image,
its limits found on the GPU, against numpy and astropy.

A sphere lit from the side, 300 x 200 pixels, its reference image summed
without a stretch: the values themselves, `sim.reference_image()`, a value
a pixel's largest channel. Then the sum carried on through each stretch,
the exported frame against those values through the same in float64:

- each scale between user limits, within an 8-bit step;
- minmax: the least and greatest values, exactly;
- percent: the limits within their histogram bin of numpy's percentiles,
  the image through them;
- zscale and zmax: astropy's `ZScaleInterval` on the same 1000 samples;
- histequ: numpy's equalisation over the same bins;
- the sum never begun again; off, the values back.

Then the shadows from the maps, no reference: the frame drawn into floats
and shown through it.

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
EQ_BINS = 2048


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def function(scale: str, x: numpy.ndarray, a: float = 1000.0) -> numpy.ndarray:
    """DS9's scales, each 1 at 1."""
    return {
        "linear": lambda: x,
        "log": lambda: numpy.log(a * x + 1) / numpy.log(a + 1),
        "pow": lambda: (a**x - 1) / (a - 1),
        "sqrt": lambda: numpy.sqrt(x),
        "squared": lambda: x * x,
        "asinh": lambda: numpy.arcsinh(10 * x) / numpy.arcsinh(10),
        "sinh": lambda: numpy.sinh(3 * x) / numpy.sinh(3),
    }[scale]()


def through(values: numpy.ndarray, low: float, high: float, scale: str) -> numpy.ndarray:
    return function(scale, numpy.clip((values - low) / max(high - low, 1e-30), 0, 1))


def equalised(values: numpy.ndarray, low: float, high: float) -> numpy.ndarray:
    """Histogram equalisation over `stretch.wgsl`'s bins: the largest
    channels between the limits counted into 2048 of equal width, each
    channel at its running share, linearly across its bin."""
    v = values.max(axis=-1).ravel()
    span = high - low
    inside = v[(v >= low) & (v <= high)]
    k = numpy.minimum(((inside - low) / span * EQ_BINS).astype(int), EQ_BINS - 1)
    cdf = numpy.concatenate([[0.0], numpy.cumsum(numpy.bincount(k, minlength=EQ_BINS))]) / max(len(inside), 1)
    at = numpy.clip((values - low) / span, 0, 1) * EQ_BINS
    i = numpy.minimum(at.astype(int), EQ_BINS - 1)
    return cdf[i] + (at - i) * (cdf[i + 1] - cdf[i])


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
    out = tempfile.mkdtemp(prefix="kalast_stretch_")
    c.export.dir, c.export.sync, c.export.hud = out, True, False

    def settle() -> None:
        for _ in range(3):
            app.step()
        for _ in range(500):
            if sim.reference_done:
                break
            app.step()

    def frame() -> numpy.ndarray:
        """The frame as exported, 0 to 1, after the limits have been read
        back."""
        for _ in range(4):
            app.step()
        sim.export_once()
        for _ in range(3):
            app.step()
        name = sorted(f for f in os.listdir(out) if f.endswith(".png"))[-1]
        return numpy.asarray(Image.open(os.path.join(out, name)).convert("RGB")).astype(float) / 255.0

    def close(name: str, got: numpy.ndarray, want: numpy.ndarray, detail: str = "") -> None:
        d = numpy.abs(got - want)
        check(name, float(d.max()) <= 2.0 / 255 and float(numpy.percentile(d, 99.9)) <= 1.0 / 255,
              f"max |diff| {255 * d.max():.2f}/255, 99.9 % within {255 * numpy.percentile(d, 99.9):.2f}" + (f"; {detail}" if detail else ""))

    settle()
    raw = sim.reference_image().astype(float)
    samples = sim.reference_samples
    v = raw.max(axis=-1)
    check("the values summed", sim.reference_done and v.max() > 0.5, f"{samples} samples, brightest {v.max():.3f}")
    check("off: no limits", sim.stretch_limits is None)
    close("off: the values themselves", frame(), numpy.clip(raw, 0, 1))

    c.stretch.low, c.stretch.high = 0.1, 0.6
    for scale in ("linear", "log", "pow", "sqrt", "squared", "asinh", "sinh"):
        c.stretch.scale = scale
        close(f"{scale}, user limits 0.1 to 0.6", frame(), through(raw, 0.1, 0.6, scale))
    c.stretch.scale, c.stretch.exponent = "log", 10.0
    close("log with an exponent of 10", frame(), function("log", numpy.clip((raw - 0.1) / 0.5, 0, 1), 10.0))
    c.stretch.exponent = 1000.0

    c.stretch.scale, c.stretch.limits = "sqrt", "minmax"
    got = frame()
    low, high = sim.stretch_limits
    # The mean decoded into the frame and encoded again: to float32's last
    # places.
    check("minmax: the least and greatest values", abs(low - v.min()) <= 1e-6 and abs(high - v.max()) <= 1e-5 * v.max(),
          f"{low:.7g} to {high:.7g} against {v.min():.7g} to {v.max():.7g}")
    close("minmax through sqrt", got, through(raw, v.min(), v.max(), "sqrt"))

    for percent in (99.5, 90.0):
        c.stretch.limits, c.stretch.percent = "percent", percent
        got = frame()
        low, high = sim.stretch_limits
        want = numpy.percentile(v, [(100 - percent) / 2, 100 - (100 - percent) / 2])
        # A bin is 1/32 of an octave, 2.2 % wide; the limit is drawn across it.
        near = all(abs(g - w) <= 0.035 * w + 1e-7 for g, w in zip((low, high), want))
        check(f"percent {percent}: numpy's percentiles to a bin", near, f"{low:.5g} to {high:.5g} against {want[0]:.5g} to {want[1]:.5g}")
        close(f"percent {percent} through sqrt", got, through(raw, low, high, "sqrt"))

    c.stretch.scale, c.stretch.limits = "histequ", "minmax"
    got = frame()
    low, high = sim.stretch_limits
    close("histequ, minmax: numpy's equalisation", got, equalised(raw, low, high))
    c.stretch.limits, c.stretch.low, c.stretch.high = "user", 0.2, 0.7
    close("histequ between user limits", frame(), equalised(raw, 0.2, 0.7))

    check("the sum carried on through them all", sim.reference_done and sim.reference_samples == samples, f"{sim.reference_samples} samples")

    # zscale wants values as a sky's are, which a sphere on black is not --
    # its line about the median is cut to the samples' own least and
    # greatest, whatever the contrast: the sphere filling the frame, lit
    # from the front, each facet's albedo near 0.5, a few far above and
    # below.
    from astropy.visualization import ZScaleInterval

    c.stretch.scale, c.stretch.low, c.stretch.high = "linear", 0.0, 1.0
    rng = numpy.random.default_rng(1)
    n = len(sim.bodies[0].mesh.colors)
    albedo = 0.5 + 0.01 * rng.standard_normal(n)
    albedo[rng.random(n) < 0.05] = 0.9
    albedo[rng.random(n) < 0.05] = 0.1
    sim.bodies[0].mesh.colors[:] = albedo[:, None]
    sim.bodies[0].mesh.update_gpu_colors()
    sim.camera.projection.fovy = 2.0 * math.atan(0.2 / 4.0)
    sim.sun.pos = [1e5, 5e4, 1e6]
    settle()
    v = sim.reference_image().astype(float).max(axis=-1)
    c.stretch.scale = "sqrt"
    for limits, contrast in (("zscale", 0.25), ("zmax", 0.25), ("zscale", 0.1)):
        c.stretch.limits, c.stretch.contrast = limits, contrast
        got = frame()
        low, high = sim.stretch_limits
        want = ZScaleInterval(contrast=contrast).get_limits(v.ravel())
        want = (want[0], v.max() if limits == "zmax" else want[1])
        near = all(abs(g - w) <= 1e-4 * max(abs(w), 1e-3) for g, w in zip((low, high), want))
        check(f"{limits} at a contrast of {contrast}: astropy's", near,
              f"{low:.6g} to {high:.6g} against {want[0]:.6g} to {want[1]:.6g}, the values {v.min():.3g} to {v.max():.3g}")
        close(f"{limits} at {contrast} through sqrt", got, through(sim.reference_image().astype(float), low, high, "sqrt"))
    c.stretch.contrast = 0.25
    raw = sim.reference_image().astype(float)

    c.stretch.scale, c.stretch.limits, c.stretch.low, c.stretch.high = "linear", "user", 0.0, 1.0
    close("off again: the values themselves", frame(), numpy.clip(raw, 0, 1))
    check("off again: no limits", sim.stretch_limits is None)

    # The maps, no reference: drawn into floats single-sampled, as with the
    # stretch off at one sample a pixel.
    c.shadows.method = "maps"
    c.shading.msaa = 1
    plain = frame()
    c.stretch.scale, c.stretch.limits = "linear", "minmax"
    got = frame()
    low, high = sim.stretch_limits
    pv = plain.max(axis=-1)
    check("maps: minmax read back", abs(low - pv.min()) <= 1 / 255 and abs(high - pv.max()) <= 1 / 255, f"{low:.4f} to {high:.4f}")
    d = numpy.abs(got - through(plain, low, high, "linear"))
    check("maps through the stretch", float(numpy.percentile(d, 99)) <= 2.0 / 255, f"99 % within {255 * numpy.percentile(d, 99):.2f}/255")

    app.close()
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
