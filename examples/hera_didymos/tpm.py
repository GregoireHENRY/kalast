#!/usr/bin/env python
"""Didymos thermophysical model, spun up over two solar orbits.

Produces the surface temperatures that `rad.py` turns into simulated TIRI
radiance at 2027-01-21T05:36 UTC -- the Dimorphos transit, chosen because a
mutual event is the interesting thermophysical case.

Two waves have to be resolved at once, which is what makes the setup awkward:

    diurnal   P = 2.26 h    ls1 = 1.01 cm    ls2pi = 6.3 cm
    seasonal  P = 700 d     ls1 = 86.7 cm    ls2pi = 5.45 m

The column must reach metres for the seasonal wave while resolving a
centimetre-scale diurnal one at the surface, and the explicit timestep is set
by the *thinnest* layer. `notes/2026-08-27_conduction_solvers/` measures the
options; this script exposes them so the same physics can be run three ways
and compared.

    GRID   = "uniform"     equal spacing            (stage 2, the baseline)
           = "geometric"   nonuniform.column        (stage 3, fewer nodes)

Run with BENCHMARK = True first: it times a few hundred steps and extrapolates,
rather than committing to a run that may take a day.
"""

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

# ---------------------------------------------------------------- settings
GRID = "geometric"  # "uniform" | "geometric"
# Step every facet as one array operation rather than looping in Python.
# Measured 13.5x on the conduction core; the per-facet path is kept because it
# is the reference the vectorised one is validated against.
VECTORISED = True
BENCHMARK = False  # time a short run and extrapolate instead of running it all
BENCHMARK_STEPS = 200

# Coarse first: 4 nodes per diurnal skin depth. Refine only once the coarse
# run is understood -- halving this doubles the node count and quarters the
# stable timestep.
NODES_PER_SKIN_DEPTH = 4
# How deep, in seasonal e-folding depths (ls2pi). 1.0 already reaches 5.45 m.
DEPTH_IN_SEASONAL = 1.0

N_ORBITS_SPINUP = 2  # 2 x 700 d, so the deep layers equilibrate
DT_SAFETY = 0.4  # fraction of the stability limit

MESH = (
    "/Users/gregoireh/data/mesh/didymos/"
    "g_01165mm_spc_obj_didy_0000n00000_v003_decimated_10k.obj"
)
KERNEL = "/Users/gregoireh/data/spice/hera/kernels/mk/hera_plan_local.tm"
# The meta-kernel's Didymos SPK is the Hera proximity phase only
# (2026-07-01 -> 2027-07-01), which cannot reach a two-orbit spin-up. This
# Horizons ephemeris spans 1999-2050. Loaded after the meta-kernel so it wins
# for Didymos throughout, which also avoids a discontinuity where the spin-up
# would otherwise cross from one ephemeris into another.
KERNEL_LONG = (
    "/Users/gregoireh/data/spice/hera/kernels/spk/"
    "didymos_hor_000101_500101_v01.bsp"
)
OUT = "out/hera_didymos/didymos_tpm"
T_INIT = 200.0

# ------------------------------------------------------------------ setup
spice.kclear()
spice.furnsh(KERNEL)
spice.furnsh(KERNEL_LONG)

didymos = kalast.entity.DIDYMOS
prop = kalast.tpm.properties.DIDYMOS
prop.se = STEFAN_BOLTZMANN * prop.emissivity
prop.compute_conductivity_diffusivity()
D = prop.diffusivity

mesh = kalast.mesh.Mesh(MESH)
mesh.flatten()
nface = len(mesh.facets)

et_end = spice.str2et("2027-01-21 05:36:00 UTC")
et_start = et_end - N_ORBITS_SPINUP * didymos.orbit_period

ls1_day = properties.skin_depth_1(D, didymos.spin_period)
ls2pi_season = properties.skin_depth_2pi(D, didymos.orbit_period)

