#!/usr/bin/env python
"""Write matplotlib's colormaps into kalast: `res/colormaps.bin`, compiled into
the renderer, and `res/LICENSE-colormaps`, the notices they come with.

    python tools/gen_colormaps.py        # after a matplotlib release adds one
    python tests/test_colormaps.py       # checks they are current

matplotlib is needed here and nowhere else: kalast carries the tables, so the
panel lists them and a script names them without it, in Rust alone too.

Each colormap is sampled at the renderer's 256 entries, a byte a channel --
exact for the listed ones, whose steps a colormap with fewer colours (tab10's
ten) keeps as steps. In matplotlib's registration order, the perceptually
uniform first. A name whose table is another's -- `grey` of `gray`, `Greys`
of `Grays` -- is kept as an alias: taken by name, not listed twice.

The file: `KALASTCM`, a little-endian u16 count, then per colormap a flags
byte (1 an alias), the name's length and its UTF-8, and 256 RGB triples.
"""

import pathlib
import struct
import sys

import matplotlib
import numpy

ROOT = pathlib.Path(__file__).resolve().parent.parent
TABLES = ROOT / "res" / "colormaps.bin"
NOTICES = ROOT / "res" / "LICENSE-colormaps"
MAGIC = b"KALASTCM"
SIZE = 256


def tables() -> list[tuple[str, bool, bytes]]:
    """(name, alias, 768 bytes) in matplotlib's order, `_r` left out: kalast
    reverses any colormap by that suffix itself. Of the names sharing a
    table, the one listed is the first not `gist_` -- `gray` rather than
    `gist_gray`, registered before it -- the others its aliases."""
    sampled = []
    for name in matplotlib.colormaps:
        if name.endswith("_r"):
            continue
        rgb = matplotlib.colormaps[name](numpy.linspace(0.0, 1.0, SIZE))[:, :3]
        sampled.append((name, numpy.round(rgb * 255.0).astype(numpy.uint8).tobytes()))
    listed = {}
    for name, data in sampled:
        if data not in listed or (listed[data].startswith("gist_") and not name.startswith("gist_")):
            listed[data] = name
    return [(name, listed[data] != name, data) for name, data in sampled]


def encode(entries) -> bytes:
    out = bytearray(MAGIC + struct.pack("<H", len(entries)))
    for name, alias, data in entries:
        raw = name.encode()
        out += bytes([1 if alias else 0, len(raw)]) + raw + data
    return bytes(out)


def notices() -> str:
    """matplotlib's license, and the sections of its bundled components the
    colormaps come from, as its own LICENSE gives them."""
    dist = next(pathlib.Path(matplotlib.__file__).parent.parent.glob(f"matplotlib-{matplotlib.__version__}.dist-info"))
    text = (dist / "LICENSE").read_text()
    lines = text.splitlines()
    starts = [i for i, l in enumerate(lines) if l.startswith("Name: ")]
    head = "\n".join(lines[: starts[0]]).rstrip() if starts else text
    sections = []
    for k, i in enumerate(starts):
        end = starts[k + 1] if k + 1 < len(starts) else len(lines)
        block = lines[i:end]
        # A component whose files are matplotlib's colormap modules, `_cm.py`.
        files = next((l for l in block if l.startswith("Files:")), "")
        if "/_cm" in files:
            sections.append("\n".join(block).rstrip())
    return (
        f"kalast's colormaps, in res/colormaps.bin, are sampled from matplotlib\n"
        f"{matplotlib.__version__}'s by tools/gen_colormaps.py. They come under matplotlib's\n"
        f"license and, where matplotlib takes them from elsewhere, the notices below,\n"
        f"as matplotlib's own LICENSE gives them.\n\n"
        + head
        + "\n\n\n"
        + "\n\n\n".join(sections)
        + "\n"
    )


def main() -> int:
    entries = tables()
    TABLES.write_bytes(encode(entries))
    NOTICES.write_text(notices())
    listed = sum(1 for _, alias, _ in entries if not alias)
    print(f"wrote {TABLES.relative_to(ROOT)}: {len(entries)} colormaps, {listed} listed, from matplotlib {matplotlib.__version__}")
    print(f"wrote {NOTICES.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
