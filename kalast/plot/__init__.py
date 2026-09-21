"""Plotting helpers, imported on first use rather than with `kalast`.

`from kalast.plot import tool, smap, cbar, style, util` used to sit here, and
it meant that `import kalast` -- in a script that only renders a mesh --
loaded matplotlib, scipy and pyarrow. In a release bundle those come to 174
of the 283 MB of site-packages, for code the script never calls.

PEP 562 lets a module supply `__getattr__`, so `kalast.plot.cbar.Params()`
still works exactly as written; the submodule is imported the first time it
is reached and cached in `globals()` after that. The cost is that a missing
matplotlib now surfaces at that first access rather than at `import kalast`,
which is the better place for it anyway.
"""

_SUBMODULES = ("tool", "smap", "cbar", "style", "util")

__all__ = list(_SUBMODULES)


def __getattr__(name: str):
    if name in _SUBMODULES:
        import importlib

        module = importlib.import_module(f"{__name__}.{name}")
        globals()[name] = module  # so this runs once
        return module
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


def __dir__():
    return sorted(set(globals()) | set(_SUBMODULES))
