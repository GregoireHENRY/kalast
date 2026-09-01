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

`HEATING` ablates the radiative coupling, which section 9's view factors now
make available:

    "none"   -- direct insolation only, as before
    "self"   -- a facet is warmed by its own body's other facets
    "mutual" -- and by the companion

Both terms are first order in the view factors: thermal re-radiation
`eps_i sum_j VF_ij eps_j sigma T_j^4` and scattered sunlight
`(1 - A_i) sum_j VF_ij A_j S_j`. See `kalast.tpm.heating` for why a second
bounce is dropped.

Shadowing was done first because it is a multiplier on a term that already
exists, and section 7.5b measures it as the dominant effect during a transit.
Heating is the smaller correction, and on Didymos a very small one -- its self
view-factor row sums are mean 0.0013. It is not small on Dimorphos, which sits
1.15 km from a body four times its size.
"""

import hashlib
import time
from pathlib import Path

import numpy
import pandas
import spiceypy as spice

import kalast
import kalast.tpm.heating as heating
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

HEATING = "mutual"  # "none" | "self" | "mutual"
# "mutual" heating needs both bodies loaded and stepped, so it is only
# available when SHADOW_MODE is "mutual" -- checked below rather than left to
# produce a silently self-only answer.

HEATING_BOUNCES = 5  # radiosity order. 1 drops light bounced twice; higher
                     # runs a Neumann series, one sparse matvec per extra
                     # bounce, converging geometrically at
                     # `reflectivity * rowsum`.
                     #
                     # Measured over one rotation against 12 bounces: a single
                     # bounce errs by 0.013 K mean and 0.37 K at worst on
                     # Dimorphos, and 0.0001 / 0.006 K on Didymos. Five is
                     # converged to four decimals. It costs almost nothing --
                     # 179 s against 175 s for the whole segment, because the
                     # view-factor rebuilds dominate and a bounce is one
                     # sparse matvec -- so there is no reason to leave the
                     # approximation in.

VF_RES = 128     # hemicube face resolution; 3e-5 closure error at 128
VF_TABLE = False  # precompute the mutual view factors over one synodic period
                 # and look them up, instead of rebuilding as the run goes.
                 #
                 # The rebuilds are the same handful of configurations over
                 # and over: the pair's relative geometry repeats with the
                 # synodic period, 2.821 h here, which is *not* the orbital
                 # period -- Didymos turns 5.0299 times per orbit, so the pair
                 # never repeats on the orbit alone. A 1,309-step segment does
                 # 262 rebuilds at a 12 deg cadence; the table does 30, once,
                 # and a run of any length pays no more.
                 #
                 # **Off, because it is not accurate enough here.** Against
                 # direct rebuilds it costs Dimorphos 0.66 K in the mean, 5.7 K
                 # at p99 and 19 K at worst -- 23 % of the 2.92 K effect being
                 # modelled. Doubling to 60 phases does not help (0.688 K),
                 # so the floor is not table density: it is that the pair does
                 # not repeat synodically as cleanly as the geometry suggests.
                 # Dimorphos's post-DART libration leaves a ~5.7 deg wobble
                 # that no amount of sampling removes.
                 #
                 # The reasoning that led here was wrong in an instructive
                 # way: 5.7 deg looked acceptable because the *rebuild cadence*
                 # tolerates 12 deg. But a cadence error is staleness that is
                 # zero at every rebuild and averages out, while this is a
                 # persistent offset that never comes back to zero.
                 #
                 # Kept because it is correct code and the assumption it rests
                 # on -- spin axis along the orbit normal, circular orbit, rigid
                 # locking -- would hold for a pair without the libration.
                 # Costs memory too: 30 phases is 1.16 GB, since the mutual
                 # block carries 4.98M nonzeros on Dimorphos against the self
                 # block's 251k.
VF_TABLE_PHASES = 30  # 12 deg apart, matching the measured rebuild cadence

VF_CHUNK = 2500  # rows per frame. Sets peak memory: 2,500 x 20,000 float32 is
                 # 200 MB, against 800 MB for all 10,000 at once.
VF_EVERY_DEG = 12.0  # rebuild the view factors every this many degrees of
                     # the fastest rotation in the system.
                     #
                     # Expressed as geometry, not as a step count, because a
                     # step count means nothing here: `dt` is set by the
                     # stiffest grid in the system, so the same VF_EVERY would
                     # sample the geometry differently on a different mesh, a
                     # different body, or a different depth grid. What the
                     # rebuild is chasing is the scene turning over.
                     #
                     # Self view factors are fixed in the body frame -- they do
                     # not move at all, measured. The mutual ones must be
                     # rebuilt, and the reason inverts the obvious argument:
                     # Dimorphos is tidally locked, so Didymos holds still in
                     # its sky and the solid angle barely changes. But Didymos
                     # *rotates underneath* in 2.26 h, so which of the
                     # primary's facets fill that angle, and whether they are
                     # day side or night side, turns over completely. The row
                     # sum is nearly blind to it: 4.6% between rebuilds while
                     # the temperature behind it swings hundreds of kelvin.
                     #
                     # So the fastest rotation in the system sets the cadence.
                     # For a tidally locked pair that is the primary's spin;
                     # the secondary's spin period equals its orbital period
                     # and is the slower of the two, so taking the minimum over
                     # the loaded bodies gets it right without special-casing.
                     #
                     # Measured over one Didymos rotation against a rebuild
                     # every 4.95 deg, on a heating effect of +2.92 K mean:
                     #
                     #   degrees  % of spin  ms/step  Dimorphos err mean/p99
                     #     4.95      1.38%      2925  reference
                     #    12.38      3.44%      1199  +0.014 / 0.136  <- default
                     #    24.75      6.88%       601  +0.058 / 0.519
                     #    61.88     17.19%       250  +0.113 / 1.290
                     #      360       100%        56  -0.631 / 4.731
                     #
                     # 12 deg is 0.5% of the effect; 62 deg is 3.9%, and
                     # holding them fixed for a whole spin is 22%. The full
                     # 1,309-step segment costs about 26 min at 12 deg.
                     #
                     # Didymos does not care at any cadence: its entire mutual
                     # term is 0.099 K on the worst facet of 10,000, because
                     # the view factor to a companion goes as (R/d)^2 and it
                     # sees a body a fifth of its own radius.
                     #
                     # Set to 0 to compute once and never rebuild.
                     #
                     # The 6.6 K maximum at 24.75 deg is not a convergence
                     # failure: it is one terminator facet whose 4-sample
                     # shadow fraction lands the other side of a quarter step.
                     # See 2026-08-31_view_factors on SAMPLES_PER_FACET.

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
if HEATING != "none":
    OUT += f"_heat_{HEATING}"

BODIES = ("DIDYMOS", "DIMORPHOS")
# A decimated proxy per body, used for two things at once: the shadow map, and
# the *columns* of the view-factor hemicube. Rows stay at the real resolution.
#
# What fills a hemicube is a far-field quantity -- the companion subtends tens
# of degrees, so its day/night structure is what matters, not which metre-scale
# facet a ray struck -- exactly the argument that makes a shadow proxy safe.
# The accumulator is `batch * n_columns`, so without this, putting Didymos on
# the 100k mesh takes a rebuild from 6 s to 110 s, and the 3.1M mesh is
# unreachable.
#
# None keeps the real mesh as its own columns, which is right for a body whose
# mesh is already coarse.
PROXY = {
    "DIDYMOS": (
        "/Users/gregoireh/data/mesh/didymos/"
        "g_01165mm_spc_obj_didy_0000n00000_v003_decimated_10k.obj"
    ),
    "DIMORPHOS": None,
}

MESH = {
    "DIDYMOS": (
        "/Users/gregoireh/data/mesh/didymos/"
        "g_01165mm_spc_obj_didy_0000n00000_v003_decimated_100k.obj"
    ),
    "DIMORPHOS": (
        "/Users/gregoireh/data/mesh/dimorphos/"
        "g_00243mm_spc_obj_dimo_0000n00000_v004_decimated_10k.obj"
    ),
}
RESTART = {
    "DIDYMOS": "out/hera_didymos/didymos_tpm_3orbit_v2",
    "DIMORPHOS": "out/hera_didymos/dimorphos_tpm_v2",
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

# Turn the geometric cadence into a step count, now that dt is known. The
# fastest rotation in the system is what the mutual view factors have to keep
# up with; a tidally locked secondary spins at its orbital period, which is the
# slower one, so the minimum over the loaded bodies is the right choice.
FASTEST_SPIN = min(s["body"].spin_period for s in state.values())
VF_EVERY = (0 if not VF_EVERY_DEG else
            max(1, int(round(VF_EVERY_DEG / 360.0 * FASTEST_SPIN / dt))))

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
if HEATING == "mutual":
    print(f"  view factors rebuilt every {VF_EVERY} steps "
          f"= {VF_EVERY * dt:.0f} s = {VF_EVERY_DEG:.1f} deg "
          f"({VF_EVERY * dt / FASTEST_SPIN:.2%}) of the "
          f"{FASTEST_SPIN / 3600:.2f} h fastest spin")
for n in ACTIVE:
    s = state[n]
    print(f"  {s['name']:10s} {s['nface']:,} facets x {s['z'].size} nodes, "
          f"own stability limit {s['max_dt']:.0f}s, "
          f"restart surface mean {s['T'][:, 0].mean():.2f} K per facet")

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
    app.simulation.load_mesh(path=MESH[name], mat=numpy.eye(4), flatten=True,
                             shadow_path=PROXY.get(name))
    mesh = app.simulation.bodies[i].mesh
    s = state[name]
    if len(mesh.facets) != s["nface"]:
        raise SystemExit(
            f"{name}: renderer mesh has {len(mesh.facets):,} facets, restart "
            f"state has {s['nface']:,} -- phase 1 and 2 must use one mesh"
        )
    s["index"] = i
    # A restart state is an array indexed by facet, so it is only valid for
    # the mesh it was spun up on. The facet *count* is not enough to check
    # that: re-decimating a shape model to the same target leaves the count
    # identical and every position different, and the run would proceed
    # silently against the wrong geometry. Fingerprint the positions.
    fp = hashlib.sha256(
        numpy.asarray(mesh.positions, dtype=numpy.float64).tobytes()
    ).hexdigest()
    fp_file = Path(RESTART[name]) / "mesh_fingerprint.txt"
    if fp_file.exists():
        if fp_file.read_text().strip() != fp:
            raise SystemExit(
                f"{name}: the shape model has changed since the spin-up in "
                f"{RESTART[name]} was saved. The restart state is indexed by "
                f"facet and is no longer valid -- re-run phase 1 (tpm.py) for "
                f"this body, or restore the previous mesh."
            )
    else:
        # Deliberately not written here. A fingerprint taken now would record
        # whatever mesh is loaded *today* and certify it against a state spun
        # up on some other one -- blessing exactly the mismatch this guards
        # against. Only `tpm.py` may write it, when it saves the state.
        print(f"  {name}: WARNING, no mesh fingerprint alongside the restart "
              f"state in {RESTART[name]}. It predates this check, so whether "
              f"it matches the current mesh cannot be verified. Re-run phase 1 "
              f"if the shape model has changed.")
    s["positions"] = numpy.array(
        [mesh.facets[k].pos for k in range(s["nface"])], dtype=numpy.float64
    )
    s["normals"] = numpy.array(
        [mesh.facets[k].normal for k in range(s["nface"])], dtype=numpy.float64
    )
    # For reporting: a facet-count mean is not the mean over the surface.
    s["areas"] = numpy.array(
        [mesh.facets[k].area for k in range(s["nface"])], dtype=numpy.float64
    )
    # Emitted flux has to be expressed over whatever the hemicube columns are.
    # With a proxy those are its facets, so each takes the area-weighted mean
    # of the real facets nearest it.
    if PROXY.get(name):
        pm = kalast.mesh.Mesh(PROXY[name])
        pm.flatten()
        s["proxy"] = heating.FacetMap(
            s["positions"],
            numpy.array([pm.facets[k].pos for k in range(len(pm.facets))]),
        )
        print(f"  {name}: view-factor columns from a {len(pm.facets):,}-facet "
              f"proxy ({s['nface'] / len(pm.facets):.0f} real facets each)")
    else:
        s["proxy"] = None

history = {"et": []}
for name in ACTIVE:
    history[f"{name.lower()}_shadowed"] = []
    history[f"{name.lower()}_t_mean"] = []
    history[f"{name.lower()}_q_extra"] = []
SNAP_EVERY = 5
snapshots = {"et": [], **{n: [] for n in ACTIVE}}
# The snapshot cadence is coarse (SNAP_EVERY * dt = 280 s), which is fine for
# watching the scar decay but not for a data product: 280 s is a third of the
# time the shadow spot needs to cross a facet. So the step landing nearest the
# study epoch is captured separately, exactly.
at_epoch = {"dt": None, **{n: None for n in ACTIVE}}
clock = {"t0": None, "done": False}
rate = kalast.util.Rate()


# The scene is built in the frame of whichever body sits at the origin. In
# "mutual" that is Didymos, with Dimorphos placed relative to it. In "self"
# it is the single loaded body, in *its own* frame -- getting this wrong puts
# the shadow map and the TPM in different frames, so the facets the renderer
# reports as shadowed are not the facets the physics thinks are lit. That
# produced a "self" run colder than "mutual", which cannot happen, since
# mutual is self plus an extra occluder.
REF = ACTIVE[0] if SHADOW_MODE == "self" else "DIDYMOS"
REF_FRAME = getattr(kalast.entity, REF).frame



# --------------------------------------------------- view-factor driver
HEATING_ON = HEATING != "none"
if HEATING == "mutual" and SHADOW_MODE != "mutual":
    raise SystemExit(
        "HEATING='mutual' needs both bodies in the scene and stepped, which "
        f"SHADOW_MODE='{SHADOW_MODE}' does not do. Set SHADOW_MODE='mutual', "
        "or HEATING='self'."
    )

NFACE = [state[n]["nface"] for n in loaded]


class ViewFactorDriver:
    """Builds one body's rows at a time, a chunk per frame.

    Only one hemicube request is in flight at a time -- the simulation holds a
    single request slot -- so the bodies are worked through in order, and each
    body's rows arrive `VF_CHUNK` at a time. The TPM does not step while this
    is running, which is why the step index is its own counter rather than
    `sim.state.iteration`.
    """

    def __init__(self):
        self.rows = {}
        self.queue = []
        self.current = None

    def start(self):
        self.queue = list(ACTIVE)
        self.current = None

    @property
    def busy(self):
        return self.current is not None or bool(self.queue)

    @property
    def ready(self):
        return len(self.rows) == len(ACTIVE)

    @property
    def status(self):
        """What to show while a rebuild is running.

        The rebuild spans many frames and the TPM does not step during it, so
        without this the window simply looks frozen every fifth step. Naming
        the body and its percentage makes the pause legible as work.
        """
        if self.current is None:
            return "computing view factors" if self.queue else ""
        name, builder = self.current
        return (f"computing view factors: {name} "
                f"{builder.progress * 100:.0f}%"
                + (f" (+{len(self.queue) - 1} queued)"
                   if len(self.queue) > 1 else ""))

    def request(self, sim):
        if self.current is None:
            if not self.queue:
                return
            name = self.queue[0]
            self.current = (name, heating.ViewFactorBuilder(
                body=state[name]["index"], n_facets=state[name]["nface"],
                resolution=VF_RES, chunk=VF_CHUNK,
            ))
        self.current[1].request(sim)

    def collect(self, sim):
        if self.current is None:
            return
        name, builder = self.current
        if builder.collect(sim, NFACE):
            self.rows[name] = builder.result
            self.queue.pop(0)
            self.current = None


vf = ViewFactorDriver()

SYNODIC = heating.synodic_period([state[n]["body"].spin_period for n in BODIES])
table = (heating.SynodicTable(list(loaded), SYNODIC, et_start, VF_TABLE_PHASES)
         if (HEATING_ON and VF_TABLE) else None)
# Which (phase, body) the table build is on. `None` once it is finished.
build = {"phase": 0} if table is not None else None

if HEATING_ON and table is None:
    vf.start()
elif table is not None:
    print(f"  synodic period {SYNODIC / 3600:.3f} h; precomputing "
          f"{VF_TABLE_PHASES} phases ({360 / VF_TABLE_PHASES:.0f} deg apart), "
          f"{VF_TABLE_PHASES * len(ACTIVE)} hemicube passes", flush=True)
    vf.start()


def placement_epoch():
    """Where to put the bodies this frame.

    While the table is building this is the epoch the *entry* stands for, not
    the run's current time -- the whole point is to sample geometry the run
    has not reached yet.
    """
    if build is not None:
        return table.epoch_for(build["phase"])
    return et_start + step["n"] * dt


def rows_for(name, et):
    """View factors for `name` at `et`, however they were obtained."""
    return table.rows(name, et) if table is not None else vf.rows[name]

step = {"n": 0}


def before_render(sim, dt_frame):
    """Place the scene for this step, and keep the view-factor build fed."""
    if step["n"] > n_steps:
        return
    et = placement_epoch()

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

    # Progress in the window's corner, so a multi-hour run can be watched
    # without tailing the log. The TPM step is the number worth showing, not
    # `sim.state.iteration`: they diverge, because a view-factor rebuild spans
    # several frames during which the physics does not advance.
    if build is not None:
        sim.hud = (f"view factors {build['phase']}/{VF_TABLE_PHASES} phases")
        vf.request(sim)
        return
    # One line each: the window is 512 px wide, and a single line of all of
    # this overflows it and gets clipped at the right edge.
    lines = [f"{step['n']}/{n_steps} it   {rate}"]
    if rate.per_second:
        lines.append(f"eta {rate.eta(n_steps - step['n'])}")
    if HEATING_ON and vf.busy:
        lines.append(vf.status)
    sim.hud = "\n".join(lines)

    # Requested after the bodies are placed: the hemicube renders the scene
    # this callback just positioned, so the mutual block belongs to this
    # epoch and not the previous one.
    if HEATING_ON and vf.busy:
        vf.request(sim)


def insolation(sim, s, et):
    """Direct sunlight on each facet, before any radiative coupling.

    Returns the *incident* flux rather than the absorbed one: the scattered
    term needs what arrives at a facet before its own albedo takes a share,
    since that is what it reflects onward.
    """
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

    incident = SOLAR_CONSTANT * cosi * lit / (d_sun / AU) ** 2
    return {"cosi": cosi, "lit": lit, "incident": incident}


def coupling(ins, et_now):
    """Absorbed flux added by radiative coupling, per body.

    Both terms are first order and use the surface temperature at the start
    of the step, which is what makes this a single matvec rather than an
    implicit solve. `HEATING == "self"` differs from `"mutual"` only in which
    bodies are allowed to contribute -- the view factors are the same rows,
    and the companion's columns are simply left at zero.
    """
    out = {n: None for n in ACTIVE}
    if not (HEATING_ON and (table is not None or vf.ready)):
        return out

    fine = {}
    for name in loaded:
        s, prop = state[name], state[name]["prop"]
        fine[name] = (heating.emitted(s["T"][:, 0], prop.emissivity),
                      prop.albedo * ins[name]["incident"])

    for name in ACTIVE:
        rows = rows_for(name, et_now)
        keep = [name] if HEATING == "self" else list(loaded)
        e, r = [], []
        for bname in loaded:
            if bname not in keep:
                e.append(None)
                r.append(None)
                continue
            ei, ri = fine[bname]
            # A proxy supplies the columns only for the *other* body: the
            # requesting body's own columns stay at full resolution, since a
            # hemicube origin sitting on the real surface would otherwise find
            # itself buried inside the coarse hull. So which array to send
            # depends on whose rows these are.
            if bname != name and state[bname]["proxy"] is not None:
                ei = state[bname]["proxy"].aggregate(ei, state[bname]["areas"])
                ri = state[bname]["proxy"].aggregate(ri, state[bname]["areas"])
            e.append(ei)
            r.append(ri)
        prop = state[name]["prop"]
        out[name] = heating.absorbed(
            rows, rows.stack(e), rows.stack(r),
            emissivity=prop.emissivity, albedo=prop.albedo,
            bounces=HEATING_BOUNCES, body=state[name]["index"],
        )
    return out


def after_render(sim, dt_frame):
    if clock["done"]:
        return
    if clock["t0"] is None:
        clock["t0"] = time.perf_counter()

    # A view-factor build owns the frame: the TPM must not advance while the
    # rows it is about to use are half assembled.
    # Building the synodic table owns the frame until it is done.
    if build is not None:
        vf.collect(sim)
        if vf.busy:
            return
        for name in ACTIVE:
            table.store(name, build["phase"], vf.rows[name])
        build["phase"] += 1
        if build["phase"] < VF_TABLE_PHASES:
            vf.start()
            return
        print(f"  synodic table built: {table.nnz():,} nonzeros, "
              f"{table.nnz() * 8 / 1e9:.2f} GB", flush=True)
        globals()["build"] = None
        return

    if HEATING_ON and vf.busy:
        vf.collect(sim)
        if vf.busy:
            return
        print(f"  view factors ready at step {step['n']:,}: "
              + ", ".join(
                  f"{n} {vf.rows[n].nnz:,} nnz "
                  f"(self {vf.rows[n].row_sums(state[n]['index']).mean():.4f}"
                  + ("" if len(loaded) < 2 else
                     f", mutual {vf.rows[n].row_sums(1 - state[n]['index']).mean():.4f}")
                  + ")"
                  for n in ACTIVE), flush=True)

    it = step["n"]
    if it > n_steps:
        elapsed = time.perf_counter() - clock["t0"]
        print(f"\n{n_steps:,} steps in {elapsed:.1f}s "
              f"({elapsed / max(n_steps, 1) * 1000:.2f} ms/step)")
        save()
        clock["done"] = True
        # os._exit skips the interpreter's shutdown, buffered stdout included,
        # so a piped run loses everything save() just printed.
        import os, sys
        sys.stdout.flush()
        os._exit(0)

    et = et_start + it * dt
    rate.tick()

    # Insolation for every body before any of them steps: with heating on, a
    # body's surface feeds the other's boundary condition, so stepping one
    # first would advance it against a stale companion.
    ins = {n: insolation(sim, state[n], et) for n in ACTIVE}
    extra = coupling(ins, et)

    shadowed = {}
    for name in ACTIVE:
        s, prop = state[name], state[name]["prop"]
        flux = (1.0 - prop.albedo) * ins[name]["incident"]
        if extra[name] is not None:
            flux = flux + extra[name]

        routine.step_surface_newton(
            s["T"], flux, prop.se, prop.conductivity, s["twodz"],
            threshold=kalast.util.NEWTON_METHOD_THRESHOLD,
        )
        routine.step_conduction(s["T"], s["d_nodes"], s["coefs"])
        shadowed[name] = int(
            ((ins[name]["lit"] < 1.0) & (ins[name]["cosi"] > 0)).sum())

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
            history[f"{n.lower()}_t_mean"].append(routine.area_mean(
                state[n]["T"][:, 0], state[n]["areas"]))
            history[f"{n.lower()}_q_extra"].append(
                0.0 if extra[n] is None else float(extra[n].mean()))

    step["n"] += 1

    # A periodic rebuild picks the companion up where it has moved to. The
    # self block is fixed in the body frame and is rebuilt with it only
    # because one hemicube pass produces both.
    if (table is None and HEATING == "mutual" and VF_EVERY
            and step["n"] % VF_EVERY == 0):
        vf.start()


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
              f"max {s['T'][:, 0].max():6.1f}  "
              f"mean {routine.area_mean(s['T'][:, 0], s['areas']):6.1f} K "
              f"(area-weighted; {s['T'][:, 0].mean():.1f} per facet)  "
              f"emission {routine.emission_mean(s['T'][:, 0], s['areas']):6.1f} K")
    print(f"wrote {out}/")


app.before_render = before_render
app.after_render = after_render
app.start()
