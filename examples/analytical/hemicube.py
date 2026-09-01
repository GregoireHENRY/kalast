#!/usr/bin/env python
"""Hemicube view factors on the GPU, validated against the closed forms.

`view_factors.py` established a CPU reference: correct near-field behaviour
from subdivision, validated to ~1% on two configurations with known answers.
It has two limits that rule it out as the method -- no occlusion test at all,
and O(N^2) cost (1.8 years and 38 TB at 3.1M facets).

The hemicube removes both. Place a cube's top half at a facet, render the
scene into its five faces with each facet writing its own **index**, and the
depth test has already resolved occlusion exactly. The number of pixels a
facet covers, weighted by the per-pixel delta form factor, *is* its view
factor. One set of renders yields the whole row `VF[i, :]`, so the matrix
costs O(N) render passes rather than O(N^2) pair tests, and the result is
naturally sparse -- only facets that are genuinely visible appear.

The near-field problem dissolves too: a close facet simply covers many
pixels. There is no singular 1/d^2 and no threshold to choose.

Delta form factors
------------------

With the hemicube half-width 1 and the facet at the origin:

    top face,  z = 1, pixel at (x, y):   dF = da / (pi (x^2 + y^2 + 1)^2)
    side face, y = 1, pixel at (x, z):   dF = z da / (pi (x^2 + z^2 + 1)^2)

They sum to 1 over the whole hemicube, which is the check applied below
before any geometry is involved -- if the weights do not close to unity the
rest is meaningless.

This prototype drives the existing facet-index pass once per face, reading
back over PCIe each time. That is the slow way to do it and the numbers at
the end say how slow; the point here is to establish correctness against
known answers first.
"""

import sys
import tempfile
import time
from pathlib import Path

import numpy

import kalast

RES = 128          # hemicube face resolution
SUBDIV = 3         # subdivision level of the emitting square, for area-averaging
# Portable: this used to hardcode one session's scratchpad directory, which
# does not exist on any other machine or after that session ends.
OUT = Path(tempfile.gettempdir()) / "kalast_hemicube"


# --- delta form factors ---------------------------------------------------
def delta_top(res):
    """Per-pixel form factor for the top face, and the pixel directions."""
    e = (numpy.arange(res) + 0.5) / res * 2.0 - 1.0     # pixel centres in [-1, 1]
    x, y = numpy.meshgrid(e, e, indexing="xy")
    da = (2.0 / res) ** 2
    return da / (numpy.pi * (x * x + y * y + 1.0) ** 2)


def delta_side(res):
    """Per-pixel form factor for a side face.

    Only the half of the face above the facet plane contributes; the other
    half looks below the horizon and is masked to zero.
    """
    e = (numpy.arange(res) + 0.5) / res * 2.0 - 1.0
    x, z = numpy.meshgrid(e, e, indexing="xy")
    da = (2.0 / res) ** 2
    w = z * da / (numpy.pi * (x * x + z * z + 1.0) ** 2)
    return numpy.where(z > 0, w, 0.0)


def closure_check(res):
    """The weights must sum to 1 over the whole hemicube."""
    return delta_top(res).sum() + 4.0 * delta_side(res).sum()


print(f"hemicube {RES}x{RES} per face")
for r in (32, 64, 128, 256):
    print(f"  closure at {r:4d}: sum(dF) = {closure_check(r):.6f}  "
          f"(error {abs(closure_check(r) - 1):.2e})")
print()


# --- the scene: two unit squares, as an OBJ the renderer can load ---------
# Perpendicular, sharing an edge: the configuration where the CPU kernel is
# worst (51.7% error unsubdivided) and where occlusion and near field both
# matter. Exact F = 0.20004.
def write_obj(path):
    """Emitter square in z=0 (normal +z), receiver in y=0 (normal +y)."""
    v = [
        (0, 0, 0), (1, 0, 0), (1, 1, 0), (0, 1, 0),   # emitter, z = 0
        (0, 0, 0), (1, 0, 0), (1, 0, 1), (0, 0, 1),   # receiver, y = 0
    ]
    f = [(1, 2, 3), (1, 3, 4), (5, 7, 6), (5, 8, 7)]
    with open(path, "w") as fh:
        for x, y, z in v:
            fh.write(f"v {x} {y} {z}\n")
        for a, b, c in f:
            fh.write(f"f {a} {b} {c}\n")


def tri_subdivide(t, level):
    """`4**level` sub-triangles of `t`."""
    tris = [t]
    for _ in range(level):
        out = []
        for a, b, c in tris:
            ab, bc, ca = (a + b) / 2, (b + c) / 2, (c + a) / 2
            out += [(a, ab, ca), (ab, b, bc), (ca, bc, c), (ab, bc, ca)]
        tris = out
    return tris


