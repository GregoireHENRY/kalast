#!/usr/bin/env python
"""One number per subsystem, to compare machines.

Run on each machine and diff the output. Everything here is synthetic or uses
`res/`, so it needs **no external data** -- no SPICE kernels, no shape models --
which is the point: it can be run on a fresh clone or a cluster node before any
of the data is in place.

    python examples/analytical/machine_benchmark.py

Reference, 2026-09-01, Apple M-series laptop (unified memory, ~400 GB/s):

    CPU TPM               10k      1.355 ms per timestep
    GPU TPM + insolation  10k      0.083 ms per timestep
    CPU TPM              100k     15.793 ms per timestep
    GPU TPM + insolation 100k      0.294 ms per timestep
    GPU TPM + insolation 3.1M      8.793 ms per timestep
    CPU insolation       3.1M     64.559 ms per timestep
    GPU radiance x7      3.1M      9.396 ms per call
    CPU radiance x7      3.1M    129.540 ms per call

Radiance is per *call*, not per timestep: it converts every facet across all
seven bands once, and is wanted per output frame rather than every step.

The GPU numbers are the ones expected to move most on a discrete card: they are
memory-bandwidth bound, so they should scale roughly with it.
"""

import platform
import sys
import time

import numpy

import kalast
from kalast.tpm import nonuniform, properties, radiance, routine
from kalast._rs.tpm.gpu import GpuContext, GpuRadiance, GpuTpm
from kalast.util import AU, SOLAR_CONSTANT, STEFAN_BOLTZMANN

SIZES = [10_000, 100_000, 3_145_728]
STEPS = {10_000: 400, 100_000: 200, 3_145_728: 40}

print(f"kalast machine benchmark")
print(f"  {platform.platform()}")
print(f"  python {sys.version.split()[0]}, numpy {numpy.__version__}\n")

prop = properties.DIDYMOS
prop.se = STEFAN_BOLTZMANN * prop.emissivity
prop.compute_conductivity_diffusivity()
D = prop.diffusivity
ls1 = properties.skin_depth_1(D, 8136.0)
z = nonuniform.column(
    ls1, m=4, n=5,
    b=properties.skin_depth_2pi(D, kalast.entity.DIDYMOS.orbit_period) / ls1,
)
dt = 0.4 * routine.nonuniform_max_dt(z, D)
cl, ch = routine.nonuniform_coefficients(z, dt)
nodes, twodz = z.size, 2.0 * (z[1] - z[0])

w = numpy.linspace(6e-6, 16e-6, 400)
bands = [radiance.BandRadiance(w, ((w >= lo) & (w <= hi)).astype(float),
                               emissivity=prop.emissivity)
         for lo, hi in [(7e-6, 9e-6), (9e-6, 11e-6), (11e-6, 14e-6),
                        (8e-6, 12e-6), (10e-6, 15e-6), (7e-6, 14e-6),
                        (6.5e-6, 15.5e-6)]]
tables = numpy.array([b.l_table for b in bands], dtype=numpy.float32)
t_min, t_max = bands[0].t_range


def timed(f, k):
    f()
    t0 = time.perf_counter()
    for _ in range(k):
        f()
    return (time.perf_counter() - t0) / k * 1000


# The unit is not the same for every row: the model and insolation are per
# timestep, radiance is per call over all facets and all bands, since it is
# computed once per output frame rather than every step.
print(f"{'subsystem':22s} {'facets':>12} {'ms':>10}  per")
for n in SIZES:
    k = STEPS[n]
    rng = numpy.random.default_rng(0)
    pos = rng.random((n, 3)) * 800.0
    nrm = rng.random((n, 3)) - 0.5
    nrm /= numpy.linalg.norm(nrm, axis=1)[:, None]
    sun = numpy.array([1.644 * AU, 0.2 * AU, -0.1 * AU])
    absorbed = SOLAR_CONSTANT * (1.0 - prop.albedo)

    # CPU model, on the smaller sizes only: 3.1M takes half a second a step
    if n <= 100_000:
        T = numpy.full((n, nodes), 200.0)
        dn = numpy.full(nodes, D)
        co = (numpy.asarray(cl, numpy.float64), numpy.asarray(ch, numpy.float64))
        f = numpy.full(n, 500.0)
        ms = timed(lambda: (routine.step_surface_newton(
            T, f, prop.se, prop.conductivity, twodz, threshold=0.1),
            routine.step_conduction(T, dn, co)), max(k // 4, 10))
        print(f"{'CPU TPM':22s} {n:12,} {ms:10.3f}  step")

    ctx = GpuContext()
    g = GpuTpm(n, numpy.asarray(cl, numpy.float32), numpy.asarray(ch, numpy.float32),
               numpy.full(nodes, D, numpy.float32), numpy.float32(prop.se),
               numpy.float32(prop.conductivity), numpy.float32(twodz),
               numpy.float32(0.1), 100, ctx)
    g.upload(numpy.full((n, nodes), 200.0, numpy.float32))
    g.set_geometry(pos.astype(numpy.float32), nrm.astype(numpy.float32))
    sp = [numpy.float32(x) for x in sun]
    a32, au32 = numpy.float32(absorbed), numpy.float32(AU)

    def gpu_step():
        g.step_sun(sp, a32, au32)
    gpu_step(); g.surface()
    t0 = time.perf_counter()
    for _ in range(k):
        gpu_step()
    g.surface()
    print(f"{'GPU TPM + insolation':22s} {n:12,} {(time.perf_counter()-t0)/k*1000:10.3f}  step")

    # CPU insolation, for the comparison that motivated moving it
    def cpu_insol():
        v = sun[None, :] - pos
        d = numpy.linalg.norm(v, axis=1)
        c = numpy.einsum("ij,ij->i", nrm, v / d[:, None])
        numpy.maximum(c, 0.0, out=c)
        return absorbed * c / (d / AU) ** 2
    print(f"{'CPU insolation':22s} {n:12,} {timed(cpu_insol, max(k//4, 5)):10.3f}  step")

    t = numpy.full(n, 280.0)
    gr = GpuRadiance(n, tables, numpy.float32(t_min), numpy.float32(t_max), ctx)
    gr.set_temperatures(t.astype(numpy.float32))
    print(f"{'GPU radiance x7':22s} {n:12,} {timed(gr.compute, 5):10.3f}  call")
    print(f"{'CPU radiance x7':22s} {n:12,} "
          f"{timed(lambda: numpy.stack([b(t) for b in bands], 1), 3):10.3f}  call")
    print()
