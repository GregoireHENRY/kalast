#!/usr/bin/env python
"""Deimos thermophysical spin-up, for the Mars swing-by TIRI reconstruction.

The Didymos phase 1 (`examples/hera_didymos/tpm.py`) with the body swapped.
Two things differ and both are the kind that go wrong silently:

- **The seasonal wave is Mars's year, not Deimos's orbit.** Deimos is tidally
  locked, so its 30.31 h "orbit_period" about Mars *is* its diurnal period.
  Using it for the seasonal skin depth would build a column millimetres deep.
  The heliocentric forcing is `MARS.orbit_period`, 687 days.
- **Thermal inertia is 20**, an order below Didymos's, so the diurnal skin
  depth is 0.42 cm against 1.0 cm and the grid is finer at the surface.

Kernel coverage was checked back three Mars years to 2019, and the
heliocentric distance repeats at 1.6597-1.6599 AU across them, which is a free
check that the period is the right one.
"""

import hashlib
import time
from pathlib import Path

import numpy
import pandas
import spiceypy as spice

import kalast
import kalast.tpm.nonuniform as nonuniform
import kalast.tpm.properties as properties
import kalast.tpm.routine as routine
from kalast.util import AU, SOLAR_CONSTANT, STEFAN_BOLTZMANN

BACKEND = "gpu"          # "gpu" | "cpu"
BENCHMARK = False
N_ORBITS_SPINUP = 4.7    # x 687 d = 8.8 years, and the most the kernels allow.
                         #
                         # **Three is not enough here**, unlike Didymos where it
                         # was validated. The surface converges almost at once
                         # -- 0.1 K by two orbits -- but the deep reservoir does
                         # not, and it is what sets night-side temperatures.
                         # Against a 4.7-orbit reference, the area-weighted
                         # deep-node error is:
                         #
                         #     2 orbits   +7.36 K   (max column 21.1 K)
                         #     3 orbits   +4.32 K   (max column 13.8 K)
                         #     4 orbits   +1.52 K   (max column  7.1 K)
                         #
                         # The Deimos ephemeris reaches back only 4.78 Mars
                         # years, to 2016-03-14; beyond that `spkpos` fails with
                         # SPKINSUFFDATA partway through the run rather than at
                         # setup. So 4.7 is the floor of what is achievable, and
                         # the residual there is of order 0.5-1 K by
                         # extrapolation -- worth stating in the product rather
                         # than claiming convergence.
NODES_PER_SKIN_DEPTH = 4
DEPTH_IN_SEASONAL = 1.0
DT_SAFETY = 0.4
T_INIT = 200.0

MESH = "/Users/gregoireh/data/mesh/deimos/deimos_k005_tho_v02.obj"
KERNEL = "/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm"
OUT = "out/hera_mars_swingby/deimos_tpm"
EPOCH = "2025-03-12 12:00:00 UTC"

spice.kclear()
spice.furnsh(KERNEL)

body = kalast.entity.DEIMOS
prop = kalast.tpm.properties.DEIMOS
prop.se = STEFAN_BOLTZMANN * prop.emissivity
prop.compute_conductivity_diffusivity()
D = prop.diffusivity
ORBIT_PERIOD = kalast.entity.MARS.orbit_period

mesh = kalast.mesh.Mesh(MESH)
mesh.flatten()
nface = len(mesh.facets)
positions = numpy.array([mesh.facets[i].pos for i in range(nface)]) * 1e3  # m
normals = numpy.array([mesh.facets[i].normal for i in range(nface)])
areas = numpy.array([mesh.facets[i].area for i in range(nface)])

ls1 = properties.skin_depth_1(D, body.spin_period)
z = nonuniform.column(
    ls1, m=NODES_PER_SKIN_DEPTH, n=5,
    b=DEPTH_IN_SEASONAL * properties.skin_depth_2pi(D, ORBIT_PERIOD) / ls1,
)
nx = z.size
twodz = 2.0 * (z[1] - z[0])
dt = DT_SAFETY * routine.nonuniform_max_dt(z, D)
coefs = routine.nonuniform_coefficients(z, dt)
d_nodes = numpy.full(nx, D)