print(f"Didymos  k={prop.conductivity:.4e} W/m/K  D={D:.4e} m2/s  "
      f"TI={prop.thermal_inertia:.0f}")
print(f"  diurnal  ls1={ls1_day * 100:.2f} cm")
print(f"  seasonal ls2pi={ls2pi_season:.2f} m")
print(f"  mesh {nface:,} facets")
print(f"  {spice.et2utc(et_start, 'C', 0)} -> {spice.et2utc(et_end, 'C', 0)}  "
      f"({N_ORBITS_SPINUP} orbits, "
      f"{(et_end - et_start) / didymos.spin_period:,.0f} rotations)")

# ------------------------------------------------------------------- grid
depth = DEPTH_IN_SEASONAL * ls2pi_season

if GRID == "uniform":
    dz = ls1_day / NODES_PER_SKIN_DEPTH
    z = numpy.arange(0.0, depth + dz, dz)
    max_dt = kalast.tpm.core.stability_maxdt(D, dz * dz)
else:
    # Geometric: first layer matched to the uniform case so the surface is
    # resolved the same way, growing to reach `depth` in far fewer nodes.
    z = nonuniform.column(
        ls1_day, m=NODES_PER_SKIN_DEPTH, n=5, b=depth / ls1_day
    )
    max_dt = routine.nonuniform_max_dt(z, D)

nx = z.size
dt = DT_SAFETY * max_dt
routine.print_resolution_report(z, D, didymos.spin_period, GRID)

nit = int(numpy.ceil((et_end - et_start) / dt)) + 1
print(f"  dt={dt:.2f} s ({didymos.spin_period / dt:.0f} steps/rotation), "
      f"{nit:,} steps total")

if GRID == "uniform":
    coefs = (routine.uniform_coefficients(z, dt),)
    conduct = kalast.tpm.core.conduction_1d
else:
    coefs = routine.nonuniform_coefficients(z, dt)
    conduct = kalast.tpm.core.conduction_1d_nonuniform

twodz0 = 2.0 * (z[1] - z[0])

# --------------------------------------------------------------- columns
positions = numpy.array(
    [mesh.facets[i].pos for i in range(nface)], dtype=numpy.float64
)
normals = numpy.array(
    [mesh.facets[i].normal for i in range(nface)], dtype=numpy.float64
)

if VECTORISED:
    # One (n_facets, n_nodes) array instead of n_facets Column objects.
    T = numpy.full((nface, nx), T_INIT, dtype=numpy.float64)
    d_nodes = numpy.full(nx, D, dtype=numpy.float64)
    coefs64 = tuple(numpy.asarray(c, dtype=numpy.float64) for c in coefs)
else:
    column = kalast.tpm.column.Column(z.astype(numpy.float32), prop, T_INIT)
    columns = [column.clone() for _ in range(nface)]


def sun_direction(et):
    """Unit vector to the Sun and its distance in AU, in the body frame."""
    (p_sun, _lt) = spice.spkpos("SUN", et, didymos.frame, "none", "DIDYMOS")
    return numpy.asarray(p_sun, dtype=numpy.float64) * 1e3


# ---------------------------------------------------------------- solve
n_steps = BENCHMARK_STEPS if BENCHMARK else nit
print(f"\n{'benchmarking' if BENCHMARK else 'running'} {n_steps:,} steps"
      f"{' (vectorised)' if VECTORISED else ' (per-facet loop)'}...")

