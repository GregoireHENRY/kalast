"""Reading a Mars Express HRSC SRC image: its pixels and the label's few
values the simulation needs."""

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
