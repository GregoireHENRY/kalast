#!/usr/bin/env python
"""A telescope's frames, through Python: made, written, read, reduced.

`kalast.telescope` makes the frames a ground-based night records and
measures them as a night's reduction does. The engine's own tests hold each
piece (`cargo test telescope`); this one holds them together, as a script
uses them:

1. A frame written as FITS comes back the same: its counts, its header, its
   WCS.
2. Reduced against comparison stars of known magnitude, the target's
   magnitude is its own within its errors, at airmass 1 and 2 alike: the
   stars take the extinction out.
3. With the effects off, a frame is the light alone: no sky, no bias, no
   noise.

No network (the stars are made here, not asked of Gaia), no window.
"""

import os
import sys
import tempfile

import numpy

import kalast.telescope as telescope

failures: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def main() -> int:
    camera = telescope.Telescope(width=512, height=512)
    band, sky = telescope.Band("Rc"), telescope.Sky()
    ra0, dec0 = 124.6, 15.2
    rng = numpy.random.default_rng(1)
    # Twenty stars of 13 to 15.5 mag across the frame, none near its centre.
    stars = []
    while len(stars) < 20:
        dx, dy = rng.uniform(-130.0, 130.0, 2)  # arcsec
        if abs(dx) < 25 and abs(dy) < 25:
            continue
        stars.append(telescope.Source(ra0 + dx / 3600 / numpy.cos(numpy.radians(dec0)), dec0 + dy / 3600,
                                      float(rng.uniform(13.0, 15.5)), comparison=True))
    target = telescope.Source(ra0, dec0, 15.0)
    aperture = telescope.Aperture(7.0, 13.0, 23.0)

    # 1. Written and read back.
    exposure = telescope.Exposure("2022-11-18T03:30:00.000", 60.0, ra0, dec0, airmass=1.0, seed=3)
    frame = telescope.expose(camera, band, sky, telescope.Effects(), exposure, target, stars, "test")
    with tempfile.TemporaryDirectory(prefix="kalast-telescope-") as scratch:
        path = os.path.join(scratch, "frame.fits")
        frame.write(path)
        back = telescope.read(path)
    # Halves away from zero, as the camera's file has them (numpy's round
    # takes them to the even count).
    check("the counts come back, rounded to whole ADU", numpy.array_equal(back.data, numpy.floor(numpy.clip(frame.data, 0, 65535) + 0.5)))
    h = back.header
    check("the header says what the frame is",
          h["FILTER"] == "Rc" and h["EXPTIME"] == 60.0 and h["CTYPE1"] == "RA---TAN" and abs(h["TARGRA"] - ra0) < 1e-12,
          f"{h['FILTER']}, {h['EXPTIME']} s, {h['CTYPE1']}")

    # 2. Reduced: the target's own magnitude, the extinction taken out.
    for airmass in (1.0, 2.0):
        pulls = []
        for seed in range(12):
            e = telescope.Exposure("2022-11-18T03:30:00.000", 60.0, ra0, dec0, airmass=airmass, seed=100 + seed)
            f = telescope.expose(camera, band, sky, telescope.Effects(trail=False), e, target, stars, "test")
            red = telescope.reduce(f, stars, aperture)
            if red is not None:
                pulls.append((red.mag - 15.0) / red.error)
        mean = float(numpy.mean(pulls)) if pulls else float("nan")
        check(f"airmass {airmass}: the target's magnitude, within its errors", len(pulls) == 12 and abs(mean) < 1.0,
              f"mean pull {mean:+.2f} over {len(pulls)} frames")

    # 3. The light alone.
    quiet = telescope.Effects(sky=False, stars=False, extinction=False, scintillation=False, photon_noise=False,
                              dark=False, read_noise=False, bias=False)
    f = telescope.expose(camera, band, sky, quiet, exposure, target, stars, "test")
    total = float(f.data.sum()) * camera.gain
    expected = camera.zero_rate(band) * 10 ** (-0.4 * 15.0) * 60.0
    check("with the effects off, the target's electrons alone", abs(total / expected - 1) < 2e-3, f"{total / expected - 1:+.1e}")
    check("and no sky under them", float(numpy.median(f.data)) == 0.0)

    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
