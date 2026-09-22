#!/usr/bin/env python
"""The four ways to run the model, and what each costs.

The thermophysical model and the radiance conversion each run on the CPU or
the GPU, independently. All four combinations work; this checks that they
agree and measures them, so the choice can be made on numbers.

    TPM   radiance   how the temperatures cross
    CPU   CPU        never leave numpy
    GPU   CPU        one readback per snapshot
    CPU   GPU        one upload per snapshot
    GPU   GPU        nothing moves -- radiance reads the TPM's own buffer

The last one needs both built on a single `GpuContext`; otherwise each makes
its own device and the buffers cannot be shared. Pass one and the temperatures
never touch host memory.

Run:  python examples/analytical/backends.py [n_facets] [n_steps]
"""

import sys
import time
from pathlib import Path

import numpy

import kalast
from kalast.tpm import nonuniform, properties, radiance, routine
from kalast._rs.tpm.gpu import GpuContext, GpuRadiance, GpuTpm
from kalast.util import SOLAR_CONSTANT, STEFAN_BOLTZMANN

N_FACETS = int(sys.argv[1]) if len(sys.argv) > 1 else 100000
N_STEPS = int(sys.argv[2]) if len(sys.argv) > 2 else 300
RESPONSE = Path("/Users/gregoireh/data/hera/tiri/response.csv")
SPIN = 8136.0

prop = properties.DIDYMOS
prop.se = STEFAN_BOLTZMANN * prop.emissivity
prop.compute_conductivity_diffusivity()
D = prop.diffusivity
ls1 = properties.skin_depth_1(D, SPIN)
z = nonuniform.column(
    ls1, m=4, n=5,
    b=1.0 * properties.skin_depth_2pi(D, kalast.entity.DIDYMOS.orbit_period) / ls1,
)
dt = 0.4 * routine.nonuniform_max_dt(z, D)
coef_lo, coef_hi = routine.nonuniform_coefficients(z, dt)
twodz = 2.0 * (z[1] - z[0])
nodes = z.size

if RESPONSE.exists():
    bands = radiance.tiri_bands(RESPONSE, emissivity=prop.emissivity)
    names = list(bands)
else:
    # Synthetic boxcar filters, so this runs without the instrument data.
    print(f"note: {RESPONSE} not found; using synthetic boxcar filters\n")
    w = numpy.linspace(6e-6, 16e-6, 400)
    bands, names = {}, []
    for i, (lo, hi) in enumerate([(7e-6, 9e-6), (9e-6, 11e-6), (11e-6, 14e-6)]):
        r = ((w >= lo) & (w <= hi)).astype(float)
        n = f"box{i}"
        bands[n] = radiance.BandRadiance(w, r, emissivity=prop.emissivity)
        names.append(n)

b0 = bands[names[0]]
tables = numpy.array([bands[n].l_table for n in names], dtype=numpy.float32)
t_min, t_max = b0.t_range

print(f"{N_FACETS:,} facets x {nodes} nodes, {len(names)} bands, {N_STEPS:,} steps")
print(f"bands: {', '.join(names)}\n")

phase = numpy.linspace(0.0, 2.0 * numpy.pi, N_FACETS, endpoint=False)
tilt = numpy.cos(numpy.linspace(-1.2, 1.2, N_FACETS))


def flux_at(step):
    ang = 2.0 * numpy.pi * step * dt / SPIN
    cosi = numpy.cos(ang + phase) * tilt
    numpy.maximum(cosi, 0.0, out=cosi)
    return SOLAR_CONSTANT * (1.0 - prop.albedo) * cosi / 1.644 ** 2


def cpu_radiance(t):
    return numpy.stack([bands[n](t) for n in names], axis=1)


results, timings = {}, {}

# ---- CPU TPM ---------------------------------------------------------
T = numpy.full((N_FACETS, nodes), 200.0, dtype=numpy.float64)
dn = numpy.full(nodes, D)
coefs = (numpy.asarray(coef_lo, numpy.float64), numpy.asarray(coef_hi, numpy.float64))
t0 = time.perf_counter()
for i in range(N_STEPS):
    routine.step_surface_newton(T, flux_at(i), prop.se, prop.conductivity, twodz,
                                threshold=kalast.util.NEWTON_METHOD_THRESHOLD)
    routine.step_conduction(T, dn, coefs)
