#!/usr/bin/env python
"""`config.reference`: the sum under a new exposure, and stopped and resumed
by hand.

A sphere lit from the side, the sum taken to `reference.max_samples`:

- a new `light.exposure` halfway carries the sum on, and the image is the
  one summed at the new exposure from the start: the sums are of the
  samples over the exposure, which the mean is multiplied by as it is shown;
- once done, a new exposure scales the image and leaves the sum;
- a new `shading.srgb_mode` carries it on too: how the mean is shown, not
  what is summed;
- `sim.stop_reference()` makes the mean so far the image, done, and the sum
  takes no more samples; `sim.resume_reference()` goes on from there, to the
  limit -- what the UI's stop and continue buttons ask;
- the error is a share of the image's brightest value: the same sum stops
  at the same sample, with the same error, at an exposure of 1 and of 0.2.

Opens a window, in the background.
"""

import os
import sys

import numpy

from kalast.app import App

failures: list[str] = []
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LIMIT = 256


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def main() -> int:
    app = App()
    app.config.open_in_background = True
    c = app.simulation.config
    c.image.width = c.image.height = 200
    c.axes.style = "off"
    c.wireframe.mode = 0
    c.shading.srgb_mode = "linear"
    c.light.exposure = 1.0
    c.shadows.method = "reference"
    c.reference.max_samples = LIMIT
    c.reference.error = 1e-9  # to the limit
    sim = app.simulation
    sim.load_mesh(path=os.path.join(ROOT, "res", "ico5.obj"))
    sim.sun.pos = [1e6, 3e5, 2e5]
    sim.camera.pos, sim.camera.dir, sim.camera.up = [0.0, 0.0, 4.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]

    def to_the_end() -> None:
        for _ in range(5000):
            if sim.reference_done:
                break
            app.step()

    def again() -> None:
        """The camera moved away and back: a new scene, the sum begun again."""
        sim.camera.pos = [0.0, 0.0, 5.0]
        app.step()
        app.step()
        sim.camera.pos = [0.0, 0.0, 4.0]
        app.step()

    for _ in range(53):
        app.step()
    before = sim.reference_samples
    c.light.exposure = 2.0
    app.step()
    check("a new exposure carries the sum on", sim.reference_samples > before, f"{before} samples, then {sim.reference_samples}")
    to_the_end()
    halfway = sim.reference_image().copy()
    check("done at the limit", sim.reference_done and sim.reference_samples == LIMIT, f"{sim.reference_samples}")

    again()
    check("a new scene begins the sum again", sim.reference_samples < 5, f"{sim.reference_samples}")
    to_the_end()
    whole = sim.reference_image().copy()
    d = float(numpy.abs(halfway - whole).max())
    check("1 then 2 halfway is the sum at 2", d <= 1e-5 * float(whole.max()), f"max |diff| {d:.2e}, brightest {whole.max():.3f}")

    c.light.exposure = 4.0
    app.step()
    app.step()
    scaled = sim.reference_image()
    check(
        "done, a new exposure scales the image and keeps the sum",
        numpy.allclose(scaled, 2.0 * whole, rtol=1e-5, atol=1e-7) and sim.reference_done and sim.reference_samples == LIMIT,
        f"ratio {scaled.sum() / whole.sum():.6f}, {sim.reference_samples} samples",
    )

    # `shading.srgb_mode`: how the mean is shown, not what is summed -- the
    # stored values in both.
    again()
    for _ in range(40):
        app.step()
    before = sim.reference_samples
    c.shading.srgb_mode = "screen"
    app.step()
    check("a new srgb_mode carries the sum on", sim.reference_samples > before, f"{before} samples, then {sim.reference_samples}")
    to_the_end()
    switched = sim.reference_image().copy()
    again()
    to_the_end()
    zero = sim.reference_image().copy()
    d = float(numpy.abs(switched - zero).max())
    check("1 then 0 halfway is the sum in 0", d <= 1e-5 * float(zero.max()), f"max |diff| {d:.2e}, brightest {zero.max():.3f}")
    c.shading.srgb_mode = "linear"

    again()
    for _ in range(40):
        app.step()
    sim.stop_reference()
    app.step()
    app.step()
    n = sim.reference_samples
    check(
        "stopped by hand: done, the mean so far the image",
        sim.reference_done and sim.reference_stopped and sim.reference_image() is not None and n < LIMIT,
        f"{n} samples",
    )
    for _ in range(10):
        app.step()
    check("stopped, no more samples", sim.reference_samples == n, f"{sim.reference_samples}")
    sim.resume_reference()
    for _ in range(10):
        app.step()
    check(
        "resumed, on from there",
        sim.reference_samples > n and not sim.reference_done and not sim.reference_stopped,
        f"{n} -> {sim.reference_samples}",
    )
    to_the_end()
    check("and on to the limit", sim.reference_samples == LIMIT and sim.reference_done and not sim.reference_stopped,
          f"{sim.reference_samples}")

    # Stopped by its error, at two exposures: before 14 October the error
    # was in the image's values, and a darker image stopped sooner.
    c.reference.error = 0.03
    ends = []
    for exposure in (1.0, 0.2):
        c.light.exposure = exposure
        again()
        to_the_end()
        ends.append((sim.reference_samples, sim.reference_error))
    check(
        "stopped by its error at the same sample, whatever the exposure",
        # The sums are of the samples over the exposure: to float32's last
        # places.
        ends[0][0] < LIMIT and ends[0][0] == ends[1][0] and abs(ends[0][1] - ends[1][1]) <= 1e-5 * ends[0][1],
        f"at exposure 1: {ends[0][0]} samples, error {ends[0][1]:.5f}; at 0.2: {ends[1][0]}, {ends[1][1]:.5f}",
    )
    app.close()
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
