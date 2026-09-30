#!/usr/bin/env python
"""matplotlib's colormaps in kalast: `res/colormaps.bin` and
`res/LICENSE-colormaps` as `tools/gen_colormaps.py` writes them from the
matplotlib installed, and the tables kalast gives back matplotlib's.

Regenerate after a matplotlib release adds a colormap:

    python tools/gen_colormaps.py

Skipped without matplotlib, which kalast itself never needs.
"""

import importlib.util
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent

try:
    import matplotlib  # noqa: F401
    import numpy
except ImportError:
    matplotlib = None


def generator():
    spec = importlib.util.spec_from_file_location("gen_colormaps", ROOT / "tools" / "gen_colormaps.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_the_tables_are_regenerated():
    gen = generator()
    assert (ROOT / "res" / "colormaps.bin").read_bytes() == gen.encode(gen.tables()), (
        "res/colormaps.bin is stale: run tools/gen_colormaps.py"
    )
    assert (ROOT / "res" / "LICENSE-colormaps").read_text() == gen.notices(), (
        "res/LICENSE-colormaps is stale: run tools/gen_colormaps.py"
    )


def test_kalast_gives_back_matplotlibs():
    import matplotlib
    from kalast.app import config

    names = config.colormap_names()
    assert "viridis" in names and "tab10" in names and "grey" not in names, names
    for name in ["viridis", "RdBu", "tab10", "grey", "twilight"]:
        ours = numpy.asarray(config.colormap(name))
        theirs = matplotlib.colormaps[name](numpy.linspace(0.0, 1.0, len(ours)))[:, :3]
        assert numpy.abs(ours - theirs).max() <= 0.5 / 255 + 1e-6, name
    # `_r` is the table the other way round -- matplotlib's own `twilight_r`,
    # of 510 colours, falls a step apart at 256.
    assert numpy.array_equal(numpy.asarray(config.colormap("twilight_r")), numpy.asarray(config.colormap("twilight"))[::-1])


if __name__ == "__main__":
    if matplotlib is None:
        print("skip: matplotlib is not installed")
        raise SystemExit(0)
    failures = 0
    for name, fn in sorted(globals().items()):
        if name.startswith("test_") and callable(fn):
            try:
                fn()
                print(f"ok   {name}")
            except Exception as e:
                failures += 1
                print(f"FAIL {name}\n     {type(e).__name__}: {e}")
    raise SystemExit(failures)
