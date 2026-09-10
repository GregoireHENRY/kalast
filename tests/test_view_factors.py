#!/usr/bin/env python
"""The view-factor kernel against closed forms, and the guard it replaced.

Self-heating, mutual heating and sub-facet roughness all reduce to view-factor
bookkeeping, so an error here propagates into every one of them. Two
configurations from the standard catalogue have exact answers -- parallel
coaxial unit squares, and perpendicular unit squares sharing an edge -- and
that is enough to pin both the value and the convergence.

`examples/analytical/view_factors.py` has computed all of this from the start
and prints it; this is the same method with the numbers asserted. See
`notes/2026-09-10_code_quality_audit.md` on why that distinction matters.

The third section is the one worth keeping. `mesh::view_factor_facets` uses
the point-to-point form with a guard returning **zero** below `sqrt(area)`
separation. Adjacent facets sit exactly at that threshold, so on the
neighbours that dominate self-heating inside a concavity the guard does not
approximate them -- it deletes them, and the perpendicular pair comes out
37.8 % low. `view_factor_triangles` subdivides instead. Pinning the size of
that gap is what stops the cheaper form being reinstated as an optimisation.

Pure CPU: no GPU, no window, ~2 s.
"""

import sys

import numpy

import kalast

RATIO = 6.0
MAX_LEVEL = 4

failures: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def square(origin, u, v):
    """Two triangles covering a square, wound so the normal is `u x v`."""
    o, u, v = (numpy.asarray(x, dtype=numpy.float32) for x in (origin, u, v))
    return [
        numpy.array([o, o + u, o + u + v], dtype=numpy.float32),
        numpy.array([o, o + u + v, o + v], dtype=numpy.float32),
    ]


def tri_area(t):
    return 0.5 * numpy.linalg.norm(numpy.cross(t[1] - t[0], t[2] - t[0]))


def view_factor(patch_a, patch_b, ratio=RATIO, max_level=MAX_LEVEL):
    """`F(A->B)`, area-weighted over the triangles of each patch."""
    area_a = sum(tri_area(t) for t in patch_a)
    total = 0.0
    for ta in patch_a:
        for tb in patch_b:
            total += tri_area(ta) * kalast.mesh.view_factor_triangles(
                ta, tb, ratio, max_level
            )
    return total / area_a


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
    """Unit squares sharing one edge at 90 degrees."""
    a = numpy.sqrt(h**2 + w**2)
    return (1.0 / (numpy.pi * w)) * (
        w * numpy.arctan(1.0 / w)
        + h * numpy.arctan(1.0 / h)
        - a * numpy.arctan(1.0 / a)
        + 0.25
        * numpy.log(
            ((1 + w**2) * (1 + h**2) / (1 + w**2 + h**2))
            * (w**2 * (1 + w**2 + h**2) / ((1 + w**2) * (w**2 + h**2))) ** (w**2)
            * (h**2 * (1 + h**2 + w**2) / ((1 + h**2) * (h**2 + w**2))) ** (h**2)
        )
    )


# --- 1. parallel squares, across separations ------------------------------
# Measured relative errors: 0.75, 0.63, 0.20, 0.09, 0.52 % at c = 2, 1, 0.5,
# 0.25, 0.1. The budget is the worst of those with headroom; it is not
# monotone in `c` because the near field is subdivision-limited while the far
# field is quadrature-limited, so a single bound is the honest statement.
PARALLEL_MAX_REL = 0.015

worst_parallel = 0.0
for c in (2.0, 1.0, 0.5, 0.25, 0.1):
    a = square([0, 0, 0], [1, 0, 0], [0, 1, 0])  # normal +z
    b = square([0, 0, c], [0, 1, 0], [1, 0, 0])  # normal -z, faces a
    rel = abs(view_factor(a, b) / parallel_exact(c) - 1.0)
    worst_parallel = max(worst_parallel, rel)

