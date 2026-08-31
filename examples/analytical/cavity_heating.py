#!/usr/bin/env python
"""Self-heating validated on a closed isothermal cavity.

`hemicube.py` validates the view-factor kernel itself against closed forms.
This validates what consumes it -- `kalast.tpm.heating`: the chunked sparse
assembly, the shared-column layout, and the absorbed-flux physics.

A sealed box is the configuration where the answer is known without a
reference value, the same reason section 9.3e used it: every direction out of
a facet lands on another facet, so the row sums must be 1, and **an
isothermal black cavity must be in equilibrium** -- each facet absorbs exactly
the `sigma T^4` it emits. Nothing about that requires a closed form to
compare against, and it exercises the whole chain end to end.

Three checks:

  closure      row sums are 1.
  reciprocity  `sum_i A_i VF_ij = A_j rowsum_j`. Checked in this aggregated
               form rather than per pair: per pair it is quantisation-limited
               (mean 12 % at 128 px, since a small `VF_ij` is a handful of
               pixels and one direction can resolve a facet the other misses),
               and none of that is bias. Summed over `j` the noise averages
               out and a systematic asymmetry survives -- which is what a
               clipped frustum looks like, the defect fixed in 9f75c53.
  cavity       eps=1 balances; eps<1 falls short by exactly `1 - eps`.

Run:  python examples/analytical/cavity_heating.py [resolution] [subdivisions]
"""

import sys
from pathlib import Path

import numpy

import kalast
from kalast.tpm import heating
from kalast.util import STEFAN_BOLTZMANN

RES = int(sys.argv[1]) if len(sys.argv) > 1 else 128
N = int(sys.argv[2]) if len(sys.argv) > 2 else 6
BOX = Path("/tmp/kalast_cavity_box.obj")


def write_box(path, n):
    """A closed cube, subdivided n x n per face, with normals pointing in."""
    v, tri = [], []
    for axis in range(3):
        for sign in (-1.0, 1.0):
            base = len(v)
            for a in range(n + 1):
                for b in range(n + 1):
                    p = [0.0, 0.0, 0.0]
                    p[axis] = sign
                    p[(axis + 1) % 3] = -1.0 + 2.0 * a / n
                    p[(axis + 2) % 3] = -1.0 + 2.0 * b / n
                    v.append(p)
            for a in range(n):
                for b in range(n):
                    i0 = base + a * (n + 1) + b
                    i2 = i0 + (n + 1)
                    tri.append((i0, i2, i0 + 1))
                    tri.append((i0 + 1, i2, i2 + 1))
    v = numpy.array(v)
    # Orient inward by testing rather than by tracking winding per face: the
    # normal has to point back toward the origin.
    out = []
    for (i0, i1, i2) in tri:
        n_ = numpy.cross(v[i1] - v[i0], v[i2] - v[i0])
        if numpy.dot(n_, (v[i0] + v[i1] + v[i2]) / 3.0) > 0:
            i1, i2 = i2, i1
        out.append((i0, i1, i2))
    with open(path, "w") as fh:
        for p in v:
            fh.write(f"v {p[0]:.6f} {p[1]:.6f} {p[2]:.6f}\n")
        for (i0, i1, i2) in out:
            fh.write(f"f {i0 + 1} {i1 + 1} {i2 + 1}\n")
    return len(v), len(out)


nv, nf = write_box(BOX, N)
print(f"closed box: {nv} vertices, {nf} facets, inward-facing, "
      f"hemicube {RES} px")

app = kalast.app.App()
app.config.width = 256
app.config.height = 256
app.config.vsync = False
app.simulation.load_mesh(path=str(BOX), mat=numpy.eye(4), flatten=True)

mesh = app.simulation.bodies[0].mesh
nface = len(mesh.facets)
area = numpy.array([mesh.facets[i].area for i in range(nface)])

builder = heating.ViewFactorBuilder(
    body=0, n_facets=nface, resolution=RES, batch=64, chunk=100
)


def before_render(sim, _dt):
    sim.camera.pos = numpy.array([5.0, 0.0, 0.0])
    sim.camera.dir = numpy.array([-1.0, 0.0, 0.0])
    if sim.state.iteration >= 2:
        builder.request(sim)


def after_render(sim, _dt):
    if sim.state.iteration < 2 or builder.done:
        return
    if not builder.collect(sim, [nface]):
        return

    vf = builder.result
    rs = vf.row_sums()
    print(f"\n{vf.nnz:,} nonzeros, {vf.nnz / nface:.0f} per row, "
          f"{vf.nnz / nface ** 2:.1%} dense")
    print(f"closure      row sum  min {rs.min():.5f}  mean {rs.mean():.5f}  "
          f"max {rs.max():.5f}")

    lhs = vf.matrix.T.dot(area)
    rhs = area * rs
    rel = numpy.abs(lhs - rhs) / numpy.maximum(rhs, 1e-30)
    print(f"reciprocity  mean {rel.mean():.2e}  max {rel.max():.2e}")
    print("             scales with facet size, not resolution -- the "
          "hemicube samples\n             facet i at its centre, and a "
          "point-to-area view factor does not obey\n             reciprocity "
          "with an area-to-area one. Measured 3.9e-2 at 192 facets\n"
          "             falling to 1.5e-2 at 2,352, and flat in resolution "
          "from 64 to 256 px.")

    T = 300.0
    print()
    for eps in (1.0, 0.9):
        em = heating.emitted(numpy.full(nface, T), eps)
        q = heating.absorbed(vf, vf.stack([em]), None,
                             emissivity=eps, albedo=0.0)
        out_ = eps * STEFAN_BOLTZMANN * T ** 4
        print(f"cavity       eps={eps}  emits {out_:8.3f}  absorbs "
              f"{q.mean():8.3f} W/m2  net {(q - out_).mean():+8.3f} "
              f"({q.mean() / out_ - 1:+.3%})")
    print("\neps=1 must balance: a black isothermal cavity is in equilibrium.\n"
          "eps=0.9 must fall short by 1-eps, the light this single-bounce\n"
          "model reflects and never re-absorbs -- see the module docstring.")

    sys.stdout.flush()
    import os
    os._exit(0)


app.before_render = before_render
app.after_render = after_render
app.start()
