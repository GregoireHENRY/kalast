"""Reading a Rosetta OSIRIS NAC image of the archive's level-4 sets (I/F,
geometric distortion corrected): its pixels and the label's few values the
simulation needs; and fetching it from ESA's Planetary Science Archive."""

import os
import re
import urllib.request

import numpy

# ESA's Planetary Science Archive: the same products as NASA's PDS Small
# Bodies Node, under upper-case names.
PSA = "https://archives.esac.esa.int/psa/ftp/INTERNATIONAL-ROSETTA-MISSION/OSINAC"
# Where the images are kept: set it to yours.
DATA = "/Users/gregoireh/data/rosetta/osiris_nac/str_refl"


def fetch(dataset, product, folder=DATA):
    """The path to `product` of `dataset` in `folder`, downloaded first if it
    is not there: 38 MB a full frame."""
    path = os.path.join(folder, product)
    if not os.path.exists(path):
        os.makedirs(folder, exist_ok=True)
        print(f"downloading {product}", flush=True)
        urllib.request.urlretrieve(f"{PSA}/{dataset.upper()}/DATA/IMG/{product.upper()}", path + ".part")
        os.replace(path + ".part", path)
    return path


def read(path):
    """`(pixels, label)`: the image in I/F as stored, its first line along
    the NAC's +X; the label's mid-exposure time and the window of the CCD it
    was read from."""
    raw = open(path, "rb").read()
    head = raw[:120000].decode("latin-1")

    def key(name, text=head, cast=str):
        found = re.search(r"^\s*" + name + r"\s*=\s*\"?([^\"\r\n]+)\"?", text, re.M)
        return cast(found.group(1).strip().split()[0]) if found else None

    record, start = key("RECORD_BYTES", cast=int), key(r"\^IMAGE", cast=int)
    image = head[re.search(r"OBJECT\s*=\s*IMAGE\b", head).start():]
    image = image[: image.index("END_OBJECT")]
    lines, samples = key("LINES", image, int), key("LINE_SAMPLES", image, int)
    dtype = {"PC_REAL": "<f4", "IEEE_REAL": ">f4"}[key("SAMPLE_TYPE", image)]
    pixels = numpy.frombuffer(raw, dtype=dtype, count=lines * samples, offset=(start - 1) * record)
    label = {
        "start": key("START_TIME"),
        "exposure": key("EXPOSURE_DURATION", cast=float),
        "lines": lines,
        "samples": samples,
        # The window of the CCD read out, in its pixels: 2048 of them across
        # in a full frame.
        "x": (key("ROSETTA:X_START", cast=int), key("ROSETTA:X_END", cast=int)),
        "y": (key("ROSETTA:Y_START", cast=int), key("ROSETTA:Y_END", cast=int)),
        "phase": key("PHASE_ANGLE", cast=float),
        "distance": key("TARGET_CENTER_DISTANCE", cast=float),
    }
    return pixels.reshape(lines, samples).astype(numpy.float32), label
