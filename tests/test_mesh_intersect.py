#!/usr/bin/env python
"""Ray/plane, ray/triangle and ray/mesh intersection.

Moved out of `examples/mesh/test_intercept.py`, which was never an example --
39 assertions, no window, nothing to read -- and where nothing ran it. These
four free functions appear in `src/mesh.rs` and its bindings and in no other
test, so this was their only coverage.

**Every numeric check here was one-sided in the original**, written as

    assert numpy.all(r[1] - expected < tol)

with no `abs()`. A result of `[0, 0, -99]` against an expected `[0, 0, 1]`
gives a difference of `-100`, which is comfortably `< tol`, and passed. Fourteen
of them, so most of this file's arithmetic was unchecked in one direction. They
are `close()` now, which is two-sided, and the tolerance is `1e-6` throughout:
the mesh is f32, so ~1e-7 is the floor, and the original 1e-8 comparisons were
below the precision they claimed to test even had they been two-sided.

**Making them two-sided showed four of the expected values to be wrong**, two
of them badly: the 45-degree ray from the origin was written as hitting
`-0.30834377` where it hits `-0.30872506`, off by 3.8e-4, and the one from
`z = 0.4` by 6.6e-5. The other two were literals rounded to four decimals.
Every replacement was checked against the crossing computed independently in
numpy from the facet's own three corners -- the engine agrees with that to
1.7e-7 or better at every point here, so the code was right all along and only
the expectations were wrong.

Needs `res/` and nothing else: no data paths, no GPU, no window.
"""

import numpy

import kalast.mesh

CUBE = "res/cube.obj"
# The Davidsson roughness plane: 2048 triangles over -0.5..0.5 in x and y at
# z = 0, with a crater of radius 0.437 cut to the same depth.
CRATER = "res/plane_crater_1024-5000_h=0.437.obj"
TOL = 1e-6

Z = numpy.array([0.0, 0.0, 1.0])
DOWN = numpy.array([0.0, 0.0, -1.0])


def close(got, want, tol=TOL):
    """Two-sided, unlike every comparison this file was written with."""
    return numpy.allclose(numpy.asarray(got, float), numpy.asarray(want, float),
                          atol=tol, rtol=0.0)


def test_a_point_above_a_triangle_is_in_its_prism():
    """`is_point_in_or_on_triangle` tests the 3D prism, not the plane."""
    a, b, c = (numpy.array(v) for v in ([0.0, 0, 0], [1.0, 0, 0], [0.0, 1, 0]))
    assert kalast.mesh.is_point_in_or_on_triangle(numpy.array([0.0, 0.0, 2.0]), a, b, c)


def test_a_ray_hits_the_plane_it_points_at():
    x = kalast.mesh.intersect_plane(Z, DOWN, numpy.array([0.0, 0, 0]), Z)
    assert close(x, [0.0, 0.0, 0.0]), x


def test_a_ray_is_infinite_so_it_hits_the_plane_behind_it_too():
    """Worth pinning: `intersect_plane` does not check the ray's direction.

    A caller who cares must ask `is_facing_plane` separately, which is the
    whole reason that function exists.
    """
    p, u = numpy.array([0.0, 0.0, 2.0]), Z
    x = kalast.mesh.intersect_plane(p, u, numpy.array([0.0, 0, 0]), Z)
    assert close(x, [0.0, 0.0, 0.0]), x
    assert not kalast.mesh.is_facing_plane(u, Z)


def test_a_triangle_is_a_plane_plus_its_boundary():
    a, b, c = (numpy.array(v) for v in ([0.0, 0, 0], [1.0, 0, 0], [0.0, 1, 0]))
    # Inside: same answer as the plane.
    x = kalast.mesh.intersect_triangle(Z, DOWN, a, b, c, Z)
    assert close(x, [0.0, 0.0, 0.0]), x

    # Outside: the plane still answers, the triangle does not.
    p = numpy.array([2.0, 0.0, 1.0])
    assert close(kalast.mesh.intersect_plane(p, DOWN, a, Z), [2.0, 0.0, 0.0])
    assert kalast.mesh.intersect_triangle(p, DOWN, a, b, c, Z) is None


