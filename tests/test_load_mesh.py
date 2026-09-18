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
    # Flat and smooth are the same geometry: eight shared corners either
    # way. They used to be different vertex arrays, 36 rows against 8.
    rows = [len(sim.bodies[i].mesh.positions) for i in range(2)]
    check("flat keeps the file's shared corners", rows[0] == 8, f"{rows[0]} rows")
    check("and so does smooth", rows[1] == 8, f"{rows[1]} rows")

    # What differs is the shading: a colour per facet against one per vertex.
    colors = [sim.bodies[i].mesh.colors.shape for i in range(2)]
    check("flat is coloured per facet", colors[0] == (12, 3), f"{colors[0]}")
    check("smooth per vertex", colors[1] == (8, 3), f"{colors[1]}")

    # And a mesh loaded flat can be smoothed: there is nothing to restore, so
    # nothing to be missing. It used to refuse, having kept no shared copy.
    flat_mesh = sim.bodies[0].mesh
    flat_mesh.smoothen()
    check(
        "a mesh loaded flat can be smoothed",
        not flat_mesh.is_flat() and flat_mesh.colors.shape == (8, 3),
        f"is_flat {flat_mesh.is_flat()}, colors {flat_mesh.colors.shape}",
    )
    flat_mesh.flatten()
    check("and flattened again", flat_mesh.is_flat() and flat_mesh.colors.shape == (12, 3))

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
