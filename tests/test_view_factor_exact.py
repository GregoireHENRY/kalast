#!/usr/bin/env python
"""The exact view factor between two triangles, from Python.

`kalast.mesh.view_factor_exact` is the double contour integral of Sparrow
(1963) as Lagerros (1998, appendix A.1) writes it for triangles. The Rust tests
(`tpm::view_factor`) hold it against the closed forms to 1e-9 in double
precision; here the binding is checked with the same two cases, given as the
float32 arrays a script passes, and beside `view_factor_triangles`, the
subdivision it is the reference for.

Pure CPU: no GPU, no window.
"""

import sys

import numpy

import kalast

failures: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def square(origin, u, v):
    """Two triangles covering a rectangle, wound so the normal is `u x v`."""
    o, u, v = (numpy.asarray(x, dtype=numpy.float32) for x in (origin, u, v))
    return [
        numpy.array([o, o + u, o + u + v], dtype=numpy.float32),
        numpy.array([o, o + u + v, o + v], dtype=numpy.float32),
    ]


def area(t):
    return 0.5 * numpy.linalg.norm(numpy.cross(t[1] - t[0], t[2] - t[0]))


def between(a, b, factor):
    total = sum(area(t) for t in a)
    return sum(area(s) * sum(factor(s, t) for t in b) for s in a) / total


def opposed(c):
    """Two unit squares directly opposed, `c` apart."""
    x = 1.0 / c
    r = numpy.sqrt(1.0 + x * x)
    return 2.0 / (numpy.pi * x * x) * (
        numpy.log((1.0 + x * x) / numpy.sqrt(1.0 + 2.0 * x * x))
        + 2.0 * x * r * numpy.arctan(x / r)
        - 2.0 * x * numpy.arctan(x)
    )


HINGED = 0.200043776075  # two unit squares at a right angle sharing a side

exact = kalast.mesh.view_factor_exact

# 1. the closed forms, in the float32 a script passes
for c in (1.0, 0.1, 5.0):
    low = square([0, 0, 0], [1, 0, 0], [0, 1, 0])
    high = square([0, 0, c], [0, 1, 0], [1, 0, 0])
    got, want = between(low, high, exact), opposed(c)
    check(f"opposed squares {c} apart", abs(got / want - 1.0) < 2e-6, f"{got:.8f} against {want:.8f}")

floor = square([0, 0, 0], [1, 0, 0], [0, 1, 0])
wall = square([0, 0, 0], [0, 0, 1], [1, 0, 0])
got = between(floor, wall, exact)
check("squares sharing a side", abs(got / HINGED - 1.0) < 2e-6, f"{got:.8f} against {HINGED:.8f}")

# 2. reciprocity, and nothing for what is behind or in the same plane
a, b = floor[0], wall[1]
check("reciprocity", abs(area(a) * exact(a, b) - area(b) * exact(b, a)) < 1e-7)
check("in one plane", exact(floor[0], floor[1]) == 0.0)
behind = numpy.array([[0, 0, -1], [0, 1, -1], [1, 0, -1]], dtype=numpy.float32)
check("behind", exact(floor[0], behind) == 0.0)

# 3. the subdivision it is the reference for: 3.7 % high on the pair that shares a side, at its usual setting
sub = between(floor, wall, lambda s, t: kalast.mesh.view_factor_triangles(s, t, 6.0, 4))
check("the subdivision is 3 to 4.5 % high on a shared side", 0.03 < sub / got - 1.0 < 0.045, f"{sub:.6f} against {got:.6f}")

# 4. a triangle is three vertices
try:
    exact(numpy.zeros((4, 3), dtype=numpy.float32), floor[0])
    check("a (4, 3) array is refused", False)
except ValueError:
    check("a (4, 3) array is refused", True)

if failures:
    print(f"\n{len(failures)} failed: {failures}")
    sys.exit(1)
print("\nall passed")
