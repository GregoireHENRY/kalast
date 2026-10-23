#!/usr/bin/env python
"""A telescope's frame made on the GPU from the rendered light, against the
CPU's of the same plan.

`sim.expose(...)` asks for a frame of the frame a step draws: its target's
light the frame's own photometry, summed and kept on the GPU, the frame made
there (`shaders/telescope.wgsl`) from the plan `kalast.telescope.expose`
makes its own from (`telescope::Plan`):

1. Without noise, the two frames are the same light, pixel by pixel: the
   GPU's fixed point and f32 against the CPU's f64.
2. With every effect, the GPU's sky is the expected one, its mean and its
   scatter, and the target reduced against comparison stars has its
   rendered magnitude within its error.
3. Asked without photometry, an exposure is dropped, not left pending.

A background window; `res/ico5.obj`, no data paths, no network.
"""

import os
import sys
import tempfile

import numpy

import kalast.telescope as telescope
from kalast.app import App
from kalast.scattering import Hapke

failures: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def clipped(d):
    d = numpy.asarray(d, dtype=float).ravel()
    for _ in range(8):
        m, s = d.mean(), d.std()
        d = d[abs(d - m) < 3 * s]
    return d.mean(), d.std()


def main() -> int:
    app = App()
    app.config.open_in_background = True
    sim = app.simulation
    sim.config.huds = []
    sim.config.photometry.enabled = True
    sim.load_mesh(path="res/ico5.obj", smooth=True)
    sim.bodies[0].scattering = Hapke(w=0.126)
    sim.bodies[0].mat[:3, :3] = numpy.diag([0.4, 0.4, 0.3])  # km
    sim.camera.projection.set_orthographic()
    sim.camera.projection.side = 0.6
    sim.camera.pos = [0.0, -30.0, 0.0]
    sim.camera.dir = [0.0, 1.0, 0.0]
    sim.camera.up = [0.0, 0.0, 1.0]
    sim.sun.pos = (numpy.array([0.3, -1.0, 0.1]) * 1.5e8).tolist()

    camera = telescope.Telescope(width=512, height=512)
    band, sky = telescope.Band("Rc"), telescope.Sky()
    ra0, dec0 = 124.6, 15.2
    rng = numpy.random.default_rng(2)
    stars = []
    while len(stars) < 24:
        dx, dy = rng.uniform(-140.0, 140.0, 2)
        if abs(dx) < 25 and abs(dy) < 25:
            continue
        stars.append(telescope.Source(ra0 + dx / 3600 / numpy.cos(numpy.radians(dec0)), dec0 + dy / 3600,
                                      float(rng.uniform(13.0, 15.0)), comparison=True))
    target, rate, r, delta = (ra0, dec0), (0.0, 0.02), 1.05, 2.2e7
    quiet = telescope.Effects(sky=False, dark=False, photon_noise=False, read_noise=False, bias=False,
                              scintillation=False, saturation=False)

    with tempfile.TemporaryDirectory(prefix="kalast-gpu-frames-") as scratch:
        def made(path, exposure, effects):
            sim.expose(path, exposure, target, rate, r, delta, stars, camera, band, sky, effects, "a test")
            app.step()
            for _ in range(30):
                app.step()
                if sim.exposures_pending == 0:
                    break
            done = sim.exposed()
            return (telescope.read(path), done[-1]) if done else (None, None)

        # 1. Without noise, the CPU's frame of the same plan.
        exposure = telescope.Exposure("2022-11-18T03:30:00.000", 90.0, ra0, dec0, airmass=1.3, seed=5)
        gpu, (_, _, flux, mag) = made(os.path.join(scratch, "quiet.fits"), exposure, quiet)
        check("a frame is written, its target's light the rendered one", gpu is not None and flux > 0, f"Rc {mag:.4f}")
        cpu = telescope.expose(camera, band, sky, quiet, exposure, telescope.Source(ra0, dec0, mag, rate), stars, "a test")
        g, c = gpu.data.astype(float), numpy.floor(cpu.data.astype(float) + 0.5)
        check("the same light, pixel by pixel", float(abs(g - c).max()) <= 1.0 + 1e-3 * float(c.max()),
              f"largest difference {abs(g - c).max():.1f} ADU of {c.max():.0f}")
        check("the same light in all", abs(g.sum() / c.sum() - 1) < 2e-4, f"{g.sum() / c.sum() - 1:+.1e}")

        # 2. Every effect: the sky, and the target's magnitude reduced.
        sky_e = camera.zero_rate(band) * 10 ** (-0.4 * sky.brightness) * camera.pixel_scale ** 2 * 90.0 + camera.dark * 90.0
        expected = (sky_e / camera.gain + camera.bias, (sky_e + camera.read_noise ** 2) ** 0.5 / camera.gain)
        pulls = []
        for seed in range(8):
            e = telescope.Exposure("2022-11-18T03:30:00.000", 90.0, ra0, dec0, airmass=1.3, seed=100 + seed)
            frame, (_, _, _, mag) = made(os.path.join(scratch, f"night_{seed}.fits"), e, telescope.Effects())
            if seed == 0:
                m, s = clipped(frame.data)
                check("the sky as expected, its mean and its scatter", abs(m - expected[0]) < 1.0 and abs(s / expected[1] - 1) < 0.03,
                      f"{m:.1f} +- {s:.2f} ADU against {expected[0]:.1f} +- {expected[1]:.2f}")
            red = telescope.reduce(frame, stars, telescope.Aperture(7.0, 13.0, 23.0))
            if red is not None:
                pulls.append((red.mag - mag) / red.error)
        mean = float(numpy.mean(pulls)) if pulls else float("nan")
        check("reduced, the target has its rendered magnitude", len(pulls) == 8 and abs(mean) < 1.0, f"mean pull {mean:+.2f} over {len(pulls)}")

        # 3. Without photometry, dropped.
        sim.config.photometry.enabled = False
        sim.expose(os.path.join(scratch, "none.fits"), exposure, target, rate, r, delta, stars)
        for _ in range(4):
            app.step()
        check("asked without photometry, dropped", sim.exposures_pending == 0 and not os.path.exists(os.path.join(scratch, "none.fits")))

    app.close()
    app.step()
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
