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
# Which body's column to spin up. Both orbit the Sun on the same heliocentric
# ellipse, so the seasonal forcing is identical; what differs is the rotation
# period (Dimorphos is tidally locked at 11.37 h against Didymos's 2.26 h),
# which sets the diurnal skin depth and hence the whole grid.
BODY = "DIMORPHOS"  # "DIDYMOS" | "DIMORPHOS"

GRID = "geometric"  # "uniform" | "geometric"
# Step every facet as one array operation rather than looping in Python.
# Measured 13.5x on the conduction core; the per-facet path is kept because it
# is the reference the vectorised one is validated against.
VECTORISED = True
# "cpu"  -- the numpy path above
# "gpu"  -- the same physics in a compute shader, with insolation on the GPU
#           too, so nothing per-facet crosses the bus. Measured 11x at 10k and
#           23x at 3.1M facets, agreeing with the numpy path to 1.5e-05 K.
#           See notes/2026-09-01_gpu_tpm/.
BACKEND = "gpu"
BENCHMARK = False  # time a short run and extrapolate instead of running it all
BENCHMARK_STEPS = 200

# Coarse first: 4 nodes per diurnal skin depth. Refine only once the coarse
# run is understood -- halving this doubles the node count and quarters the
# stable timestep.
NODES_PER_SKIN_DEPTH = 4
# How deep, in seasonal e-folding depths (ls2pi). 1.0 already reaches 5.45 m.
DEPTH_IN_SEASONAL = 1.0

N_ORBITS_SPINUP = 3  # x 700 d
# Restart from a previously saved column state instead of an isothermal start.
# Phase 2 (the high-fidelity segment) needs this, and it is also how spin-up
# convergence is measured: continue for another orbit and compare.
RESTART_FROM = None  # e.g. "out/hera_didymos/didymos_tpm"
DT_SAFETY = 0.4  # fraction of the stability limit