t0 = time.perf_counter()
for it in range(n_steps):
    et = et_start + it * dt
    p_sun = sun_direction(et)

    if VECTORISED:
        v = p_sun[None, :] - positions
        d_sun = numpy.linalg.norm(v, axis=1)
        # cosine_incidence clamps negatives to zero (night); radiation_sun
        # does not, so the clamp has to be explicit here.
        cosi = numpy.einsum("ij,ij->i", normals, v / d_sun[:, None])
        numpy.maximum(cosi, 0.0, out=cosi)
        sflux = SOLAR_CONSTANT * (1.0 - prop.albedo) * cosi / (d_sun / AU) ** 2

        routine.step_surface_newton(
            T, sflux, prop.se, prop.conductivity, twodz0,
            threshold=kalast.util.NEWTON_METHOD_THRESHOLD,
        )
        routine.step_conduction(T, d_nodes, coefs64)
    else:
        p_sun32 = p_sun.astype(numpy.float32)
        for ii in range(nface):
            v_sun = p_sun32 - positions[ii].astype(numpy.float32)
            d_sun = numpy.linalg.norm(v_sun)
            cosi = kalast.math.cosine_incidence(
                (v_sun / d_sun).astype(numpy.float32),
                normals[ii].astype(numpy.float32),
            )
            sflux = kalast.tpm.core.radiation_sun(d_sun / AU, cosi, prop.albedo)

            c = columns[ii]
            c.t[0] = kalast.tpm.core.newton_method(
                c.t[0], sflux, prop.se, prop.conductivity, c.t[1], c.t[2], twodz0
            )
            c.t[1:-1] = conduct(c.t, c.d, *coefs)
            c.t[-1] = c.t[-2]

elapsed = time.perf_counter() - t0
per_step = elapsed / n_steps
print(f"{per_step * 1000:.2f} ms/step  ({1 / per_step:.1f} steps/s)")

if BENCHMARK:
    total_h = per_step * nit / 3600.0
    print(f"\nEXTRAPOLATED for the full {nit:,}-step run:")
    print(f"  {total_h:,.1f} hours  ({total_h / 24:.1f} days)")
    print(f"  grid={GRID}  {nx} nodes  dt={dt:.1f}s  {nface:,} facets")
    print("\nSet BENCHMARK = False to run it for real.")
else:
    temps = T[:, 0].copy() if VECTORISED else numpy.array([c.t[0] for c in columns])
    print(f"surface T: min {temps.min():.1f} K  max {temps.max():.1f} K  "
          f"mean {temps.mean():.1f} K")

    # Save the full column state, not just the surface: this is a spin-up,
    # and its whole purpose is the equilibrated subsurface profile that a
    # later, shorter, higher-fidelity run restarts from. Losing it would mean
    # repeating the spin-up.
    out = Path(OUT)
    out.mkdir(parents=True, exist_ok=True)

    state = T.copy() if VECTORISED else numpy.array([c.t for c in columns])
    pandas.DataFrame(state).to_csv(
        out / "tmp_state.csv", index=False, encoding="utf-8-sig"
    )
    pandas.DataFrame({"depth": z}).to_csv(
        out / "z.csv", index=False, encoding="utf-8-sig"
    )
    pandas.DataFrame({"facet": numpy.arange(nface), "t_surface": temps}).to_csv(
        out / "tmp_surf_final.csv", index=False, encoding="utf-8-sig"
    )
    pandas.DataFrame({
        "grid": [GRID], "nodes": [nx], "dt": [dt], "nface": [nface],
        "et_start": [et_start], "et_end": [et_end],
        "n_orbits_spinup": [N_ORBITS_SPINUP],
        "nodes_per_skin_depth": [NODES_PER_SKIN_DEPTH],
        "depth_in_seasonal": [DEPTH_IN_SEASONAL],
        "depth_m": [float(z[-1])], "mesh": [MESH],
        "thermal_inertia": [prop.thermal_inertia],
        "albedo": [prop.albedo], "emissivity": [prop.emissivity],
        "conductivity": [prop.conductivity], "diffusivity": [D],
        "physics": ["direct insolation only -- no eclipse shadowing, "
                    "mutual heating or self-heating (see notes section 7)"],
        "vectorised": [VECTORISED],
        "elapsed_s": [elapsed],
    }).to_csv(out / "settings.csv", index=False, encoding="utf-8-sig")
    print(f"wrote {out}/ (tmp_state {state.shape}, z, tmp_surf_final, settings)")

spice.kclear()
