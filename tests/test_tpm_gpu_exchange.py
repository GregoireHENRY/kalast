#!/usr/bin/env python
"""`GpuExchange` from Python: what facets send to one another, on the GPU,
against `kalast.tpm.exchange.absorbed` on the CPU.

The two are tested side by side in Rust (`tpm::gpu_exchange::tests`). Here: the
binding, with the same view factors, surfaces and calls on both, at one bounce,
at several, and followed to the end; an isothermal closed box; and the passes
the GPU counts beforehand.

Without a GPU the tests say so and check nothing.
"""

import sys

import numpy
from scipy import sparse

from kalast.tpm import exchange
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


def gpu_class():
    """`GpuExchange` and a device, or `None` where no GPU answers."""
    from kalast._rs.tpm.gpu import GpuContext, GpuExchange

    try:
        return GpuExchange, GpuContext()
    except RuntimeError as e:
        print(f"     (no GPU here, nothing checked: {e})")
        return None, None


GpuExchange, context = gpu_class()
if GpuExchange is None:
    sys.exit(0)

rng = numpy.random.default_rng(11)
n = 2000
m = sparse.random(n, n, density=0.02, random_state=11, format="csr", dtype=numpy.float64)
m.setdiag(0.0)
m.eliminate_zeros()
m = sparse.csr_matrix(m.multiply(0.65 / numpy.maximum(m.sum(axis=1), 1e-9)))
vf = exchange.ViewFactors.from_csr(m.data.astype(numpy.float32), m.indices, m.indptr, n)
albedo = (0.05 + 0.5 * rng.random(n)).astype(numpy.float32)
emissivity = (0.6 + 0.4 * rng.random(n)).astype(numpy.float32)
sunlight = (1361.0 * numpy.maximum(rng.random(n) - 0.4, 0.0)).astype(numpy.float32)
temperature = (120.0 + 250.0 * rng.random(n)).astype(numpy.float32)

gpu = GpuExchange(vf, albedo, emissivity, context=context)
check("the facets", gpu.facets == n)
for bounces in (1, 4, 0):
    want = exchange.absorbed(vf, albedo, emissivity, incident=sunlight, temperature=temperature, bounces=bounces)
    got = gpu.absorbed(incident=sunlight, temperature=temperature, bounces=bounces)
    err = max(numpy.abs(got[0] - want[0]).max() / want[0].max(), numpy.abs(got[1] - want[1]).max() / want[1].max())
    check(f"bounces={bounces}: the GPU is the CPU", err < 5e-6, f"largest difference {err:.1e} of the largest flux, {gpu.passes(bounces)} passes")

only = gpu.absorbed(temperature=temperature, bounces=2)
both = gpu.absorbed(incident=sunlight, temperature=temperature, bounces=2)
check("sunlight left out: nothing scattered, the thermal part the same", not only[0].any() and numpy.array_equal(only[1], both[1]))
check("a number of bounces asked for is taken as given", gpu.passes(7) == 7)
check("to the end: more passes for a tighter tolerance", gpu.passes(0, 1e-9) > gpu.passes(0, 1e-3) >= 1,
      f"{gpu.passes(0, 1e-3)} and {gpu.passes(0, 1e-9)}")


# an isothermal closed box
def rectangle(origin, e1, e2):
    o, e1, e2 = (numpy.asarray(v, dtype=numpy.float32) for v in (origin, e1, e2))
    return [numpy.array([o, o + e1, o + e1 + e2]), numpy.array([o, o + e1 + e2, o + e2])]


x, y, z = [1.3, 0, 0], [0, 0.8, 0], [0, 0, 2.1]
o = numpy.array([-0.4, 0.2, 0.1])
faces = (rectangle(o, x, y) + rectangle(o + z, y, x) + rectangle(o, z, x) + rectangle(o + y, x, z)
         + rectangle(o, y, z) + rectangle(o + x, z, y))
box = exchange.ViewFactors.exact(numpy.array(faces, dtype=numpy.float32))
black = STEFAN_BOLTZMANN * 280.0**4
for e in (1.0, 0.9, 0.3):
    _, thermal = GpuExchange(box, 0.1, e, context=context).absorbed(temperature=numpy.full(12, 280.0), tolerance=1e-7)
    check(f"isothermal box on the GPU, emissivity {e}", numpy.allclose(thermal, e * black, rtol=2e-5),
          f"{thermal.mean():.3f} against {e * black:.3f} W/m2")

check("an albedo above one is refused", raises(ValueError, lambda: GpuExchange(vf, 1.2, 0.9, context=context), "albedo"))
check("a temperature a facet short is refused", raises(ValueError, lambda: gpu.absorbed(temperature=temperature[:-1]), "temperature"))
check("a cavity that absorbs nothing is refused", raises(
    ValueError, lambda: GpuExchange(box, 1.0, 0.9, context=context).absorbed(incident=numpy.full(12, 100.0)), "does not settle"))

if failures:
    print(f"\n{len(failures)} failed: {failures}")
    sys.exit(1)
print("\nall passed")
