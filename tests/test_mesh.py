#!/usr/bin/env python
"""`kalast.mesh.Mesh`: loading, facet access, and the flatten/smoothen pair.

Moved out of `examples/mesh/test.py`, where it had been since before the
`examples/` convention existed. It was never an example -- 87 assertions, no
window, no output, nothing to read -- and living there meant nothing ran it,
so the only coverage of the flatten/smoothen round-trip was invisible to the
suite. `smoothen` appears nowhere else in `tests/` or `kalast/` but the stubs.

The values are exact on purpose. A cube's corners are exactly +/-1 and a
facet's vertex count is exactly three, so there is nothing to tolerance and an
approximate check would only be looser for no reason.

Needs `res/` and nothing else: no data paths, no GPU, no window.
"""

import numpy

import kalast.mesh

CUBE = "res/cube.obj"
# 1024 square facets triangulated to 2048, spanning -0.5..0.5 in x and y at
# z = 0, with a crater of radius 0.437 (60 % coverage) cut to the same depth.
# The Davidsson roughness test case.
CRATER = "res/plane_crater_1024-5000_h=0.437.obj"


def test_a_cube_loads_with_shared_corners():
    m = kalast.mesh.Mesh(CUBE)
    assert len(m.vertices) == 8, len(m.vertices)
    assert m.indices.size == 36, m.indices.size
    assert len(m.facets) == 12, len(m.facets)
    # Empty until `flatten` stashes the shared corners to restore later.
    assert len(m._vertices_before_flatten) == 0


def test_every_per_vertex_array_has_one_row_per_vertex():
    """Eight vertices, so every attribute array is eight rows.

    A cube `.obj` carries only positions and indices; the rest are allocated
    at load and left zero, which is what makes the *shapes* the thing to
    check rather than the contents.
    """
    m = kalast.mesh.Mesh(CUBE)
    assert m.positions.shape == (8, 3), m.positions.shape
    assert m.textures.shape == (8, 2), m.textures.shape
    assert m.normals.shape == (8, 3), m.normals.shape
    assert m.tangents.shape == (8, 3), m.tangents.shape
    assert m.bitangents.shape == (8, 3), m.bitangents.shape
    assert m.colors.shape == (8, 3), m.colors.shape
    assert m.color_modes.shape == (8,), m.color_modes.shape


def test_facet_indices_and_positions_are_the_same_question():
    m = kalast.mesh.Mesh(CUBE)
    assert m.get_facet_indices(0) == [0, 1, 2]
    assert m.get_facet_indices(1) == [1, 3, 4]
    assert m.get_facet_indices(11) == [0, 2, 7]

    for f, want in (
        (0, [[-1.0, 1.0, 1.0], [1.0, -1.0, 1.0], [1.0, 1.0, 1.0]]),
        (1, [[1.0, -1.0, 1.0], [-1.0, -1.0, -1.0], [1.0, -1.0, -1.0]]),
        (11, [[-1.0, 1.0, 1.0], [1.0, 1.0, 1.0], [1.0, 1.0, -1.0]]),
    ):
        assert (m.get_facet_positions(f) == numpy.array(want)).all(), (f, m.get_facet_positions(f))
        # The two routes must agree, which is the point of having both.
        assert (m.get_facet_positions(f) == m.positions[m.get_facet_indices(f)]).all(), f


def test_facet_normals_are_the_cube_faces():
    m = kalast.mesh.Mesh(CUBE)
    assert (m.facets[0].normal == numpy.array([0.0, 0.0, 1.0])).all()
    assert (m.facets[1].normal == numpy.array([0.0, -1.0, 0.0])).all()
    assert (m.facets[11].normal == numpy.array([0.0, 1.0, 0.0])).all()


def test_flatten_gives_every_facet_its_own_corners():
    m = kalast.mesh.Mesh(CUBE)
    m.flatten()
    assert len(m.vertices) == 36, len(m.vertices)
    assert m.indices.size == 36, m.indices.size
    assert len(m.facets) == 12, len(m.facets)
    # The shared corners are kept, which is what lets `smoothen` undo this.
    assert len(m._vertices_before_flatten) == 8
    assert m.positions.shape == (36, 3), m.positions.shape
    assert m.normals.shape == (36, 3), m.normals.shape


def test_flatten_moves_no_geometry():
    """Unsharing corners must not move one of them.

    Checked on `positions[3f:3f+3]`, the rows flatten actually rebuilds, so
    it holds independently of how the facet accessors index into them -- see
    `test_flatten_renumbers_the_indices_to_the_identity` for that half.
    """
    m = kalast.mesh.Mesh(CUBE)
    before = {f: numpy.array(m.get_facet_positions(f)) for f in range(12)}
    m.flatten()
    for f, want in before.items():
        got = m.positions[3 * f:3 * f + 3]
        assert (got == want).all(), (f, got, want)


def test_flatten_gives_every_corner_its_own_facet_normal():
    """The whole point of flattening, and the invariant worth asserting.

    Before it a corner shared between faces carries one averaged normal;
    after it each of a facet's three rows carries that facet's own.
    """
    m = kalast.mesh.Mesh(CUBE)
    m.flatten()
    for f in range(12):
        want = numpy.array(m.facets[f].normal)
        got = m.normals[3 * f:3 * f + 3]
        assert (got == want).all(), (f, got, want)


