#!/usr/bin/env python
"""Phase 2: high-fidelity TPM segment through the Dimorphos transit.

Phase 1 (`tpm.py`) spins each body's column up over three solar orbits with
direct insolation only. That is defensible for the deep, orbit-averaged field
-- see `notes/2026-08-27_conduction_solvers/` section 7.4, which also measures
that Didymos has no permanently shadowed regions, so no facet depends on
self-heating to be warm at all.

This restarts *both* bodies from those states a few rotations before
2027-01-21T05:36 UTC and adds the physics that only matters near a mutual
event. It runs **inside the render loop**, because eclipse shadowing comes
from the GPU shadow map:

    before_render : place Didymos, Dimorphos and the Sun from spice
    (renderer)    : shadow pass builds the depth map from the Sun's view
    after_render  : read the occluded fraction per body, take the TPM steps

Both bodies are stepped, on their own grids. They do not share one: the grid
follows the diurnal skin depth, and Dimorphos is tidally locked at 11.37 h
against Didymos's 2.26 h, so its skin depth is sqrt(5) larger and its column
is coarser and its timestep longer. The run marches on Didymos's timestep and
sub-cycles nothing -- Dimorphos simply takes the same dt, which is well
inside its own stability limit.

`SHADOW_MODE` ablates the two shadowing terms:

    "none"   -- phase 1 physics, direct insolation only
    "self"   -- each body alone, so only its own concavities shadow it
    "mutual" -- both loaded, adding the eclipses

Mutual and self *heating* are still absent: both need the view-factor work in
section 9, whose current implementation returns zero for neighbouring facets.
Shadowing is done first because it is a multiplier on a term that already
exists, and section 7.5b measures it as the dominant effect during a transit.
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
SHADOW_MODE = "mutual"  # "none" | "self" | "mutual"
SHADOWING = SHADOW_MODE != "none"
# "self" means a body shadowed only by its own topography, which requires it
# to be alone in the scene -- the companion would otherwise occlude it. One
# body per run, so the ablation takes two.
SELF_BODY = "DIDYMOS"

N_ROTATIONS = 6  # Didymos spins before the study epoch
# Continue past it, to watch the eclipse scar fade. Didymos spins in 2.26 h
# while Dimorphos orbits in 11.37 h, so the shadow spot sweeps across the
# surface rather than dwelling: each facet is darkened for ~10-16 min, and
# section 7.5b measures the scar still at -10 K a full rotation later.
SPINS_AFTER = 3

NODES_PER_SKIN_DEPTH = 4
DEPTH_IN_SEASONAL = 1.0
DT_SAFETY = 0.4

OUT = ("out/hera_didymos/phase2_self_" + SELF_BODY.lower()
       if SHADOW_MODE == "self" else f"out/hera_didymos/phase2_{SHADOW_MODE}")

BODIES = ("DIDYMOS", "DIMORPHOS")
MESH = {
    "DIDYMOS": (
        "/Users/gregoireh/data/mesh/didymos/"
        "g_01165mm_spc_obj_didy_0000n00000_v003_decimated_10k.obj"
    ),
    "DIMORPHOS": (
        "/Users/gregoireh/data/mesh/dimorphos/"
        "g_00243mm_spc_obj_dimo_0000n00000_v004_decimated_10k.obj"
    ),
}
RESTART = {
    "DIDYMOS": "out/hera_didymos/didymos_tpm_3orbit",
    "DIMORPHOS": "out/hera_didymos/dimorphos_tpm",
}

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
ORBIT_PERIOD = didymos.orbit_period  # heliocentric, shared by both bodies

et_study = spice.str2et("2027-01-21 05:36:00 UTC")
et_start = et_study - N_ROTATIONS * didymos.spin_period
et_end = et_study + SPINS_AFTER * didymos.spin_period


def build(name):
    """Grid, restart state and stepping coefficients for one body.

    The grid is rebuilt exactly as `tpm.py` built it, and the saved `z.csv`
    is checked against it -- a restart onto a different grid is silently
    wrong, since the state is just an array of node temperatures.
    """
    body = getattr(kalast.entity, name)
    prop = getattr(kalast.tpm.properties, name)
    prop.se = STEFAN_BOLTZMANN * prop.emissivity
    prop.compute_conductivity_diffusivity()
    d = prop.diffusivity

    ls1 = properties.skin_depth_1(d, body.spin_period)
    z = nonuniform.column(
        ls1, m=NODES_PER_SKIN_DEPTH, n=5,
        b=DEPTH_IN_SEASONAL * properties.skin_depth_2pi(d, ORBIT_PERIOD) / ls1,
    )

    src = Path(RESTART[name])
    t = pandas.read_csv(src / "tmp_state.csv").to_numpy()
    z_prev = pandas.read_csv(src / "z.csv")["depth"].to_numpy()
    if not numpy.allclose(z_prev, z):
        raise SystemExit(f"{name}: restart grid does not match this run's grid")

    return {
        "name": name,
        "body": body,
        "prop": prop,
        "z": z,
        "T": t,
        "nface": t.shape[0],
        "max_dt": routine.nonuniform_max_dt(z, d),
        "twodz": 2.0 * (z[1] - z[0]),
        "d_nodes": numpy.full(z.size, d, dtype=numpy.float64),
    }


ACTIVE = BODIES if SHADOW_MODE != "self" else (SELF_BODY,)
# Built for *every* body even when only one is stepped: the timestep is set
# by the stiffest grid in the system, and it must not change between ablation
# runs or their difference would measure the timestep as well as the physics.
state = {n: build(n) for n in BODIES}

# One timestep for both, set by whichever body is stiffer -- Didymos, whose
# finer grid follows its shorter rotation.
dt = DT_SAFETY * min(s["max_dt"] for s in state.values())
n_steps = int(numpy.ceil((et_end - et_start) / dt))

for s in state.values():
    s["coefs"] = tuple(
        numpy.asarray(c, dtype=numpy.float64)
        for c in routine.nonuniform_coefficients(s["z"], dt)
    )

print(f"phase 2, shadow mode = {SHADOW_MODE}"
      + (f" ({SELF_BODY} alone)" if SHADOW_MODE == "self" else ""))
print(f"  {spice.et2utc(et_start, 'C', 0)} -> {spice.et2utc(et_end, 'C', 0)}"
      f"  (study epoch {spice.et2utc(et_study, 'C', 0)}, "
      f"+{SPINS_AFTER} spins after)")
print(f"  dt={dt:.2f}s, {n_steps:,} steps")
for n in ACTIVE:
    s = state[n]
    print(f"  {s['name']:10s} {s['nface']:,} facets x {s['z'].size} nodes, "
          f"own stability limit {s['max_dt']:.0f}s, "
          f"restart surface mean {s['T'][:, 0].mean():.2f} K")

# -------------------------------------------------------------- rendering
app = kalast.app.App()
app.config.width = 512
app.config.height = 512
app.config.vsync = False
app.config.export_dir = f"{OUT}/frames"
app.config.access_shadow_map = SHADOWING
app.simulation.camera.projection.fovy = 20.0 * RPD

# In "self" mode each body must be alone in the scene, or the other would
# occlude it. Two passes would be needed to do both; Didymos is the one the
# deliverable is about, so it is the one kept.
loaded = ACTIVE
for i, name in enumerate(loaded):
    app.simulation.load_mesh(path=MESH[name], mat=numpy.eye(4), flatten=True)
    mesh = app.simulation.bodies[i].mesh
    s = state[name]
    if len(mesh.facets) != s["nface"]:
        raise SystemExit(
            f"{name}: renderer mesh has {len(mesh.facets):,} facets, restart "
            f"state has {s['nface']:,} -- phase 1 and 2 must use one mesh"
        )
    s["index"] = i
    s["positions"] = numpy.array(
        [mesh.facets[k].pos for k in range(s["nface"])], dtype=numpy.float64
    )
    s["normals"] = numpy.array(
        [mesh.facets[k].normal for k in range(s["nface"])], dtype=numpy.float64
    )

history = {"et": []}
for name in ACTIVE:
    history[f"{name.lower()}_shadowed"] = []
    history[f"{name.lower()}_t_mean"] = []
SNAP_EVERY = 5
snapshots = {"et": [], **{n: [] for n in ACTIVE}}
# The snapshot cadence is coarse (SNAP_EVERY * dt = 280 s), which is fine for
# watching the scar decay but not for a data product: 280 s is a third of the
# time the shadow spot needs to cross a facet. So the step landing nearest the
# study epoch is captured separately, exactly.
at_epoch = {"dt": None, **{n: None for n in ACTIVE}}
clock = {"t0": None, "done": False}


# The scene is built in the frame of whichever body sits at the origin. In
# "mutual" that is Didymos, with Dimorphos placed relative to it. In "self"
# it is the single loaded body, in *its own* frame -- getting this wrong puts
# the shadow map and the TPM in different frames, so the facets the renderer
# reports as shadowed are not the facets the physics thinks are lit. That
# produced a "self" run colder than "mutual", which cannot happen, since
# mutual is self plus an extra occluder.
REF = ACTIVE[0] if SHADOW_MODE == "self" else "DIDYMOS"
REF_FRAME = getattr(kalast.entity, REF).frame


def before_render(sim, dt_frame):
    it = sim.state.iteration
    if it > n_steps:
        return
    et = et_start + it * dt

    # Body 0 sits at the origin unrotated and everything else moves around
    # it, so its facet positions -- which the TPM indexes -- stay static in
    # the renderer and no per-frame vertex upload is needed.
    (p_sun, _lt) = spice.spkpos("SUN", et, REF_FRAME, "none", REF)
    u_sun = numpy.asarray(p_sun) / numpy.linalg.norm(p_sun)

    # The shadow projection is orthographic, so this distance sets only the
    # view origin, not the shadow's divergence: the light is collimated, as
    # sunlight at 1 AU effectively is.
    sim.sun.pos = u_sun * 50.0
    sim.sun.look_anchor()

    sim.bodies[0].mat[:3, :3] = numpy.eye(3)
    sim.bodies[0].mat[:3, 3] = [0.0, 0.0, 0.0]
    if len(sim.bodies) > 1:
        (p_dimo, _lt) = spice.spkpos(
            "DIMORPHOS", et, REF_FRAME, "none", REF)
        sim.bodies[1].mat[:3, :3] = spice.pxform(
            "DIMORPHOS_FIXED", REF_FRAME, et)
        sim.bodies[1].mat[:3, 3] = p_dimo

    sim.camera.pos = u_sun * 30.0
    sim.camera.dir = -u_sun
    sim.camera.anchor = [0.0, 0.0, 0.0]


def step_body(sim, s, et):
    """Insolation, surface balance and conduction for one body."""
    # Each body's columns live in its own frame, so the Sun direction is
    # taken there rather than transformed from Didymos's.
    (p_sun, _lt) = spice.spkpos(
        "SUN", et, s["body"].frame, "none", s["name"])
    p_sun = numpy.asarray(p_sun, dtype=numpy.float64) * 1e3

    v = p_sun[None, :] - s["positions"]
    d_sun = numpy.linalg.norm(v, axis=1)
    cosi = numpy.einsum("ij,ij->i", s["normals"], v / d_sun[:, None])
    numpy.maximum(cosi, 0.0, out=cosi)

    if SHADOWING and "index" in s:
        frac = sim.facet_shadow(s["index"])
        lit = (1.0 - numpy.asarray(frac, dtype=numpy.float64)
               if frac is not None else numpy.ones(s["nface"]))
    else:
        lit = numpy.ones(s["nface"])

    prop = s["prop"]
    sflux = (SOLAR_CONSTANT * (1.0 - prop.albedo) * cosi * lit
             / (d_sun / AU) ** 2)

    routine.step_surface_newton(
        s["T"], sflux, prop.se, prop.conductivity, s["twodz"],
        threshold=kalast.util.NEWTON_METHOD_THRESHOLD,
    )
    routine.step_conduction(s["T"], s["d_nodes"], s["coefs"])
    return int(((lit < 1.0) & (cosi > 0)).sum())


def after_render(sim, dt_frame):
    it = sim.state.iteration
    if clock["done"]:
        return
    if clock["t0"] is None:
        clock["t0"] = time.perf_counter()

    if it > n_steps:
        elapsed = time.perf_counter() - clock["t0"]
        print(f"\n{n_steps:,} steps in {elapsed:.1f}s "
              f"({elapsed / max(n_steps, 1) * 1000:.2f} ms/step)")
        save()
        clock["done"] = True
        import os
        os._exit(0)

    et = et_start + it * dt
    shadowed = {n: step_body(sim, state[n], et) for n in ACTIVE}

    offset = et - et_study
    if at_epoch["dt"] is None or abs(offset) < abs(at_epoch["dt"]):
        at_epoch["dt"] = offset
        for n in ACTIVE:
            at_epoch[n] = state[n]["T"][:, 0].copy()

    if it % SNAP_EVERY == 0:
        snapshots["et"].append(et)
        for n in ACTIVE:
            snapshots[n].append(state[n]["T"][:, 0].copy())
    if it % 10 == 0:
        history["et"].append(et)
        for n in ACTIVE:
            history[f"{n.lower()}_shadowed"].append(shadowed[n])
            history[f"{n.lower()}_t_mean"].append(float(state[n]["T"][:, 0].mean()))


def save():
    out = Path(OUT)
    out.mkdir(parents=True, exist_ok=True)
    pandas.DataFrame(history).to_csv(out / "history.csv", index=False,
                                     encoding="utf-8-sig")
    numpy.save(out / "snap_et.npy", numpy.array(snapshots["et"]))
    print(f"epoch snapshot taken {at_epoch['dt']:+.2f} s from the study epoch")
    for n in ACTIVE:
        d = out / n.lower()
        d.mkdir(exist_ok=True)
        s = state[n]
        pandas.DataFrame(s["T"]).to_csv(d / "tmp_state.csv", index=False,
                                        encoding="utf-8-sig")
        pandas.DataFrame({"depth": s["z"]}).to_csv(d / "z.csv", index=False,
                                                   encoding="utf-8-sig")
        pandas.DataFrame(
            {"facet": numpy.arange(s["nface"]), "t_surface": s["T"][:, 0]}
        ).to_csv(d / "tmp_surf_final.csv", index=False, encoding="utf-8-sig")
        numpy.save(d / "snap_tsurf.npy", numpy.array(snapshots[n]))
        numpy.save(d / "tsurf_at_epoch.npy", at_epoch[n])
        print(f"{n:10s} surface T: min {s['T'][:, 0].min():6.1f}  "
              f"max {s['T'][:, 0].max():6.1f}  mean {s['T'][:, 0].mean():6.1f} K")
    print(f"wrote {out}/")


app.before_render = before_render
app.after_render = after_render
app.start()
