#!/usr/bin/env python
"""`sim.load_mesh` loads flat unless asked not to.

`flatten` was the argument until 17 September, default off, so every script
in the repository said `flatten=True` -- a default nobody wanted, spelled out
27 times. Flat is what per-facet data, the wireframe overlay and the facet
index map need; the shared-vertex mesh is the exception, and is now the thing
you ask for: `smooth=True`. The old spelling still works for a release,
inverted, with a `DeprecationWarning`, so an unconverted script warns rather
than silently rendering the other kind of mesh.

Constructs an `App` for its simulation, so it wants a GPU adapter; no window.
"""

import sys
import warnings

import numpy

import kalast.mesh
from kalast.app import App

CUBE = "res/cube.obj"

failures: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def main() -> int:
    sim = App().simulation
    sim.load_mesh(path=CUBE, mat=numpy.eye(4))
    sim.load_mesh(path=CUBE, smooth=True)
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        sim.load_mesh(path=CUBE, flatten=True)
        sim.load_mesh(path=CUBE, flatten=False)
    flat = [sim.bodies[i].mesh.is_flat() for i in range(4)]

    check("the default is flat", flat[0] is True, f"is_flat {flat[0]}")
    check("smooth=True keeps the shared vertices", flat[1] is False, f"is_flat {flat[1]}")
    check("flatten=True still means flat", flat[2] is True, f"is_flat {flat[2]}")
    check("flatten=False still means smooth", flat[3] is False, f"is_flat {flat[3]}")
    deprecations = [w for w in caught if issubclass(w.category, DeprecationWarning)]
    check(
        "the old spelling warns, once per call",
        len(deprecations) == 2 and all("smooth=True" in str(w.message) for w in deprecations),
        f"{len(deprecations)} DeprecationWarning(s)",
    )
    # A flat cube: 12 facets, three rows each; shared: the file's 8 corners.
    rows = [len(sim.bodies[i].mesh.vertices) for i in range(2)]
    check("flat is three rows per facet", rows[0] == 36, f"{rows[0]} rows")
    check("smooth is the file's corners", rows[1] == 8, f"{rows[1]} rows")

    # Flat is built as flat: nothing is kept to smoothen back to, and asking
    # says so rather than silently doing nothing.
    flat_mesh = sim.bodies[0].mesh
    kept = len(flat_mesh._vertices_before_flatten)
    check("a mesh loaded flat keeps no shared copy", kept == 0, f"{kept} shared vertices kept")
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        flat_mesh.smoothen()
    check(
        "smoothen() on it warns and leaves it flat",
        flat_mesh.is_flat() and any("smooth=True" in str(w.message) for w in caught),
        f"is_flat {flat_mesh.is_flat()}, {len(caught)} warning(s)",
    )
    # Explicit round trips on a Mesh object keep working.
    m = kalast.mesh.Mesh(CUBE)
    m.flatten()
    m.smoothen()
    check("an explicit flatten still finds its way back", not m.is_flat() and len(m.vertices) == 8)

    if failures:
        print(f"\n{len(failures)} failure(s)")
        return 1
    print("\nall ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
