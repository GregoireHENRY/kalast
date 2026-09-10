#!/usr/bin/env python
"""Self-heating on a closed isothermal cavity, where the answer is exact.

`test_view_factors.py` pins the view-factor kernel against closed forms. This
pins what consumes it -- `kalast.tpm.heating`, 471 lines with no test between
them: the chunked sparse assembly, the shared-column layout, and the
absorbed-flux physics.

A sealed box is the configuration whose answer needs no reference value. Every
direction out of a facet lands on another facet, so:

- **closure**: every row of the view-factor matrix must sum to 1;
- **equilibrium**: a black isothermal cavity absorbs exactly the `sigma T^4`
  it emits, because there is nowhere else for the energy to go.

At emissivity `eps < 1` the balance is not 1 but `eps`, and that is a
statement about the model rather than an error: each facet emits
`eps sigma T^4` and absorbs `eps` of what arrives, so absorbed/emitted is
`eps` exactly. The missing `1 - eps` is the light this single-bounce model
reflects and never re-absorbs. Checking both values is what makes the test
sensitive to the emissivity being applied twice, or not at all.

From `examples/analytical/cavity_heating.py`, which computes all of this and
prints it. Opens a real window: the view factors come off the GPU hemicube.
"""

import sys
import tempfile
from pathlib import Path

import numpy

import kalast
from kalast.tpm import heating
from kalast.util import STEFAN_BOLTZMANN

# Small enough to stay quick, large enough that closure is a real constraint.
# The example's defaults are 128 px and n=6 (432 facets); the numbers below
# are flat in resolution from 64 to 256 px, so the cheaper setting measures
# the same thing.
RES = 64
N = 4
T = 300.0

# Closure is a property of the hemicube, not of the scene, so it should hold
# tightly: measured 1.00001 for min, mean and max alike.
CLOSURE_TOL = 1e-3

# Absorbed/emitted must equal eps. Measured +0.001 % at eps=1 and -9.999 %
# against sigma T^4 at eps=0.9, i.e. 0.9000 for the ratio. The budget is what
# the hemicube's own closure error can carry into it, with headroom.
BALANCE_TOL = 5e-3

# Reciprocity is deliberately loose. The hemicube samples facet i at its
# centre, so it produces a point-to-area view factor, which does not obey
# reciprocity with an area-to-area one. The example measured 3.9e-2 at 192
# facets falling to 1.5e-2 at 2,352 -- it scales with facet size and is flat
# in resolution. Bounded here only to catch it becoming wild.
RECIPROCITY_MAX = 0.35

failures: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


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
    # Oriented inward by testing rather than by tracking winding per face: the
    # normal has to point back toward the origin.
    out = []
    for i0, i1, i2 in tri:
        n_ = numpy.cross(v[i1] - v[i0], v[i2] - v[i0])
        if numpy.dot(n_, (v[i0] + v[i1] + v[i2]) / 3.0) > 0:
            i1, i2 = i2, i1
        out.append((i0, i1, i2))
    with open(path, "w") as fh:
        for p in v:
            fh.write(f"v {p[0]:.6f} {p[1]:.6f} {p[2]:.6f}\n")
        for i0, i1, i2 in out:
            fh.write(f"f {i0 + 1} {i1 + 1} {i2 + 1}\n")
    return len(v), len(out)


def main() -> int:
    box = Path(tempfile.gettempdir()) / "kalast_test_cavity_box.obj"
    write_box(box, N)

    app = kalast.app.App()
    app.config.width = 256
    app.config.height = 256
    sim = app.simulation
    sim.config.vsync = False
    sim.load_mesh(path=str(box), mat=numpy.eye(4), flatten=True)

    mesh = sim.bodies[0].mesh
    nface = len(mesh.facets)
    area = numpy.array([mesh.facets[i].area for i in range(nface)])

    builder = heating.ViewFactorBuilder(
        body=0, n_facets=nface, resolution=RES, batch=64, chunk=100
    )

    # The builder wants a settled scene: the first frames have no geometry on
    # the GPU yet. Requested before the step and collected after, the way
    # every GPU query in this engine works.
    sim.camera.pos = numpy.array([5.0, 0.0, 0.0])
    sim.camera.dir = numpy.array([-1.0, 0.0, 0.0])

    for i in range(4000):
        if not app.running:
            break
        if i >= 2:
            builder.request(sim)
        app.step()
        if i >= 2 and not builder.done:
            builder.collect(sim, [nface])
        if builder.done:
            break

    if not builder.done:
        print("FAIL view factors never finished building")
        return 1

    vf = builder.result
    rs = vf.row_sums()

    check(
        "test_every_row_of_the_view_factor_matrix_sums_to_one",
        float(numpy.abs(rs - 1.0).max()) <= CLOSURE_TOL,
        f"{nface} facets, row sums min {rs.min():.5f} mean {rs.mean():.5f} "
        f"max {rs.max():.5f} (tol {CLOSURE_TOL:g})",
    )

    lhs = vf.matrix.T.dot(area)
    rhs = area * rs
    rel = numpy.abs(lhs - rhs) / numpy.maximum(rhs, 1e-30)
    check(
        "test_reciprocity_stays_within_its_known_bound",
        float(rel.max()) <= RECIPROCITY_MAX,
        f"mean {rel.mean():.2e} max {rel.max():.2e} "
        f"(loose by design: point-to-area vs area-to-area)",
    )

    # A black isothermal cavity is in equilibrium; at eps < 1 the ratio is eps.
    for eps in (1.0, 0.9):
        em = heating.emitted(numpy.full(nface, T), eps)
        q = heating.absorbed(vf, vf.stack([em]), None, emissivity=eps, albedo=0.0)
        emits = eps * STEFAN_BOLTZMANN * T**4
        ratio = float(q.mean()) / emits
        check(
            f"test_isothermal_cavity_absorbs_eps_of_what_it_emits_at_eps_{eps}",
            abs(ratio - eps) <= BALANCE_TOL,
            f"emits {emits:.3f}, absorbs {q.mean():.3f} W/m2, "
            f"ratio {ratio:.5f} vs eps {eps} (tol {BALANCE_TOL:g})",
        )

    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
