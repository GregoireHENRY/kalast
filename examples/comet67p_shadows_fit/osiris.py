"""Reading a Rosetta OSIRIS NAC image of the archive's level-4 sets (I/F,
geometric distortion corrected): its pixels and the label's few values the
simulation needs; and fetching it from ESA's Planetary Science Archive, the
whole frame or only some of its lines."""

import os
import re
import time
import urllib.request

import numpy

# ESA's Planetary Science Archive: the same products as NASA's PDS Small
# Bodies Node, under upper-case names.
PSA = "https://archives.esac.esa.int/psa/ftp/INTERNATIONAL-ROSETTA-MISSION/OSINAC"
# Where the images are kept: set it to yours.
DATA = "/Users/gregoireh/data/rosetta/osiris_nac/str_refl"


def fetch(dataset, product, folder=DATA, rows=None):
    """The path to `product` of `dataset` in `folder`, downloaded first if it
    is not there: 38 MB a full frame. With `rows`, `(first, last)`, the
    label and those lines only -- what a patch of the surface needs of a
    frame, a few MB -- kept apart in `folder/rows/`, which `read` reads as
    the frame cut to them."""
    url = f"{PSA}/{dataset.upper()}/DATA/IMG/{product.upper()}"
    if rows is not None:
        return _fetch_rows(url, product, os.path.join(folder, "rows"), *rows)
    path = os.path.join(folder, product)
    if not os.path.exists(path):
        os.makedirs(folder, exist_ok=True)
        print(f"downloading {product}", flush=True)
        urllib.request.urlretrieve(url, path + ".part")
        os.replace(path + ".part", path)
    return path


def _get(url, first, last):
    """Bytes `first` to `last` of `url`: the archive answers byte ranges."""
    for attempt in range(6):
        try:
            request = urllib.request.Request(url, headers={"Range": f"bytes={first}-{last}"})
            with urllib.request.urlopen(request, timeout=60) as answer:
                data = answer.read()
            if answer.status == 206 and len(data) == last - first + 1:
                return data
            error = f"status {answer.status}, {len(data)} bytes"
        except OSError as e:
            error = e
        time.sleep(2 * 2**attempt)
    raise OSError(f"{url} bytes {first}-{last}: {error}")


def _fetch_rows(url, product, folder, first, last):
    path = os.path.join(folder, f"{product[:-4]}_{first}-{last}.npz")
    if not os.path.exists(path):
        os.makedirs(folder, exist_ok=True)
        head = _get(url, 0, 32767).decode("latin-1")
        record, start = _key(head, "RECORD_BYTES", int), _key(head, r"\^IMAGE", int)
        image = _image_object(head)
        samples = _key(image, "LINE_SAMPLES", int)
        dtype = {"PC_REAL": "<f4", "IEEE_REAL": ">f4"}[_key(image, "SAMPLE_TYPE")]
        offset = (start - 1) * record
        data = _get(url, offset + first * samples * 4, offset + last * samples * 4 - 1)
        pixels = numpy.frombuffer(data, dtype).reshape(last - first, samples)
        numpy.savez(path + ".part.npz", label=numpy.array(head), first=first, image=pixels.astype(numpy.float32))
        os.replace(path + ".part.npz", path)
    return path


def _key(text, name, cast=str):
    found = re.search(r"^\s*" + name + r"\s*=\s*\"?([^\"\r\n]+)\"?", text, re.M)
    return cast(found.group(1).strip().split()[0]) if found else None


def _image_object(head):
    image = head[re.search(r"OBJECT\s*=\s*IMAGE\b", head).start():]
    return image[: image.index("END_OBJECT")]


def read(path):
    """`(pixels, label)`: the image in I/F as stored, its first line along
    the NAC's +X; the label's mid-exposure time and the window of the CCD it
    was read from, and `first`, the frame's first line in `pixels` -- not 0
    for lines fetched alone."""
    if path.endswith(".npz"):
        stored = numpy.load(path)
        head, first, pixels = str(stored["label"]), int(stored["first"]), stored["image"]
    else:
        raw = open(path, "rb").read()
        head = raw[:120000].decode("latin-1")
        record, start = _key(head, "RECORD_BYTES", int), _key(head, r"\^IMAGE", int)
        image = _image_object(head)
        lines, samples = _key(image, "LINES", int), _key(image, "LINE_SAMPLES", int)
        dtype = {"PC_REAL": "<f4", "IEEE_REAL": ">f4"}[_key(image, "SAMPLE_TYPE")]
        pixels = numpy.frombuffer(raw, dtype=dtype, count=lines * samples, offset=(start - 1) * record).reshape(lines, samples)
        first = 0
    image = _image_object(head)
    label = {
        "start": _key(head, "START_TIME"),
        "exposure": _key(head, "EXPOSURE_DURATION", float),
        "lines": _key(image, "LINES", int),
        "samples": _key(image, "LINE_SAMPLES", int),
        # The window of the CCD read out, in its pixels: 2048 of them across
        # in a full frame.
        "x": (_key(head, "ROSETTA:X_START", int), _key(head, "ROSETTA:X_END", int)),
        "y": (_key(head, "ROSETTA:Y_START", int), _key(head, "ROSETTA:Y_END", int)),
        "phase": _key(head, "PHASE_ANGLE", float),
        "distance": _key(head, "TARGET_CENTER_DISTANCE", float),
        "first": first,
    }
    return pixels.astype(numpy.float32), label
