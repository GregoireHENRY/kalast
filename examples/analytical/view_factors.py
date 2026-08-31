#!/usr/bin/env python
"""Validate the view-factor kernel against closed-form configurations.

Self-heating, mutual heating and sub-facet roughness all reduce to view-factor
bookkeeping, so this is the same kind of check `sinusoidal.py` is for
conduction: two geometries whose answer is known exactly, used to measure what
the numerical form actually delivers.

Two configurations from the standard catalogue, both with unit squares:

- **parallel coaxial squares**, side 1, separation c

      X = Y = 1/c
      F = (2 / (pi X Y)) * { ln sqrt((1+X^2)(1+Y^2)/(1+X^2+Y^2))
                             + X sqrt(1+Y^2) atan(X/sqrt(1+Y^2))
                             + Y sqrt(1+X^2) atan(Y/sqrt(1+X^2))
                             - X atan X - Y atan Y }

- **perpendicular squares sharing an edge**, H = W = 1, whose closed form is
  the longer expression in `perpendicular_exact` below.

Each square is two triangles, so a square-to-square factor is

    F(A->B) = (1/A_A) sum_i sum_j  A_i F(tri_i -> tri_j)

which also exercises the area weighting the mesh code will use.

Why this matters here
---------------------

`mesh::view_factor_facets` uses the point-to-point form with a guard that
returns **zero** when the separation drops below sqrt(area). Adjacent facets
sit exactly at that threshold, so the guard fires on the neighbours that
dominate self-heating inside a concavity -- it does not merely approximate
them, it deletes them. `view_factor_triangles` subdivides instead. The third
section below measures the difference on the configuration where it bites.
"""

import numpy

import kalast

RATIO = 6.0      # subdivide while separation < RATIO * facet size
MAX_LEVEL = 4    # recursion bound


def square(origin, u, v):
    """Two triangles covering the square at `origin` spanned by `u`, `v`.

    Wound so the normal is `u x v`; the caller picks the direction by the
    order of the two spanning vectors, which is how each square below is made
    to face the other one.
    """
    o, u, v = (numpy.asarray(x, dtype=numpy.float32) for x in (origin, u, v))
    return [
        numpy.array([o, o + u, o + u + v], dtype=numpy.float32),
        numpy.array([o, o + u + v, o + v], dtype=numpy.float32),
    ]


def tri_area(t):
    return 0.5 * numpy.linalg.norm(numpy.cross(t[1] - t[0], t[2] - t[0]))


def view_factor(patch_a, patch_b, ratio=RATIO, max_level=MAX_LEVEL):
    """`F(A->B)` between two patches, each a list of triangles."""
    area_a = sum(tri_area(t) for t in patch_a)
    total = 0.0
    for ta in patch_a:
        for tb in patch_b:
            total += tri_area(ta) * kalast.mesh.view_factor_triangles(
                ta, tb, ratio, max_level
            )
    return total / area_a


# --- closed forms ---------------------------------------------------------
def parallel_exact(c):
    """Identical parallel coaxial squares of side 1 at separation `c`."""
    x = y = 1.0 / c
    return (2.0 / (numpy.pi * x * y)) * (
        numpy.log(numpy.sqrt((1 + x**2) * (1 + y**2) / (1 + x**2 + y**2)))
        + x * numpy.sqrt(1 + y**2) * numpy.arctan(x / numpy.sqrt(1 + y**2))
        + y * numpy.sqrt(1 + x**2) * numpy.arctan(y / numpy.sqrt(1 + x**2))
        - x * numpy.arctan(x)
        - y * numpy.arctan(y)
    )


def perpendicular_exact(h=1.0, w=1.0):
    """Squares sharing one edge at 90 degrees; H = h/l, W = w/l."""
    a = numpy.sqrt(h**2 + w**2)
    return (1.0 / (numpy.pi * w)) * (
        w * numpy.arctan(1.0 / w)
        + h * numpy.arctan(1.0 / h)
        - a * numpy.arctan(1.0 / a)
        + 0.25 * numpy.log(
            ((1 + w**2) * (1 + h**2) / (1 + w**2 + h**2))
            * (w**2 * (1 + w**2 + h**2) / ((1 + w**2) * (w**2 + h**2))) ** (w**2)
            * (h**2 * (1 + h**2 + w**2) / ((1 + h**2) * (h**2 + w**2))) ** (h**2)
        )
    )


