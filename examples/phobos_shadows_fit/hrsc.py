"""Reading a Mars Express HRSC SRC image: its pixels and the label's few
values the simulation needs; and the SRC's response, for kalast's images."""

import re

import numpy


def read(path):
    """`(pixels, label)`: the image as floats, lines from the top, and the
    time and the window of the CCD it was read from."""
    raw = open(path, "rb").read()
    head = raw[:20000].decode("latin-1")

    def key(name, cast=str):
        found = re.search(r"^\s*" + name + r"\s*=\s*\"?([^\"\r\n]+)\"?", head, re.M)
        return cast(found.group(1).strip()) if found else None

    record, start = key("RECORD_BYTES", int), key(r"\^IMAGE", int)
    lines, samples = key("LINES", int), key("LINE_SAMPLES", int)
    pixels = numpy.frombuffer(raw, dtype=">i2", count=lines * samples, offset=(start - 1) * record)
    label = {
        "time": key("IMAGE_TIME").rstrip("Z"),
        "first_sample": key("SAMPLE_FIRST_PIXEL", int),
        "first_line": key("LINE_FIRST_PIXEL", int),
        "lines": lines,
        "samples": samples,
    }
    return pixels.reshape(lines, samples).astype(numpy.float32), label


# The SRC's response, fitted to these four images through kalast's
# (notes/2026-10-12_phobos_hrsc_shadows): Phobos's drift along the samples
# during the exposure -- SPICE's drift times EXPOSURE_DURATION, in pixels --
# a Gaussian core of 2.2 pixels, and two halos, 1.2 % of the light within
# about 50 pixels and 28 % over about 430: what lights the images' sky and
# shadows.
SMEAR = {"h8870": 1.86, "h8951": 4.90, "h8963": 2.34, "hc069": 6.98}
CORE = 2.19
HALOS = ((0.012, 53.0), (0.282, 434.0))


def response(img, name):
    """`img`, one of kalast's, as the SRC would record it: smeared, blurred,
    its light spread. Nothing beyond the frame."""
    h, w = img.shape
    y, x = numpy.meshgrid(numpy.fft.fftfreq(2 * h) * 2 * h, numpy.fft.fftfreq(2 * w) * 2 * w, indexing="ij")
    r = numpy.hypot(x, y)

    def spectrum(k):
        return numpy.fft.rfft2(k / k.sum())

    k = (1 - sum(f for f, _ in HALOS)) * spectrum(numpy.exp(-0.5 * (r / CORE) ** 2))
    for f, rho in HALOS:
        k = k + f * spectrum(numpy.exp(-r / rho))
    # The smear, a box along the samples, its ends partial.
    k = k * spectrum(numpy.clip(SMEAR[name[:5]] / 2 + 0.5 - numpy.abs(x), 0, 1) * (y == 0))
    return numpy.fft.irfft2(numpy.fft.rfft2(numpy.pad(img, ((0, h), (0, w)))) * k, s=(2 * h, 2 * w))[:h, :w]