def tri_area(t):
    return 0.5 * numpy.linalg.norm(numpy.cross(t[1] - t[0], t[2] - t[0]))


OUT.mkdir(parents=True, exist_ok=True)
obj = OUT / "two_squares.obj"
write_obj(obj)

# Sample points: centroids of the subdivided emitter, with their areas.
emitter = [
    (numpy.array([0., 0, 0]), numpy.array([1., 0, 0]), numpy.array([1., 1, 0])),
    (numpy.array([0., 0, 0]), numpy.array([1., 1, 0]), numpy.array([0., 1, 0])),
]
samples = []
for t in emitter:
    for st in tri_subdivide(t, SUBDIV):
        samples.append((sum(st) / 3.0, tri_area(st)))
total_area = sum(a for _, a in samples)
print(f"emitter sampled at {len(samples)} points, total area {total_area:.4f}")

# --- render ---------------------------------------------------------------
app = kalast.app.App()
app.config.width = RES
app.config.height = RES
app.config.vsync = False
app.config.access_shadow_map = False
app.simulation.camera.projection.fovy = numpy.pi / 2.0     # 90 deg: one cube face
app.simulation.load_mesh(path=str(obj), mat=numpy.eye(4), flatten=True)
n_facets = len(app.simulation.bodies[0].mesh.facets)
# Facets 0,1 are the emitter; 2,3 the receiver.
RECEIVER = {2, 3}
print(f"scene has {n_facets} facets; receiver is {sorted(RECEIVER)}")

W_TOP = delta_top(RES)
# Image row 0 is the top of the frame, i.e. +up. Build the side weights in
# image order so no flip is needed at accumulation time.
W_SIDE = delta_side(RES)[::-1, :]

# Five faces: the top, then four sides. `up` for a side face is the facet
# normal, so the visible half of that face is the upper half of the image --
# which is what W_SIDE masks.
NORMAL_EPS = 1e-4

state = {"i": 0, "acc": [], "t0": None}


def faces(centre, normal, tangent):
    b = numpy.cross(normal, tangent)
    o = centre + NORMAL_EPS * normal
    return [
        (o, normal, tangent, W_TOP),
        (o, tangent, normal, W_SIDE),
        (o, -tangent, normal, W_SIDE),
        (o, b, normal, W_SIDE),
        (o, -b, normal, W_SIDE),
    ]


def before_render(sim, dt):
    k = state["i"]
    if k >= len(samples) * 5:
        return
    centre, _ = samples[k // 5]
    normal = numpy.array([0.0, 0.0, 1.0])
    tangent = numpy.array([1.0, 0.0, 0.0])
    pos, d, up, _ = faces(centre, normal, tangent)[k % 5]
    sim.camera.pos = pos
    sim.camera.dir = d
    sim.camera.up = up
    sim.request_facet_id()


def after_render(sim, dt):
    k = state["i"]
    if state["t0"] is None:
        state["t0"] = time.perf_counter()
    if k >= len(samples) * 5:
        finish()
        return
    r = sim.facet_id_map()
    if r is None:
        return
    ids = numpy.asarray(r[0])
    _, _, _, w = faces(numpy.zeros(3), numpy.array([0., 0, 1]),
                       numpy.array([1., 0, 0]))[k % 5]
    # ids hold 1 + facet; sum the weight of every pixel showing a receiver facet
    hit = numpy.isin(ids - 1, list(RECEIVER)) & (ids > 0)
    state["acc"].append(float(w[hit].sum()))
    state["i"] += 1


def finish():
    elapsed = time.perf_counter() - state["t0"]
    acc = numpy.array(state["acc"]).reshape(len(samples), 5).sum(axis=1)
    areas = numpy.array([a for _, a in samples])
    f = float((acc * areas).sum() / areas.sum())
    exact = 0.20004
    print(f"\nhemicube F(emitter -> receiver) = {f:.5f}")
    print(f"  exact                        = {exact:.5f}")
    print(f"  error                        = {abs(f / exact - 1):.2%}")
    print(f"\n{len(samples) * 5} renders in {elapsed:.1f}s "
          f"({elapsed / (len(samples) * 5) * 1000:.1f} ms per face, "
          f"{elapsed / len(samples) * 1000:.1f} ms per hemicube)")
    print(f"  -> a 10,000-facet matrix this way: "
          f"{elapsed / len(samples) * 10000 / 60:.1f} min")
    import os
    sys.stdout.flush()
    os._exit(0)


app.before_render = before_render
app.after_render = after_render
app.start()