tpm_cpu_s = time.perf_counter() - t0
t_cpu = T[:, 0].copy()

t0 = time.perf_counter()
results["CPU / CPU"] = cpu_radiance(t_cpu)
timings["CPU / CPU"] = tpm_cpu_s + (time.perf_counter() - t0)

rad_only_cpu = time.perf_counter()
_ = cpu_radiance(t_cpu)
rad_only_cpu = time.perf_counter() - rad_only_cpu

# CPU TPM -> GPU radiance: one upload
gr = GpuRadiance(N_FACETS, tables, numpy.float32(t_min), numpy.float32(t_max))
t0 = time.perf_counter()
gr.set_temperatures(t_cpu.astype(numpy.float32))
results["CPU / GPU"] = gr.compute()
rad_only_gpu_up = time.perf_counter() - t0
timings["CPU / GPU"] = tpm_cpu_s + rad_only_gpu_up

# ---- GPU TPM, sharing one context with GPU radiance ------------------
ctx = GpuContext()
gt = GpuTpm(N_FACETS, numpy.asarray(coef_lo, numpy.float32),
            numpy.asarray(coef_hi, numpy.float32),
            numpy.full(nodes, D, numpy.float32), numpy.float32(prop.se),
            numpy.float32(prop.conductivity), numpy.float32(twodz),
            numpy.float32(kalast.util.NEWTON_METHOD_THRESHOLD), 100, ctx)
gt.upload(numpy.full((N_FACETS, nodes), 200.0, numpy.float32))
t0 = time.perf_counter()
for i in range(N_STEPS):
    gt.step(flux_at(i).astype(numpy.float32))
gt.surface()                      # drain the queue so the timing is honest
tpm_gpu_s = time.perf_counter() - t0

t0 = time.perf_counter()
results["GPU / CPU"] = cpu_radiance(gt.surface().astype(numpy.float64))
timings["GPU / CPU"] = tpm_gpu_s + (time.perf_counter() - t0)

gr2 = GpuRadiance(N_FACETS, tables, numpy.float32(t_min), numpy.float32(t_max), ctx)
gr2.bind_tpm(gt)
t0 = time.perf_counter()
results["GPU / GPU"] = gr2.compute(gt.front)
rad_only_gpu_bound = time.perf_counter() - t0
timings["GPU / GPU"] = tpm_gpu_s + rad_only_gpu_bound

# ---- agreement -------------------------------------------------------
ref = results["CPU / CPU"]
print("agreement in band radiance, against CPU / CPU (W/m2/sr)")
for k, v in results.items():
    d = numpy.abs(v - ref)
    rel = d / numpy.maximum(numpy.abs(ref), 1e-30)
    print(f"  {k:11s} max abs {d.max():.3e}   max rel {rel.max():.3e}")

print("\ntiming")
print(f"  {'combination':12s} {'TPM':>9} {'radiance':>10} {'total':>9}")
print(f"  {'CPU / CPU':12s} {tpm_cpu_s:8.3f}s {rad_only_cpu:9.4f}s "
      f"{timings['CPU / CPU']:8.3f}s")
print(f"  {'CPU / GPU':12s} {tpm_cpu_s:8.3f}s {rad_only_gpu_up:9.4f}s "
      f"{timings['CPU / GPU']:8.3f}s")
print(f"  {'GPU / CPU':12s} {tpm_gpu_s:8.3f}s {rad_only_cpu:9.4f}s "
      f"{timings['GPU / CPU']:8.3f}s")
print(f"  {'GPU / GPU':12s} {tpm_gpu_s:8.3f}s {rad_only_gpu_bound:9.4f}s "
      f"{timings['GPU / GPU']:8.3f}s")

best = min(timings, key=timings.get)
worst = max(timings, key=timings.get)
print(f"\nfastest: {best}  ({timings[worst] / timings[best]:.1f}x the slowest, "
      f"{worst})")
print("radiance alone, GPU against CPU: "
      f"{rad_only_cpu / max(rad_only_gpu_bound, 1e-9):.1f}x bound, "
      f"{rad_only_cpu / max(rad_only_gpu_up, 1e-9):.1f}x via upload")
