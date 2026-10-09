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


def test_every_per_vertex_array_has_one_row_per_vertex():
    """Eight vertices, so a smooth cube's arrays are eight rows.

    A vertex is a position; the normal and the colour are arrays of their own,
    per vertex on a smooth mesh and per facet on a flat one. The texture
    coordinate, tangent and bitangent a vertex used to carry were 36 of its 76
    bytes and no live shader read any of them.
    """
    m = kalast.mesh.Mesh(CUBE)
    assert not m.is_flat()
    assert m.positions.shape == (8, 3), m.positions.shape
    assert m.normals.shape == (8, 3), m.normals.shape
    assert m.colors.shape == (8, 3), m.colors.shape
    assert m.color_modes.shape == (8,), m.color_modes.shape
    for gone in ("textures", "tangents", "bitangents"):
        assert not hasattr(m, gone), f"{gone} should be gone"


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


def test_flatten_changes_the_shading_not_the_geometry():
    """Flattening is a flag and two arrays, not a second copy of the mesh.

    It used to rebuild the vertex array -- one entry per corner, 36 for a cube
    -- and renumber the indices to the identity to match, keeping the shared
    corners aside so `smoothen` could put them back. That was 293 MB of
    duplicated positions, normals and colours on a 3M-facet model. The corners
    stay shared now, the indices go on addressing them, and what changes is
    `colors` (per facet) and `normals` (empty, the facet's own being every
    corner's).
    """
    m = kalast.mesh.Mesh(CUBE)
    before = m.positions.copy()
    indices = list(m.indices)

    m.flatten()

    assert m.is_flat()
    assert (m.positions == before).all(), "flatten must not move a corner"
    assert list(m.indices) == indices, "nor renumber one"
    assert len(m.facets) == 12
    assert m.colors.shape == (12, 3), "a colour per facet"
    assert m.color_modes.shape == (12,)
    assert m.normals.shape == (0, 3), "and no per-vertex normal to give"


def test_a_flat_facets_corners_take_its_normal():
    """The whole point of flat shading, asked of the accessor that answers it.

    A corner shared between faces carries one averaged normal while the mesh
    is smooth; flat, each of a facet's three corners reports that facet's own.
    """
    m = kalast.mesh.Mesh(CUBE)
    m.flatten()
    for f in range(12):
        want = numpy.array(m.facets[f].normal)
        got = numpy.array(m.get_facet_normals(f))
        assert (got == want).all(), (f, got, want)


def test_flatten_and_smoothen_round_trip():
    """Both ways, any number of times, with the geometry untouched.

    A mesh loaded flat used to have nothing to go back to, because `smoothen`
    restored a copy that only `flatten` made.
    """
    m = kalast.mesh.Mesh(CUBE)
    positions, indices = m.positions.copy(), list(m.indices)
    for _ in range(3):
        m.flatten()
        assert m.is_flat() and m.colors.shape == (12, 3)
        m.smoothen()
        assert not m.is_flat() and m.colors.shape == (8, 3)
        assert m.normals.shape == (8, 3)
        assert (m.positions == positions).all()
        assert list(m.indices) == indices


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


ICO = "res/ico3.obj"


def map_at(lat, lon, image, west):
    """A simple cylindrical map's bilinear value at latitude and longitude
    (degrees): written out again here rather than borrowed from the code it
    checks. Rows from 90 N, columns east from `west`, pixel centres half a
    pixel in, the outer row past the outer centres, and across the seam."""
    rows, cols = image.shape[:2]
    v = numpy.clip((90.0 - lat) * rows / 180.0 - 0.5, 0.0, rows - 1)
    r0 = numpy.minimum(numpy.floor(v).astype(int), rows - 2)
    tv = (v - r0)[..., None]
    u = numpy.mod(lon - west, 360.0) * cols / 360.0 - 0.5
    c0 = numpy.floor(u).astype(int)
    tu = (u - c0)[..., None]
    c1, c0 = (c0 + 1) % cols, c0 % cols
    image = image.reshape(rows, cols, -1)
    return ((1 - tv) * ((1 - tu) * image[r0, c0] + tu * image[r0, c1])
            + tv * ((1 - tu) * image[r0 + 1, c0] + tu * image[r0 + 1, c1]))


