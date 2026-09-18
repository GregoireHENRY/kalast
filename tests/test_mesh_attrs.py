#!/usr/bin/env python
"""Surface attributes come from a per-facet buffer, and both mesh kinds read it.

A mesh's normal, colour, colour mode and value used to be four vertex
attributes, carried on every corner -- three times over for a flat mesh,
whose three corners share one facet normal, one colour and one value between
them. They live in a storage buffer now, indexed by `vertex_index / 3` for a
flat mesh (drawn non-indexed, triangle-major) and by `vertex_index` for a
smooth one (drawn indexed). One buffer, one shader, and the instance's flat
flag choosing between them -- which is exactly the thing that could break
silently, shading one kind of mesh from another's attributes.

So this renders a flat body and a smooth body in one scene and checks each
is shaded from its own: both lit, the smooth one smoothly (many brightness
levels across a sphere), the flat one in facets. Then it colours whole facets
of the flat body and checks the colour lands on those facets and nowhere
else.

Opens a window, in the background.
"""

import glob
import os
import sys
import tempfile

import numpy
from _png import read_png

from kalast.app import App

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CRATER = os.path.join(ROOT, "res", "plane_crater_1024-5000_h=0.437.obj")
ICO = os.path.join(ROOT, "res", "ico3.obj")

failures: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def main() -> int:
    out = tempfile.mkdtemp(prefix="kalast_attrs_")
    app = App()
    app.config.open_in_background = True
    app.config.width, app.config.height = 700, 520
    c = app.simulation.config
    c.export.dir, c.export.sync = out, True
    c.shadows.resolution = 2048
    c.axes.style = "off"
    # No multisampling: an antialiased edge blends one facet's colour into its
    # neighbour's pixels, while the facet index map is not antialiased, so the
    # two disagree at every boundary and a pixel-exact check has nothing to
    # stand on.
    c.shading.msaa = 1
    sim = app.simulation

    sim.load_mesh(path=CRATER)                       # flat, the default
    m = numpy.eye(4)
    m[:3, :3] *= 0.6
    m[:3, 3] = [0.0, 0.0, 1.6]
    sim.load_mesh(path=ICO, mat=m, smooth=True)      # shared vertices
    sim.sun.pos = [6.0, 14.0, 9.0]
    sim.camera.pos = [3.4, -4.6, 3.2]
    sim.camera.anchor = [0.0, 0.0, 0.2]
    sim.camera.look_anchor()

    def grab() -> numpy.ndarray:
        """The frame in RGB. Not the max channel: a red repaint on a white
        surface leaves the maximum where it was, so a grey projection hides
        exactly the change this test is looking for."""
        before = set(glob.glob(f"{out}/*.png"))
        sim.export_once()
        app.step()
        new = set(glob.glob(f"{out}/*.png")) - before
        assert len(new) == 1, new
        return read_png(new.pop()).astype(numpy.int64)

    def facet_ids() -> numpy.ndarray:
        for _ in range(3):
            sim.request_facet_id()
            app.step()
        got = sim.facet_id_map()
        assert got is not None, "no facet id map"
        return numpy.asarray(got[0]), list(got[1])

    for _ in range(3):
        app.step()
    shaded = grab()
    grey = shaded.max(axis=2)

    # Which pixels are which body. Only flat meshes reach the facet index map
    # -- the index comes from the vertex index -- so the smooth body is what
    # is drawn and *not* in it.
    ids, offsets = facet_ids()
    n_flat = len(sim.bodies[0].mesh.indices) // 3
    flat_px = (ids > offsets[0]) & (ids <= offsets[0] + n_flat)
    drawn = (numpy.abs(shaded).max(axis=2) > 12)
    smooth_px = drawn & (ids == 0)

    check(
        "both bodies are on screen",
        flat_px.sum() > 5000 and smooth_px.sum() > 800,
        f"{flat_px.sum()} flat px, {smooth_px.sum()} smooth px",
    )
    for name, mask in (("flat", flat_px), ("smooth", smooth_px)):
        lit = grey[mask]
        check(f"the {name} body is lit", lit.mean() > 20.0, f"mean brightness {lit.mean():.1f}")
        check(
            f"the {name} body is shaded from its own normals, not one flat tone",
            len(numpy.unique(lit)) > 20,
            f"{len(numpy.unique(lit))} distinct brightness levels",
        )

    # Colour whole facets of the flat body, and see the colour land on those
    # facets and nowhere else.
    mesh = sim.bodies[0].mesh
    cols = numpy.asarray(mesh.colors)
    assert cols.shape == (n_flat, 3), f"a flat mesh is coloured per facet: {cols.shape}"
    painted = numpy.arange(0, n_flat, 4)
    cols[painted] = [1.0, 0.0, 0.0]
    mesh.mark_colors_dirty()
    app.step()
    coloured = grab()

    ids2, _ = facet_ids()
    facet_of = ids2.astype(numpy.int64) - offsets[0] - 1
    # Interior pixels only -- one whose neighbours belong to another facet
    # sits on an edge, where a half-covered pixel belongs to both -- and only
    # where the surface is lit brightly enough for red to differ from white
    # at all. In shadow both render near black and the check would measure
    # the shadow, not the colour.
    bright = grey > 60
    on = flat_px & bright & numpy.isin(facet_of, painted)
    off = flat_px & bright & ~numpy.isin(facet_of, painted)
    changed = (coloured != shaded).any(axis=2)
    # Proportions, not pixel-exact: a facet here is a handful of pixels and
    # the boundary between two of them is shared, so a few per cent land on
    # the wrong side of the index map. Colour written to the wrong facet
    # would show up as both of these failing at once.
    check(
        "the painted facets changed colour",
        changed[on].mean() > 0.85,
        f"{changed[on].sum()} of {on.sum()} px, {changed[on].mean():.1%}",
    )
    check(
        "and almost no other facet did",
        changed[off].mean() < 0.05,
        f"{changed[off].sum()} of {off.sum()} px, {changed[off].mean():.1%}",
    )
    inner_smooth = smooth_px.copy()
    for axis in (0, 1):
        for shift in (1, -1):
            inner_smooth &= numpy.roll(smooth_px, shift, axis=axis)
    check(
        "nor did the smooth body, which shares the shader",
        changed[inner_smooth].sum() == 0,
        f"{changed[inner_smooth].sum()} of {inner_smooth.sum()} px changed",
    )

    app.close()
    app.step()

    if failures:
        print(f"\n{len(failures)} failure(s)")
        return 1
    print("\nall ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
