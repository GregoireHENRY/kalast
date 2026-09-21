"""The thermophysical model, imported on first use rather than with `kalast`.

These were imported eagerly here, and `implicit` imports scipy at module
level -- so `import kalast` pulled in 54 MB of it whether or not anything
solved a heat equation. Same treatment as `kalast.plot`: PEP 562, so
`kalast.tpm.routine.something()` reads the same and costs nothing until it
is reached.

`heating` is listed although it was never imported here, because there is no
reason for it not to be reachable the same way as the rest.
"""

_SUBMODULES = (
    "core",
    "column",
    "properties",
    "emit",
    "routine",
    "radiance",
    #
    "nonuniform",
    "explicit",
    "implicit",
    "heating",
)

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
