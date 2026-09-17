"""Decode the PNGs the frame exporter writes, for the tests that look at pixels.

Eight-bit RGB/RGBA, non-interlaced, is all the exporter produces, and that is
all this reads -- so the tests add no dependency on an image library that
kalast itself does not have.
"""

import struct
import zlib

import numpy


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
