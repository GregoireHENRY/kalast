#!/usr/bin/env python
"""Phase 2: high-fidelity TPM segment through the Dimorphos transit.

Phase 1 (`tpm.py`) spins the column up over two solar orbits with direct
insolation only. That is defensible for the deep, orbit-averaged field -- see
`notes/2026-08-27_conduction_solvers/` section 7.4, which also measures that
Didymos has no permanently shadowed regions, so no facet depends on
self-heating to be warm at all.

This restarts from that state a few rotations before 2027-01-21T05:36 UTC and
adds the physics that only matters near a mutual event. It runs **inside the
render loop**: eclipse shadowing comes from the GPU shadow map via
`sim.facet_shadow`, which is why the app drives the timestep rather than a
bare Python loop.

    before_render : place Didymos, Dimorphos and the Sun from spice
    (renderer)    : shadow pass builds the depth map from the Sun's view
    after_render  : read the occluded fraction per facet, take the TPM step

`SHADOWING = False` reproduces phase 1 physics over the same interval, so
differencing the two isolates exactly what the eclipse contributes. That
ablation is the point of the run, not a diagnostic of it.

Mutual and self heating are **not** included yet: both need the view-factor
work in section 9, whose current implementation returns zero for neighbouring
facets. Eclipse shadowing is done first because it is a multiplier on a term
that already exists, and it dominates the signal during a transit.
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
from kalast.util import AU, RPD, SOLAR_CONSTANT, STEFAN_BOLTZMANN

# ---------------------------------------------------------------- settings
# Ablation. "mutual" is the real configuration; the other two isolate what
# each shadowing term contributes:
#   "none"   -- phase 1 physics, direct insolation only
#   "self"   -- Didymos alone, so only its own concavities shadow it
#   "mutual" -- Dimorphos loaded too, adding the eclipse
SHADOW_MODE = "mutual"
SHADOWING = SHADOW_MODE != "none"
N_ROTATIONS = 6  # segment length before the study epoch
# Continue past it, to watch the eclipse scar fade. Didymos spins in 2.26 h
# while Dimorphos orbits in 11.37 h, so the shadow spot sweeps across the
# surface rather than dwelling: each facet is darkened for ~10-16 min, and
# whether that scar survives to the next rotation is a thermal-memory
# question the model can answer directly.
SPINS_AFTER = 3
RESTART_FROM = "out/hera_didymos/didymos_tpm_3orbit"
OUT = f"out/hera_didymos/didymos_phase2_{SHADOW_MODE}"

NODES_PER_SKIN_DEPTH = 4
DEPTH_IN_SEASONAL = 1.0
DT_SAFETY = 0.4

MESH_DIDY = (
    "/Users/gregoireh/data/mesh/didymos/"
    "g_01165mm_spc_obj_didy_0000n00000_v003_decimated_10k.obj"
)
MESH_DIMO = (
    "/Users/gregoireh/data/mesh/dimorphos/"
    "g_00243mm_spc_obj_dimo_0000n00000_v004_decimated_10k.obj"
)
KERNEL = "/Users/gregoireh/data/spice/hera/kernels/mk/hera_plan_local.tm"

# Deliberately NOT the Horizons ephemeris that `tpm.py` furnishes. It carries
# the same body id (-658030) as the mission's `didymos_flp_*.bsp`, and SPICE
# takes the last file loaded, so furnishing it here replaces the mission
# solution -- which disagrees by ~106 km on the Didymos position. The spin-up
# needs it (it starts in 2023, outside mission coverage) and does not care,
# since only the heliocentric direction matters there. This segment cares
# about the Didymos-Dimorphos vector to the metre, and the meta-kernel covers
# 2026-07 to 2027-07, so it needs nothing else.

# ------------------------------------------------------------------ setup
spice.kclear()
spice.furnsh(KERNEL)

didymos = kalast.entity.DIDYMOS
prop = kalast.tpm.properties.DIDYMOS
prop.se = STEFAN_BOLTZMANN * prop.emissivity
prop.compute_conductivity_diffusivity()
D = prop.diffusivity

et_study = spice.str2et("2027-01-21 05:36:00 UTC")
et_start = et_study - N_ROTATIONS * didymos.spin_period
et_end = et_study + SPINS_AFTER * didymos.spin_period

# Grid must match the spin-up exactly or the saved state cannot be reused.
ls1_day = properties.skin_depth_1(D, didymos.spin_period)
z = nonuniform.column(
    ls1_day, m=NODES_PER_SKIN_DEPTH, n=5,
    b=DEPTH_IN_SEASONAL * properties.skin_depth_2pi(D, didymos.orbit_period) / ls1_day,
)
nx = z.size
dt = DT_SAFETY * routine.nonuniform_max_dt(z, D)
n_steps = int(numpy.ceil((et_end - et_start) / dt))

T = pandas.read_csv(Path(RESTART_FROM) / "tmp_state.csv").to_numpy()
z_prev = pandas.read_csv(Path(RESTART_FROM) / "z.csv")["depth"].to_numpy()
if not numpy.allclose(z_prev, z):
    raise SystemExit("restart grid does not match this run's grid")
nface = T.shape[0]

coefs = tuple(
    numpy.asarray(c, dtype=numpy.float64)
    for c in routine.nonuniform_coefficients(z, dt)
)
d_nodes = numpy.full(nx, D, dtype=numpy.float64)
twodz0 = 2.0 * (z[1] - z[0])

print(f"phase 2, shadow mode = {SHADOW_MODE}")
print(f"  restart {RESTART_FROM}: surface mean {T[:, 0].mean():.2f} K")
print(f"  {spice.et2utc(et_start, 'C', 0)} -> {spice.et2utc(et_end, 'C', 0)}"
      f"  (study epoch {spice.et2utc(et_study, 'C', 0)}, "
      f"+{SPINS_AFTER} spins after)")
print(f"  {N_ROTATIONS} rotations, dt={dt:.2f}s, {n_steps:,} steps, "
      f"{nface:,} facets x {nx} nodes")

# -------------------------------------------------------------- rendering
app = kalast.app.App()
app.config.width = 512
app.config.height = 512
app.config.color_mode = 0
app.config.vsync = False
app.config.export_dir = f"{OUT}/frames"
# Per-facet occlusion from the shadow map, read in after_render. Off when
# ablating, so the run costs nothing it does not use.
app.config.access_shadow_map = SHADOWING

app.simulation.camera.projection.fovy = 20.0 * RPD

didy_mat = numpy.eye(4)
app.simulation.load_mesh(path=MESH_DIDY, mat=didy_mat, flatten=True)
if SHADOW_MODE == "mutual":
    dimo_mat = numpy.eye(4)
    app.simulation.load_mesh(path=MESH_DIMO, mat=dimo_mat, flatten=True)

mesh = app.simulation.bodies[0].mesh
if len(mesh.facets) != nface:
    raise SystemExit(
        f"renderer mesh has {len(mesh.facets):,} facets, restart state has "
        f"{nface:,} -- phase 1 and phase 2 must use the same mesh"
    )
positions = numpy.array(
    [mesh.facets[i].pos for i in range(nface)], dtype=numpy.float64
)
normals = numpy.array(
    [mesh.facets[i].normal for i in range(nface)], dtype=numpy.float64
)

history = {"et": [], "shadowed": [], "t_mean": [], "t_max": [], "t_min": []}
# Full surface field every SNAP_EVERY steps, so two runs can be differenced
# facet by facet as a function of time rather than only at the end.
SNAP_EVERY = 5
snapshots = {"et": [], "t": []}
state = {"t0": None, "done": False}


def before_render(sim, dt_frame):
    it = sim.state.iteration
    if it > n_steps:
        return

    et = et_start + it * dt

    # Body-fixed frame of Didymos: the TPM columns live in it, so the Sun and
    # Dimorphos are placed relative to Didymos rather than in an inertial frame.
    (p_sun, _lt) = spice.spkpos("SUN", et, didymos.frame, "none", "DIDYMOS")
    (p_dimo, _lt) = spice.spkpos("DIMORPHOS", et, didymos.frame, "none", "DIDYMOS")
    m_dimo = spice.pxform("DIMORPHOS_FIXED", didymos.frame, et)

    # The Sun drives the shadow map; keep it far away along its true direction
    # so the light is effectively collimated across the system.
    u_sun = numpy.asarray(p_sun) / numpy.linalg.norm(p_sun)
    sim.sun.pos = u_sun * 50.0
    sim.sun.look_anchor()

    sim.bodies[0].mat[:3, :3] = numpy.eye(3)
    sim.bodies[0].mat[:3, 3] = [0.0, 0.0, 0.0]
    if len(sim.bodies) > 1:
        sim.bodies[1].mat[:3, :3] = m_dimo
        sim.bodies[1].mat[:3, 3] = p_dimo

    sim.camera.pos = u_sun * 30.0
    sim.camera.dir = -u_sun
    sim.camera.anchor = [0.0, 0.0, 0.0]


def after_render(sim, dt_frame):
    it = sim.state.iteration
    if state["done"]:
        return
    if state["t0"] is None:
        state["t0"] = time.perf_counter()

    if it > n_steps:
        elapsed = time.perf_counter() - state["t0"]
        print(f"\n{n_steps:,} steps in {elapsed:.1f}s "
              f"({elapsed / max(n_steps, 1) * 1000:.2f} ms/step)")
        save()
        state["done"] = True
        import os
        os._exit(0)

    et = et_start + it * dt
    (p_sun, _lt) = spice.spkpos("SUN", et, didymos.frame, "none", "DIDYMOS")
    p_sun = numpy.asarray(p_sun, dtype=numpy.float64) * 1e3

    v = p_sun[None, :] - positions
    d_sun = numpy.linalg.norm(v, axis=1)
    cosi = numpy.einsum("ij,ij->i", normals, v / d_sun[:, None])
    numpy.maximum(cosi, 0.0, out=cosi)

    # Occluded fraction from the shadow map: 0 lit, 1 fully in umbra, quarter
    # steps between for facets straddling the shadow rim.
    if SHADOWING:
        frac = sim.facet_shadow(0)
        lit = 1.0 - numpy.asarray(frac, dtype=numpy.float64) if frac is not None \
            else numpy.ones(nface)
    else:
        lit = numpy.ones(nface)

    sflux = SOLAR_CONSTANT * (1.0 - prop.albedo) * cosi * lit / (d_sun / AU) ** 2

    routine.step_surface_newton(
        T, sflux, prop.se, prop.conductivity, twodz0,
        threshold=kalast.util.NEWTON_METHOD_THRESHOLD,
    )
    routine.step_conduction(T, d_nodes, coefs)

    if it % SNAP_EVERY == 0:
        snapshots["et"].append(et)
        snapshots["t"].append(T[:, 0].copy())

    if it % 10 == 0:
        # `lit < 1` counts facets the shadow map finds even partly occluded;
        # on the day side that is the eclipse.
        day = cosi > 0
        history["et"].append(et)
        history["shadowed"].append(int(((lit < 1.0) & day).sum()))
        history["t_mean"].append(float(T[:, 0].mean()))
        history["t_max"].append(float(T[:, 0].max()))
        history["t_min"].append(float(T[:, 0].min()))


def save():
    out = Path(OUT)
    out.mkdir(parents=True, exist_ok=True)
    pandas.DataFrame(T).to_csv(out / "tmp_state.csv", index=False,
                               encoding="utf-8-sig")
    pandas.DataFrame({"depth": z}).to_csv(out / "z.csv", index=False,
                                          encoding="utf-8-sig")
    pandas.DataFrame({"facet": numpy.arange(nface), "t_surface": T[:, 0]}).to_csv(
        out / "tmp_surf_final.csv", index=False, encoding="utf-8-sig")
    pandas.DataFrame(history).to_csv(out / "history.csv", index=False,
                                     encoding="utf-8-sig")
    numpy.save(out / "snap_et.npy", numpy.array(snapshots["et"]))
    numpy.save(out / "snap_tsurf.npy", numpy.array(snapshots["t"]))
    peak = max(history["shadowed"]) if history["shadowed"] else 0
    print(f"surface T: min {T[:, 0].min():.1f}  max {T[:, 0].max():.1f}  "
          f"mean {T[:, 0].mean():.1f} K")
    print(f"peak day-side facets shadowed in a sample: {peak:,}")
    print(f"wrote {out}/")


app.before_render = before_render
app.after_render = after_render
app.start()