def test_flatten_renumbers_the_indices_to_the_identity():
    """A flat mesh's vertices are triangle-major, so its indices are `0..3f`.

    They used not to be: `flatten` rebuilt the vertices and left the shared
    pre-flatten indices in place, so facet 1 still pointed at rows 1, 3 and 4
    where its corners had moved to 3, 4 and 5. Everything that read `indices`
    without first asking `is_flat()` was then wrong for every facet but facet
    0, whose stale indices happened to be `[0, 1, 2]` anyway.

    The renderer never noticed -- `gpu.rs` draws a flat mesh sequentially and
    ignores the index buffer -- and neither did ray casting or
    `Mesh::get_facet_vertices`, both of which branch on `is_flat()`. What did
    notice was `recompute_facets`; see below.
    """
    m = kalast.mesh.Mesh(CUBE)
    m.flatten()
    assert list(m.indices) == list(range(36)), list(m.indices)[:9]
    for f in range(12):
        assert m.get_facet_indices(f) == [3 * f, 3 * f + 1, 3 * f + 2], (f, m.get_facet_indices(f))
        got = numpy.array(m.get_facet_positions(f))
        assert (got == m.positions[3 * f:3 * f + 3]).all(), (f, got)


def test_recompute_facets_survives_a_flatten():
    """The regression that made the renumbering worth doing.

    `compute_facets` reads `indices`, so with the stale ones a flattened cube
    recomputed to **NaN** normals off a degenerate triangle -- 10 of 12 facets
    wrong and a NaN total area -- from a method documented as the thing to
    call after moving vertices, on meshes every render example loads flat --
    the default.
    """
    m = kalast.mesh.Mesh(CUBE)
    want_n = [numpy.array(f.normal).copy() for f in m.facets]
    m.flatten()
    m.recompute_facets()
    for f in range(12):
        assert numpy.allclose(numpy.array(m.facets[f].normal), want_n[f]), (
            f, m.facets[f].normal, want_n[f],
        )
    total = sum(float(f.area) for f in m.facets)
    assert abs(total - 24.0) < 1e-4, total   # a cube of side 2


def test_smoothen_restores_the_shared_topology():
    m = kalast.mesh.Mesh(CUBE)
    want = list(m.indices)
    m.flatten()
    m.smoothen()
    assert list(m.indices) == want, list(m.indices)[:9]


def test_smoothen_is_the_exact_inverse_of_flatten():
    m = kalast.mesh.Mesh(CUBE)
    before = {f: numpy.array(m.get_facet_positions(f)) for f in (0, 1, 11)}
    m.flatten()
    m.smoothen()
    assert len(m.vertices) == 8, len(m.vertices)
    assert m.indices.size == 36, m.indices.size
    assert len(m.facets) == 12, len(m.facets)
    assert len(m._vertices_before_flatten) == 0
    for f, want in before.items():
        assert (numpy.array(m.get_facet_positions(f)) == want).all(), (
            f,
            m.get_facet_positions(f),
        )


def test_the_crater_plane_loads_as_a_triangulated_grid():
    m = kalast.mesh.Mesh(CRATER)
    assert len(m.vertices) == 1089, len(m.vertices)   # 33 x 33
    assert m.indices.size == 6144, m.indices.size     # 2048 x 3
    assert len(m.facets) == 2048, len(m.facets)
    assert len(m._vertices_before_flatten) == 0

    assert m.get_facet_indices(0) == [0, 1, 2]
    assert m.get_facet_indices(1) == [3, 4, 0]
    assert m.get_facet_indices(10) == [21, 22, 19]
    assert m.get_facet_indices(2047) == [1086, 1088, 1087]


def test_the_crater_plane_corners_sit_on_the_grid():
    """The rim is flat at z = 0 and the grid pitch is 1/32."""
    m = kalast.mesh.Mesh(CRATER)
    for f, want in (
        (0, [[-0.46875, -0.5, 0], [-0.5, -0.46875, 0], [-0.5, -0.5, 0]]),
        (1, [[-0.4375, -0.5, 0], [-0.46875, -0.46875, 0], [-0.46875, -0.5, 0]]),
        (2, [[-0.40625, -0.5, 0], [-0.4375, -0.46875, 0], [-0.4375, -0.5, 0]]),
        (10, [[-0.15625, -0.5, 0], [-0.1875, -0.46875, 0], [-0.1875, -0.5, 0]]),
        (2047, [[0.5, 0.46875, 0.0], [0.5, 0.5, 0.0], [0.46875, 0.5, 0.0]]),
    ):
        assert (m.get_facet_positions(f) == numpy.array(want)).all(), (f, m.get_facet_positions(f))


if __name__ == "__main__":
    # Runnable without pytest, which is not installed here.
    failures = 0
    for name, fn in sorted(globals().items()):
        if name.startswith("test_") and callable(fn):
            try:
                fn()
                print(f"ok   {name}")
            except Exception as e:
                # Not just AssertionError: a test that raises anything at all
                # has to report and let the rest run, or one broken check
                # hides every check after it.
                failures += 1
                print(f"FAIL {name}\n     {type(e).__name__}: {e}")
    raise SystemExit(failures)