def test_a_cube_is_hit_on_the_face_the_ray_meets():
    m = kalast.mesh.Mesh(CUBE)
    for p, u, facet, point in (
        ([0.0, 0.0, 10.0], [0.0, 0.0, -1.0], 0, [0.0, 0.0, 1.0]),
        ([0.0, 0.0, -10.0], [0.0, 0.0, 1.0], 3, [0.0, 0.0, -1.0]),
        ([-10.0, 0.0, 0.0], [1.0, 0.0, 0.0], 2, [-1.0, 0.0, 0.0]),
        ([10.0, 0.0, 0.0], [-1.0, 0.0, 0.0], 4, [1.0, 0.0, 0.0]),
    ):
        r = m.intersect(numpy.array(p), numpy.array(u))
        assert r is not None, (p, u)
        assert r[0] == facet, (p, u, r[0])
        assert close(r[1], point), (p, u, r[1])


def test_a_vertical_ray_reaches_the_crater_floor():
    m = kalast.mesh.Mesh(CRATER)
    r = m.intersect(numpy.array([0.0, 0.0, 0.0]), DOWN)
    assert r[0] == 496, r[0]
    assert close(r[1], [0.0, 0.0, -0.437]), r[1]


def test_a_ray_parallel_to_the_facet_normals_takes_the_first_it_meets():
    """The denominator is zero for a facet the ray runs along, so those are
    skipped and the first facet actually crossed wins."""
    m = kalast.mesh.Mesh(CRATER)
    r = m.intersect(numpy.array([1.0, 0.0, 0.0]), numpy.array([-1.0, 0.0, 0.0]))
    assert r[0] == 509, r[0]
    assert close(r[1], [0.4375, 0.0, 0.0]), r[1]


def test_forty_five_degree_rays_walk_up_the_crater_wall():
    m = kalast.mesh.Mesh(CRATER)
    diag = numpy.array([-1.0, 0.0, -1.0])
    for z, facet, point in (
        (0.0, 1510, [-0.30872506, 0.0, -0.30872506]),
        (0.4, 1506, [-0.43140486, 0.0, -0.03140485]),
        (0.5, 480, [-0.5, 0.0, 0.0]),          # the rim, on the -x edge
    ):
        r = m.intersect(numpy.array([0.0, 0.0, z]), diag)
        assert r is not None, z
        assert r[0] == facet, (z, r[0])
        assert close(r[1], point), (z, r[1])

    # Just above the rim it leaves entirely.
    assert m.intersect(numpy.array([0.0, 0.0, 0.6]), diag) is None


def test_the_same_ray_from_further_back_lands_in_the_same_place():
    """Cast from (1, 0, 1) instead of (0, 0, 0), same 45 degree direction.

    The two agree to 1.2e-7, which is f32 resolution on a 0.3-sized
    coordinate, not a disagreement about geometry.
    """
    m = kalast.mesh.Mesh(CRATER)
    diag = numpy.array([-1.0, 0.0, -1.0])
    near = m.intersect(numpy.array([0.0, 0.0, 0.0]), diag)
    far = m.intersect(numpy.array([1.0, 0.0, 1.0]), diag)
    assert far[0] == near[0] == 1510, (near[0], far[0])
    gap = float(numpy.linalg.norm(numpy.asarray(far[1], float) - numpy.asarray(near[1], float)))
    assert gap < 1e-6, gap


def test_a_ray_from_under_the_crater_hits_the_floor_from_behind():
    """Facing is not checked, same as the plane case -- `is_facing_plane` is
    the separate question."""
    m = kalast.mesh.Mesh(CRATER)
    u = numpy.array([0.0, 0.0, 1.0])
    r = m.intersect(numpy.array([0.0, 0.0, -10.0]), u)
    assert r[0] == 496, r[0]
    assert close(r[1], [0.0, 0.0, -0.437]), r[1]
    assert not kalast.mesh.is_facing_plane(u, Z)


def test_horizontal_rays_cut_the_crater_at_their_own_height():
    m = kalast.mesh.Mesh(CRATER)
    u = numpy.array([-1.0, 0.0, 0.0])
    for z, facet, point in (
        (-0.1, 541, [0.41809177, 0.0, -0.1]),
        (-0.4, 501, [0.17499483, 0.0, -0.4]),
        (-0.437, 496, [0.0, 0.0, -0.437]),     # the deepest point
    ):
        r = m.intersect(numpy.array([1.0, 0.0, z]), u)
        assert r is not None, z
        assert r[0] == facet, (z, r[0])
        assert close(r[1], point), (z, r[1])

    # A millimetre below the floor there is nothing left to hit.
    assert m.intersect(numpy.array([1.0, 0.0, -0.438]), u) is None


if __name__ == "__main__":
    # Runnable without pytest, which is not installed here.
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