check(
    "test_parallel_squares_match_the_closed_form",
    worst_parallel <= PARALLEL_MAX_REL,
    f"worst {worst_parallel:.2%} over c = 2..0.1 (budget {PARALLEL_MAX_REL:.1%})",
)

# --- 2. perpendicular squares, and convergence in subdivision -------------
# The hard case: a shared edge puts sub-pairs at arbitrarily small separation,
# which is exactly what a point-to-point form cannot represent.
A = square([0, 0, 0], [1, 0, 0], [0, 1, 0])  # z = 0, normal +z
B = square([0, 0, 0], [0, 0, 1], [1, 0, 0])  # y = 0, normal +y
PERP_EXACT = perpendicular_exact()

levels = [(0.0, 0), (2.0, 2), (4.0, 3), (6.0, 4), (8.0, 5), (10.0, 6)]
errs = [abs(view_factor(A, B, r, l) / PERP_EXACT - 1.0) for r, l in levels]

check(
    "test_perpendicular_squares_converge_with_subdivision",
    all(errs[i] > errs[i + 1] for i in range(len(errs) - 1)),
    "errors " + ", ".join(f"{e:.2%}" for e in errs),
)
check(
    "test_perpendicular_squares_reach_the_closed_form",
    errs[-1] <= PARALLEL_MAX_REL,
    f"{errs[-1]:.2%} at level 6 (budget {PARALLEL_MAX_REL:.1%})",
)

# Each extra level roughly halves the error -- first order in the subdivision,
# which is what the flat-patch kernel should give. Asserted loosely because
# the ratio wanders with the recursion bound, but tightly enough that a
# subdivision that stopped working would show as a ratio near 1.
ratios = [errs[i] / errs[i + 1] for i in range(1, len(errs) - 1)]
check(
    "test_each_subdivision_level_roughly_halves_the_error",
    all(r > 1.3 for r in ratios),
    "ratios " + ", ".join(f"{r:.2f}" for r in ratios),
)

# --- 3. reciprocity, which is an identity ---------------------------------
# A_i F_ij = A_j F_ji holds exactly, discretisation or not, so any deviation
# is implementation error rather than truncation. Cheap and catches most
# mistakes -- area weighting, winding, argument order.
for name, pa, pb in (
    (
        "parallel",
        square([0, 0, 0], [1, 0, 0], [0, 1, 0]),
        square([0, 0, 1], [0, 1, 0], [1, 0, 0]),
    ),
    ("perpendicular", A, B),
):
    aa = sum(tri_area(t) for t in pa)
    ab = sum(tri_area(t) for t in pb)
    lhs = aa * view_factor(pa, pb)
    rhs = ab * view_factor(pb, pa)
    mismatch = abs(lhs - rhs) / max(lhs, 1e-12)
    check(
        f"test_reciprocity_holds_for_{name}_squares",
        mismatch < 1e-6,
        f"{lhs:.6f} vs {rhs:.6f}, mismatch {mismatch:.2e}",
    )

# --- 4. the guard is a deletion, not an approximation ---------------------
# `view_factor_facets` returns 0 below `sqrt(area)` separation. Reproduced
# here on the geometry where it bites, so the cost of the cheaper form is a
# number in the suite rather than a paragraph in a note.
guarded = 0.0
area_a = sum(tri_area(t) for t in A)
for ta in A:
    for tb in B:
        d = numpy.linalg.norm(tb.mean(axis=0) - ta.mean(axis=0))
        if d < numpy.sqrt(tri_area(tb)):
            continue  # what the guard does
        guarded += tri_area(ta) * kalast.mesh.view_factor_triangles(ta, tb, 0.0, 0)
guarded /= area_a

guard_err = guarded / PERP_EXACT - 1.0
check(
    "test_the_distance_guard_deletes_rather_than_approximates",
    guard_err < -0.25,
    f"guarded F = {guarded:.5f}, {guard_err:+.1%} against exact "
    f"{PERP_EXACT:.5f} -- subdivision gets {view_factor(A, B) / PERP_EXACT - 1:+.1%}",
)

sys.exit(1 if failures else 0)