et_end = spice.str2et(EPOCH)
et_start = et_end - N_ORBITS_SPINUP * ORBIT_PERIOD
n_steps = int(numpy.ceil((et_end - et_start) / dt))

routine.print_resolution_report(z, D, body.spin_period, label="deimos")
print(f"  {nface:,} facets, dt={dt:.1f}s, {n_steps:,} steps "
      f"({N_ORBITS_SPINUP} Mars years)")
print(f"  spin {body.spin_period / 3600:.2f} h, seasonal {ORBIT_PERIOD / 86400:.0f} d")

T = numpy.full((nface, nx), T_INIT)
gpu = None
if BACKEND == "gpu":
    from kalast._rs.tpm.gpu import GpuTpm

    gpu = GpuTpm(nface, numpy.asarray(coefs[0], numpy.float32),
                 numpy.asarray(coefs[1], numpy.float32),
                 numpy.asarray(d_nodes, numpy.float32),
                 numpy.float32(prop.se), numpy.float32(prop.conductivity),
                 numpy.float32(twodz),
                 numpy.float32(kalast.util.NEWTON_METHOD_THRESHOLD), 100)
    gpu.upload(T.astype(numpy.float32))
    gpu.set_geometry(positions.astype(numpy.float32), normals.astype(numpy.float32))
    absorbed_1au = numpy.float32(SOLAR_CONSTANT * (1.0 - prop.albedo))

steps = 200 if BENCHMARK else n_steps
print(f"\n{'benchmarking' if BENCHMARK else 'running'} {steps:,} steps "
      f"({'GPU' if gpu else 'numpy'})...")
t0 = time.perf_counter()
for it in range(steps):
    et = et_start + it * dt
    p_sun, _ = spice.spkpos("SUN", et, body.frame, "none", "DEIMOS")
    p_sun = numpy.asarray(p_sun) * 1e3
    if gpu is not None:
        gpu.step_sun([numpy.float32(x) for x in p_sun], absorbed_1au,
                     numpy.float32(AU))
    else:
        v = p_sun[None, :] - positions
        d_sun = numpy.linalg.norm(v, axis=1)
        cosi = numpy.einsum("ij,ij->i", normals, v / d_sun[:, None])
        numpy.maximum(cosi, 0.0, out=cosi)
        routine.step_surface_newton(
            T, SOLAR_CONSTANT * (1.0 - prop.albedo) * cosi / (d_sun / AU) ** 2,
            prop.se, prop.conductivity, twodz,
            threshold=kalast.util.NEWTON_METHOD_THRESHOLD)
        routine.step_conduction(T, d_nodes, coefs)
if gpu is not None:
    T = gpu.download().astype(numpy.float64)
elapsed = time.perf_counter() - t0
print(f"{elapsed / steps * 1000:.3f} ms/step  ({elapsed:.1f} s)")

if BENCHMARK:
    print(f"\nfull run would be {elapsed / steps * n_steps / 60:.1f} min")
    raise SystemExit

t = T[:, 0]
print(f"surface T: min {t.min():.1f}  max {t.max():.1f}  "
      f"mean {routine.area_mean(t, areas):.1f} K (area-weighted; "
      f"{t.mean():.1f} per facet)  emission {routine.emission_mean(t, areas):.1f} K")

out = Path(OUT)
out.mkdir(parents=True, exist_ok=True)
pandas.DataFrame(T).to_csv(out / "tmp_state.csv", index=False, encoding="utf-8-sig")
pandas.DataFrame({"depth": z}).to_csv(out / "z.csv", index=False, encoding="utf-8-sig")
pandas.DataFrame({"facet": numpy.arange(nface), "t_surface": t}).to_csv(
    out / "tmp_surf_final.csv", index=False, encoding="utf-8-sig")
(out / "mesh_fingerprint.txt").write_text(hashlib.sha256(
    numpy.asarray(mesh.positions, dtype=numpy.float64).tobytes()).hexdigest())
print(f"wrote {out}/")