MESH_BY_BODY = {
    "DIDYMOS": (
        "/Users/gregoireh/data/mesh/didymos/"
        "g_01165mm_spc_obj_didy_0000n00000_v003_decimated_10k.obj"
    ),
    "DIMORPHOS": (
        "/Users/gregoireh/data/mesh/dimorphos/"
        "g_00243mm_spc_obj_dimo_0000n00000_v004_decimated_10k.obj"
    ),
}
_UNUSED_MESH = (
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
OUT = f"out/hera_didymos/{BODY.lower()}_tpm"
T_INIT = 200.0

# ------------------------------------------------------------------ setup
spice.kclear()
spice.furnsh(KERNEL)
spice.furnsh(KERNEL_LONG)

body = getattr(kalast.entity, BODY)
prop = getattr(kalast.tpm.properties, BODY)
MESH = MESH_BY_BODY[BODY]
# Heliocentric period, for the seasonal wave. `DIMORPHOS.orbit_period` is its
# 11.9 h orbit *around Didymos*, which is a diurnal-scale forcing, not a
# seasonal one -- using it would build a grid centimetres deep.
ORBIT_PERIOD = kalast.entity.DIDYMOS.orbit_period
prop.se = STEFAN_BOLTZMANN * prop.emissivity
prop.compute_conductivity_diffusivity()
D = prop.diffusivity

mesh = kalast.mesh.Mesh(MESH)
mesh.flatten()
nface = len(mesh.facets)

et_end = spice.str2et("2027-01-21 05:36:00 UTC")
et_start = et_end - N_ORBITS_SPINUP * ORBIT_PERIOD

ls1_day = properties.skin_depth_1(D, body.spin_period)
ls2pi_season = properties.skin_depth_2pi(D, ORBIT_PERIOD)

print(f"{BODY}  k={prop.conductivity:.4e} W/m/K  D={D:.4e} m2/s  "
      f"TI={prop.thermal_inertia:.0f}")
print(f"  diurnal  ls1={ls1_day * 100:.2f} cm")
print(f"  seasonal ls2pi={ls2pi_season:.2f} m")
print(f"  mesh {nface:,} facets")
print(f"  {spice.et2utc(et_start, 'C', 0)} -> {spice.et2utc(et_end, 'C', 0)}  "
      f"({N_ORBITS_SPINUP} orbits, "
      f"{(et_end - et_start) / body.spin_period:,.0f} rotations)")

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
routine.print_resolution_report(z, D, body.spin_period, GRID)

nit = int(numpy.ceil((et_end - et_start) / dt)) + 1
print(f"  dt={dt:.2f} s ({body.spin_period / dt:.0f} steps/rotation), "
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
    if RESTART_FROM:
        T = pandas.read_csv(Path(RESTART_FROM) / "tmp_state.csv").to_numpy()
        z_prev = pandas.read_csv(Path(RESTART_FROM) / "z.csv")["depth"].to_numpy()
        if T.shape != (nface, nx) or not numpy.allclose(z_prev, z):
            raise SystemExit(
                f"restart state {T.shape} on a {len(z_prev)}-node grid does not "
                f"match this run's ({nface}, {nx}) -- the grid must be identical"
            )
        print(f"  restarted from {RESTART_FROM}: surface mean "
              f"{T[:, 0].mean():.2f} K, base mean {T[:, -1].mean():.2f} K")
    else:
        T = numpy.full((nface, nx), T_INIT, dtype=numpy.float64)
    d_nodes = numpy.full(nx, D, dtype=numpy.float64)
    coefs64 = tuple(numpy.asarray(c, dtype=numpy.float64) for c in coefs)
else:
    column = kalast.tpm.column.Column(z.astype(numpy.float32), prop, T_INIT)
    columns = [column.clone() for _ in range(nface)]


# Mission kernels cover 2026-07 to 2027-07. Didymos has a Horizons ephemeris
# and a rotation model reaching back to 2023, so its spin-up needs nothing
# special. Dimorphos has neither: both `spkpos` and `pxform` fail with
# SPKINSUFFDATA before coverage, so a two-orbit spin-up cannot be driven from
# kernels alone.
#
# Two approximations bridge that, and both are safe here:
#
# 1. The Sun's direction is taken relative to *Didymos* rather than
#    Dimorphos. They are 1.19 km apart at 1.5e8 km, so the direction differs
#    by 8e-9 rad -- eleven orders of magnitude below anything that matters.
#
# 2. Dimorphos's body-fixed frame is extended backwards as a uniform rotation
#    about its own spin axis at the true rate, anchored to the real
#    orientation at the study epoch. It is tidally locked, so its rotation
#    *is* uniform to good approximation, and anchoring at `et_end` means the
#    synthetic frame agrees with the kernels exactly where the two meet --
#    there is no discontinuity at handover to phase 2.
#
# The rotational phase would not matter even if it were wrong: after
# thousands of rotations the column retains no memory of its initial phase,
# and what the spin-up actually delivers is the deep seasonal field, which
# depends on the obliquity and the heliocentric distance history, not on
# where the body happens to be pointing.
SYNTHETIC_FRAME = BODY == "DIMORPHOS"

if SYNTHETIC_FRAME:
    _m_ref = spice.pxform("J2000", body.frame, et_end)
    _omega = 2.0 * numpy.pi / body.spin_period
    print(f"  {BODY} has no kernel coverage before "
          f"{spice.et2utc(spice.str2et('2026-07-01'), 'C', 0)}; using a "
          f"uniform tidally-locked frame anchored at the study epoch")


def body_frame(et):
    """J2000 -> body-fixed rotation at `et`."""
    if not SYNTHETIC_FRAME:
        return spice.pxform("J2000", body.frame, et)
    a = _omega * (et - et_end)
    ca, sa = numpy.cos(a), numpy.sin(a)
    # Rotation about the body-fixed +Z (the spin axis), applied after the
    # anchor so the axis is the body's own, not an inertial one.
    return numpy.array([[ca, sa, 0.0], [-sa, ca, 0.0], [0.0, 0.0, 1.0]]) @ _m_ref


def sun_direction(et):
    """Vector to the Sun in metres, in the body frame."""
    if not SYNTHETIC_FRAME:
        (p_sun, _lt) = spice.spkpos("SUN", et, body.frame, "none", BODY)
        return numpy.asarray(p_sun, dtype=numpy.float64) * 1e3
    (p_sun, _lt) = spice.spkpos("SUN", et, "J2000", "none", "DIDYMOS")
    return body_frame(et) @ (numpy.asarray(p_sun, dtype=numpy.float64) * 1e3)


# ---------------------------------------------------------------- solve
n_steps = BENCHMARK_STEPS if BENCHMARK else nit
_how = ("GPU" if BACKEND == "gpu"
        else "vectorised" if VECTORISED else "per-facet loop")
print(f"\n{'benchmarking' if BENCHMARK else 'running'} {n_steps:,} steps"
      f" ({_how})...")

gpu = None
if BACKEND == "gpu":
    from kalast._rs.tpm.gpu import GpuTpm

    gpu = GpuTpm(
        nface,
        numpy.asarray(coefs[0], numpy.float32),
        numpy.asarray(coefs[1], numpy.float32),
        numpy.asarray(d_nodes, numpy.float32),
        numpy.float32(prop.se),
        numpy.float32(prop.conductivity),
        numpy.float32(twodz0),
        numpy.float32(kalast.util.NEWTON_METHOD_THRESHOLD),
        100,
    )
    gpu.upload(T.astype(numpy.float32))
    # Static in the body frame, so uploaded once. This is what lets the
    # boundary flux be computed in the shader instead of streamed every step.
    gpu.set_geometry(positions.astype(numpy.float32), normals.astype(numpy.float32))
    absorbed_1au = numpy.float32(SOLAR_CONSTANT * (1.0 - prop.albedo))

t0 = time.perf_counter()
for it in range(n_steps):
    et = et_start + it * dt
    p_sun = sun_direction(et)

    if gpu is not None:
        gpu.step_sun([numpy.float32(x) for x in p_sun], absorbed_1au,
                     numpy.float32(AU))
    elif VECTORISED:
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

if gpu is not None:
    T = gpu.download().astype(numpy.float64)   # drains the queue
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
    temps = (T[:, 0].copy() if (VECTORISED or gpu is not None)
             else numpy.array([c.t[0] for c in columns]))
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
    # The state above is an array indexed by facet, so it only means anything
    # against the mesh it was spun up on. Re-decimating a shape model to the
    # same target leaves the facet count identical and every position
    # different, so a count check cannot catch a stale restart -- record what
    # the mesh actually was. `tpm_phase2.py` refuses to start on a mismatch.
    (out / "mesh_fingerprint.txt").write_text(str(hash(
        numpy.asarray(mesh.positions, dtype=numpy.float64).tobytes()
    )))
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
