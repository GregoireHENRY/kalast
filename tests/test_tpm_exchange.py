#!/usr/bin/env python
"""The radiative exchange between facets, from Python.

`kalast.tpm.exchange` is the Rust core's `tpm::exchange`: sparse view factors and
what every facet absorbs of the sunlight the others scatter and of their thermal
radiation, with the reflections. The Rust tests hold the physics (a two-facet
cavity against its closed form, an isothermal closed box, light let into a
closed box). Here:

1. the binding against `kalast.tpm.heating`, the Python module it replaces, on
   a random sparse matrix: one bounce to the last digits, several bounces on
   one body's square block;
2. an isothermal closed box built with `ViewFactors.exact`: every facet absorbs
   what it emits, at any emissivity;
3. the three ways to build the factors agree, and what makes no sense raises.

Pure CPU: no GPU, no window.
"""

import sys

import numpy
from scipy import sparse

from kalast.tpm import exchange, heating
from kalast.util import STEFAN_BOLTZMANN

failures: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def raises(kind, call, *words):
    try:
        call()
    except kind as e:
        return all(w in str(e) for w in words)
    return False


rng = numpy.random.default_rng(7)

# 1. against kalast.tpm.heating on a random square matrix, rows summing to less than one
n = 400
m = sparse.random(n, n, density=0.05, random_state=7, format="csr", dtype=numpy.float64)
m.setdiag(0.0)
m.eliminate_zeros()
m = sparse.csr_matrix(m.multiply(0.6 / numpy.maximum(m.sum(axis=1), 1e-9)))  # every row sums to 0.6
vf = exchange.ViewFactors.from_csr(m.data.astype(numpy.float32), m.indices, m.indptr, n)
check("shape and count", vf.shape == (n, n) and vf.nnz == m.nnz, f"{vf!r}")
check("row sums", numpy.allclose(vf.row_sums(), 0.6, atol=1e-6))
x = rng.random(n)
check("the product", numpy.allclose(vf.dot(x), m.dot(x), rtol=1e-5))

albedo, emissivity = 0.25, 0.85
sunlight = 1361.0 * numpy.maximum(rng.random(n) - 0.4, 0.0)
temperature = 150.0 + 200.0 * rng.random(n)
old = heating.ViewFactors(m, [0], [n])
emitted = heating.emitted(temperature, emissivity)
for bounces in (1, 3, 8):
    want_ir = heating.absorbed(old, emitted, None, emissivity, albedo, bounces=bounces, body=0)
    want_vis = heating.absorbed(old, None, albedo * sunlight, emissivity, albedo, bounces=bounces, body=0)
    scattered, thermal = exchange.absorbed(vf, albedo, emissivity, incident=sunlight, temperature=temperature, bounces=bounces)
    err = max(numpy.abs(scattered / want_vis - 1.0).max(), numpy.abs(thermal / want_ir - 1.0).max())
    check(f"{bounces} bounce(s) as kalast.tpm.heating", err < 2e-5, f"relative difference {err:.1e}")

# until it settles: one more bounce changes nothing
settled = exchange.absorbed(vf, albedo, emissivity, incident=sunlight, temperature=temperature)
more = exchange.absorbed(vf, albedo, emissivity, incident=sunlight, temperature=temperature, bounces=60)
check("bounces=0 follows them to the end", numpy.allclose(settled[0], more[0], rtol=1e-5) and numpy.allclose(settled[1], more[1], rtol=1e-5))

# a value a facet as well as one for all
per_facet = exchange.absorbed(vf, numpy.full(n, albedo), numpy.full(n, emissivity), incident=sunlight, temperature=temperature)
check("one value a facet", numpy.array_equal(per_facet[0], settled[0]) and numpy.array_equal(per_facet[1], settled[1]))


# 2. an isothermal closed box
def rectangle(origin, e1, e2):
    o, e1, e2 = (numpy.asarray(v, dtype=numpy.float32) for v in (origin, e1, e2))
    return [numpy.array([o, o + e1, o + e1 + e2]), numpy.array([o, o + e1 + e2, o + e2])]


x, y, z = [1.3, 0, 0], [0, 0.8, 0], [0, 0, 2.1]
o = numpy.array([-0.4, 0.2, 0.1])
faces = (rectangle(o, x, y) + rectangle(o + z, y, x) + rectangle(o, z, x) + rectangle(o + y, x, z)
         + rectangle(o, y, z) + rectangle(o + x, z, y))
box = exchange.ViewFactors.exact(numpy.array(faces, dtype=numpy.float32))
check("a closed box sends everything to itself", numpy.allclose(box.row_sums(), 1.0, atol=2e-6), f"{box!r}")
black = STEFAN_BOLTZMANN * 280.0**4
for e in (1.0, 0.9, 0.3):
    _, thermal = exchange.absorbed(box, 0.1, e, temperature=numpy.full(12, 280.0), tolerance=1e-9)
    check(f"isothermal box, emissivity {e}: absorbs what it emits", numpy.allclose(thermal, e * black, rtol=1e-5),
          f"{thermal.mean():.3f} against {e * black:.3f} W/m2")

# 3. the three ways to build the factors, and the refusals
data, indices, indptr = box.csr()
again = exchange.ViewFactors.from_csr(data, indices, indptr, 12)
dense = sparse.csr_matrix((data, indices, indptr), shape=(12, 12)).toarray().astype(numpy.float32)
third = exchange.ViewFactors.from_dense(dense)
check("from_csr, from_dense and exact agree", numpy.array_equal(again.row_sums(), box.row_sums())
      and numpy.array_equal(third.row_sums(), box.row_sums()) and third.nnz == box.nnz)
check("an albedo above one is refused", raises(ValueError, lambda: exchange.absorbed(vf, 1.2, 0.9, incident=sunlight), "albedo"))
check("a temperature a facet short is refused", raises(ValueError, lambda: exchange.absorbed(vf, 0.1, 0.9, temperature=temperature[:-1]), "temperature"))
check("rows that are not square are refused", raises(
    ValueError, lambda: exchange.absorbed(exchange.ViewFactors.from_dense(numpy.zeros((2, 3), dtype=numpy.float32)), 0.1, 0.9), "every facet"))
check("a cavity that absorbs nothing is refused", raises(
    ValueError, lambda: exchange.absorbed(box, 1.0, 0.9, incident=numpy.full(12, 100.0)), "did not settle"))
check("triangles are (n, 3, 3)", raises(ValueError, lambda: exchange.ViewFactors.exact(numpy.zeros((4, 3, 2), dtype=numpy.float32)), "(n, 3, 3)"))

if failures:
    print(f"\n{len(failures)} failed: {failures}")
    sys.exit(1)
print("\nall passed")