# --- 1. parallel squares, several separations -----------------------------
print("Parallel coaxial unit squares, F(A->B) against the closed form:\n")
print(f"{'separation':>11}{'exact':>10}{'numerical':>11}{'rel err':>10}")
for c in (2.0, 1.0, 0.5, 0.25, 0.1):
    a = square([0, 0, 0], [1, 0, 0], [0, 1, 0])           # normal +z
    b = square([0, 0, c], [0, 1, 0], [1, 0, 0])           # normal -z, faces a
    num = view_factor(a, b)
    ex = parallel_exact(c)
    print(f"{c:>11.2f}{ex:>10.5f}{num:>11.5f}{abs(num / ex - 1):>9.2%}")

# --- 2. perpendicular squares sharing an edge -----------------------------
# The hard case: the shared edge puts sub-pairs at arbitrarily small
# separation, which is exactly what the point-to-point form cannot do.
a = square([0, 0, 0], [1, 0, 0], [0, 1, 0])               # z = 0, normal +z
b = square([0, 0, 0], [0, 0, 1], [1, 0, 0])               # y = 0, normal +y
ex = perpendicular_exact()
print(f"\nPerpendicular unit squares sharing an edge: exact F = {ex:.5f}")
print(f"{'ratio':>7}{'max_level':>11}{'numerical':>11}{'rel err':>10}")
for ratio, level in ((0.0, 0), (2.0, 2), (4.0, 3), (6.0, 4), (8.0, 5), (10.0, 6)):
    num = view_factor(a, b, ratio, level)
    label = "none" if level == 0 else f"{level}"
    print(f"{ratio:>7.1f}{label:>11}{num:>11.5f}{abs(num / ex - 1):>9.2%}")
print("  ratio=0 is the unsubdivided point-to-point form, for comparison.")

# --- 3. what the old guard does -------------------------------------------
# `view_factor_facets` returns 0 when the separation is below sqrt(area).
# Reproduced here on the same geometry, to show it is a deletion rather than
# an approximation.
print("\nThe `distance < sqrt(area)` guard, on the perpendicular pair:")
guarded = 0.0
area_a = sum(tri_area(t) for t in a)
for ta in a:
    for tb in b:
        ca, cb = ta.mean(axis=0), tb.mean(axis=0)
        d = numpy.linalg.norm(cb - ca)
        if d < numpy.sqrt(tri_area(tb)):
            continue  # what the guard does
        guarded += tri_area(ta) * kalast.mesh.view_factor_triangles(ta, tb, 0.0, 0)
guarded /= area_a
print(f"  guarded point-to-point : F = {guarded:.5f}  "
      f"({guarded / ex - 1:+.1%} against exact)")
print(f"  subdivided             : F = {view_factor(a, b):.5f}  "
      f"({view_factor(a, b) / ex - 1:+.1%})")

# --- 4. reciprocity and closure -------------------------------------------
# A_i F_ij = A_j F_ji is an identity, so any deviation is implementation
# error rather than discretisation. Cheap, and it catches most mistakes.
print("\nReciprocity  A_a F(a->b) == A_b F(b->a):")
for name, pa, pb in (
    ("parallel c=1", square([0, 0, 0], [1, 0, 0], [0, 1, 0]),
     square([0, 0, 1], [0, 1, 0], [1, 0, 0])),
    ("perpendicular", a, b),
):
    aa = sum(tri_area(t) for t in pa)
    ab = sum(tri_area(t) for t in pb)
    lhs = aa * view_factor(pa, pb)
    rhs = ab * view_factor(pb, pa)
    print(f"  {name:14s} {lhs:.6f} vs {rhs:.6f}   "
          f"mismatch {abs(lhs - rhs) / max(lhs, 1e-12):.2e}")