def lat_lon(p):
    p = numpy.asarray(p, dtype=float)
    r = numpy.linalg.norm(p, axis=-1)
    return numpy.degrees(numpy.arcsin(p[..., 2] / r)), numpy.degrees(numpy.arctan2(p[..., 1], p[..., 0]))


def facet_means(m, image, west):
    """Each facet's mean over its area: cut into k x k equal triangles, k the
    map pixels across its widest angle from the centre (at least 1, at most
    32), one sample at the middle of each."""
    rows, cols = image.shape[:2]
    pixel = numpy.radians(min(180.0 / rows, 360.0 / cols))
    tri = numpy.asarray(m.positions, dtype=float)[m.indices.reshape(-1, 3)]
    means = []
    for a, b, c in tri:
        widest = max(
            numpy.arccos(numpy.clip(p @ q / numpy.linalg.norm(p) / numpy.linalg.norm(q), -1, 1))
            for p, q in ((a, b), (b, c), (c, a))
        )
        k = int(min(max(numpy.ceil(widest / pixel), 1), 32))
        up = [(3 * i + 1, 3 * j + 1) for i in range(k) for j in range(k - i)]
        down = [(3 * i + 2, 3 * j + 2) for i in range(k) for j in range(k - i) if i + j + 2 <= k]
        uv = numpy.array(up + down, dtype=float) / (3 * k)
        at = a + uv[:, :1] * (b - a) + uv[:, 1:] * (c - a)
        means.append(map_at(*lat_lon(at), image, west).mean(axis=0))
    return numpy.array(means)


# 10 deg pixels varying both ways, nothing symmetric to hide a flip in.
PATTERN = (numpy.add.outer(7 * numpy.arange(18), 3 * numpy.arange(36)) % 11) / 10.0


def test_colors_from_map_colours_each_facet_from_where_it_is():
    m = kalast.mesh.Mesh(ICO)
    m.flatten()
    for west in (-180.0, 0.0):
        m.colors_from_map(PATTERN.astype(numpy.float32), west=west)
        want = facet_means(m, PATTERN, west)
        err = numpy.abs(m.colors - want).max()
        assert err < 1e-5, (west, err)
    # The left edge matters: a map read from the wrong one is another map.
    assert numpy.abs(facet_means(m, PATTERN, -180.0) - facet_means(m, PATTERN, 0.0)).max() > 0.1


def test_colors_from_map_averages_a_facet_wider_than_a_pixel():
    """ico1's facets span about 4 of these 10 deg pixels, so each is the mean
    of 16 samples, not its middle alone."""
    m = kalast.mesh.Mesh("res/ico1.obj")
    m.flatten()
    m.colors_from_map(PATTERN)
    want = facet_means(m, PATTERN, -180.0)
    assert numpy.abs(m.colors - want).max() < 1e-5
    middle = map_at(*lat_lon(numpy.asarray(m.positions, dtype=float)[m.indices.reshape(-1, 3)].mean(axis=1)), PATTERN, -180.0)
    assert numpy.abs(m.colors - middle).max() > 0.1, "a facet's middle is not its mean"


def test_colors_from_map_takes_three_channels_and_colours_a_smooth_mesh_per_vertex():
    m = kalast.mesh.Mesh(ICO)
    assert not m.is_flat()
    rgb = numpy.stack([PATTERN, 1.0 - PATTERN, 0.5 * PATTERN], axis=-1)  # float64
    m.colors_from_map(rgb)
    assert m.colors.shape == (len(m.vertices), 3)
    want = map_at(*lat_lon(m.positions), rgb, -180.0)
    assert numpy.abs(m.colors - want).max() < 1e-5


def test_colors_from_map_keeps_a_colour_where_there_is_no_data_and_refuses_what_is_not_a_map():
    m = kalast.mesh.Mesh(ICO)
    m.flatten()
    m.colors[:] = 0.3
    m.colors_from_map(numpy.full((18, 36), numpy.nan))
    assert (m.colors == numpy.float32(0.3)).all()
    for bad, error in (
        ((255 * PATTERN).astype(numpy.uint8), TypeError),       # 8-bit, not divided
        (numpy.zeros((18, 36, 2)), ValueError),                  # two channels
        (numpy.zeros(36), ValueError),                            # one dimension
        (numpy.zeros((1, 36)), ValueError),                       # one row
    ):
        try:
            m.colors_from_map(bad)
        except error:
            continue
        raise AssertionError(f"{bad.dtype} {bad.shape} was taken")


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
