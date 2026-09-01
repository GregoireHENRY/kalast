#!/usr/bin/env python
"""The GPU thermophysical model against the numpy one, for correctness and speed.

The CPU path runs float64; WGSL has no f64, so the GPU path is f32 throughout.
The two therefore cannot agree exactly, and the question is whether the
difference is small against anything that matters -- the ~0.1 K Newton
threshold, and the +/-6.6 K a single flipped shadow sample is already worth at
a terminator facet.

Driven with a diurnal insolation cycle rather than a constant flux, so the
surface swings the full ~200 K a real run does and the comparison exercises
the boundary solve, not just the interior relaxation.

Run:  python examples/analytical/tpm_gpu_vs_cpu.py [n_facets] [n_steps]
"""

import sys
import time

import numpy

import kalast
from kalast.tpm import nonuniform, properties, routine
from kalast._rs.tpm.gpu import GpuTpm
from kalast.util import SOLAR_CONSTANT, STEFAN_BOLTZMANN

N_FACETS = int(sys.argv[1]) if len(sys.argv) > 1 else 10000
N_STEPS = int(sys.argv[2]) if len(sys.argv) > 2 else 2000
SPIN = 8136.0  # Didymos, s

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

print(f"{N_FACETS:,} facets x {nodes} nodes, dt = {dt:.2f} s, {N_STEPS:,} steps")
print(f"state = {N_FACETS * nodes * 4 / 1e6:.1f} MB in f32\n")

# A spread of latitudes so facets do not all see the same sun.
phase = numpy.linspace(0.0, 2.0 * numpy.pi, N_FACETS, endpoint=False)
tilt = numpy.cos(numpy.linspace(-1.2, 1.2, N_FACETS))


def flux_at(step):
    """Absorbed insolation this step, W/m2, over a diurnal cycle."""
    ang = 2.0 * numpy.pi * step * dt / SPIN
    cosi = numpy.cos(ang + phase) * tilt
    numpy.maximum(cosi, 0.0, out=cosi)
    return (SOLAR_CONSTANT * (1.0 - prop.albedo) * cosi / 1.644 ** 2)


T_cpu = numpy.full((N_FACETS, nodes), 200.0, dtype=numpy.float64)
d_nodes = numpy.full(nodes, D, dtype=numpy.float64)
coefs = (numpy.asarray(coef_lo, numpy.float64), numpy.asarray(coef_hi, numpy.float64))

gpu = GpuTpm(
    N_FACETS,
    numpy.asarray(coef_lo, numpy.float32),
    numpy.asarray(coef_hi, numpy.float32),
    numpy.full(nodes, D, dtype=numpy.float32),
    numpy.float32(prop.se),
    numpy.float32(prop.conductivity),
    numpy.float32(twodz),
    numpy.float32(kalast.util.NEWTON_METHOD_THRESHOLD),
    100,
)
gpu.upload(numpy.full((N_FACETS, nodes), 200.0, dtype=numpy.float32))

t0 = time.perf_counter()
for i in range(N_STEPS):
    f = flux_at(i)
    routine.step_surface_newton(T_cpu, f, prop.se, prop.conductivity, twodz,
                                threshold=kalast.util.NEWTON_METHOD_THRESHOLD)
    routine.step_conduction(T_cpu, d_nodes, coefs)
cpu_s = time.perf_counter() - t0

t0 = time.perf_counter()
for i in range(N_STEPS):
    gpu.step(flux_at(i).astype(numpy.float32))
T_gpu = gpu.download().astype(numpy.float64)
gpu_s = time.perf_counter() - t0

d_surf = T_gpu[:, 0] - T_cpu[:, 0]
d_all = T_gpu - T_cpu
print("correctness, GPU f32 against CPU float64")
print(f"  surface T   mean |diff| {numpy.abs(d_surf).mean():.5f} K"
      f"   p99 {numpy.percentile(numpy.abs(d_surf), 99):.5f}"
      f"   max {numpy.abs(d_surf).max():.5f}")
print(f"  whole column mean |diff| {numpy.abs(d_all).mean():.5f} K"
      f"   max {numpy.abs(d_all).max():.5f}")
print(f"  CPU surface range {T_cpu[:, 0].min():.1f} - {T_cpu[:, 0].max():.1f} K"
      f"  (swing {numpy.ptp(T_cpu[:, 0]):.1f} K)")
print(f"  relative to the 0.1 K Newton threshold: "
      f"{numpy.abs(d_surf).max() / 0.1:.3f}x")

print(f"\nspeed over {N_STEPS:,} steps")
print(f"  CPU  {cpu_s:7.2f} s   {cpu_s / N_STEPS * 1000:7.3f} ms/step")
print(f"  GPU  {gpu_s:7.2f} s   {gpu_s / N_STEPS * 1000:7.3f} ms/step"
      f"   ({cpu_s / gpu_s:.1f}x)")
spin = 3243534
print(f"\nextrapolated to the {spin:,}-step Didymos spin-up:")
print(f"  CPU  {cpu_s / N_STEPS * spin / 3600:8.2f} h")
print(f"  GPU  {gpu_s / N_STEPS * spin / 3600:8.2f} h")
