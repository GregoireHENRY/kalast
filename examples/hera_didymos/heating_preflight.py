#!/usr/bin/env python
"""Which bodies actually need mutual heating, decided before the long run.

Self and mutual heating are not equally worth their cost, and the split is not
symmetric even within one binary. Running the full ablation to find that out
costs hours; this answers it at a single epoch in about a minute, by computing
the view factors once and converting each of the four flux terms into a
linearised temperature response.

    eps_i     sum_j VF_ij eps_j sigma T_j^4     thermal re-emission
    (1 - A_i) sum_j VF_ij A_j S_j               scattered sunlight

each split into the body's own facets (self) and the companion's (mutual).

The temperature figures are an **upper bound**: `dT = dF / (4 eps sigma T^3)`
ignores conduction into the column, which damps the real response. Measured
against the stepped runs it reads about 2x high, which is the safe direction
for a screening test -- a term this says is negligible really is.

Run it for a new epoch, a new pair, or a new shape model before committing to
a production segment.
"""

import sys
from pathlib import Path

import numpy
import pandas
import spiceypy as spice

import kalast
from kalast.tpm import heating, properties, routine
from kalast.util import AU, SOLAR_CONSTANT, STEFAN_BOLTZMANN

EPOCH = "2027-01-21 05:36:00 UTC"
KERNEL = "/Users/gregoireh/data/spice/hera/kernels/mk/hera_plan_local.tm"
BODIES = ("DIDYMOS", "DIMORPHOS")
MESH = {
    "DIDYMOS": ("/Users/gregoireh/data/mesh/didymos/"
                "g_01165mm_spc_obj_didy_0000n00000_v003_decimated_10k.obj"),
    "DIMORPHOS": ("/Users/gregoireh/data/mesh/dimorphos/"
                  "g_00243mm_spc_obj_dimo_0000n00000_v004_decimated_10k.obj"),
}
RESTART = {
    "DIDYMOS": "out/hera_didymos/didymos_tpm_3orbit",
    "DIMORPHOS": "out/hera_didymos/dimorphos_tpm",
}
VF_RES, VF_CHUNK = 128, 2500

# A term worth carrying. Below this the answer does not move by more than the
# per-facet precision the shadow map already imposes near the terminator
# (+/-6.6 K on one flipped sample of four), so a smaller term is noise.
KEEP_MEAN, KEEP_MAX = 0.10, 1.0

spice.kclear()
spice.furnsh(KERNEL)
et = spice.str2et(EPOCH)
REF = BODIES[0]
REF_FRAME = getattr(kalast.entity, REF).frame

state = {}
for name in BODIES:
    prop = getattr(properties, name)
    prop.se = STEFAN_BOLTZMANN * prop.emissivity
    prop.compute_conductivity_diffusivity()
    t = pandas.read_csv(Path(RESTART[name]) / "tmp_state.csv").to_numpy()
    state[name] = {"prop": prop, "T": t[:, 0], "n": t.shape[0]}

app = kalast.app.App()
app.config.width = 256
app.config.height = 256
app.config.vsync = False
for i, name in enumerate(BODIES):
    app.simulation.load_mesh(path=MESH[name], mat=numpy.eye(4), flatten=True)
    mesh = app.simulation.bodies[i].mesh
    s = state[name]
    s["index"] = i
    if len(mesh.facets) != s["n"]:
        raise SystemExit(f"{name}: mesh has {len(mesh.facets):,} facets, "
                         f"restart state has {s['n']:,}")
    s["areas"] = numpy.array([mesh.facets[k].area for k in range(s["n"])])
    s["positions"] = numpy.array([mesh.facets[k].pos for k in range(s["n"])])
    s["normals"] = numpy.array([mesh.facets[k].normal for k in range(s["n"])])

NFACE = [state[n]["n"] for n in BODIES]
queue = list(BODIES)
rows, current = {}, [None]


def place(sim):
    (p, _lt) = spice.spkpos("DIMORPHOS", et, REF_FRAME, "none", REF)
    sim.bodies[0].mat[:3, :3] = numpy.eye(3)
    sim.bodies[0].mat[:3, 3] = [0.0, 0.0, 0.0]
    sim.bodies[1].mat[:3, :3] = spice.pxform("DIMORPHOS_FIXED", REF_FRAME, et)
    sim.bodies[1].mat[:3, 3] = p
    (ps, _lt) = spice.spkpos("SUN", et, REF_FRAME, "none", REF)
    u = numpy.asarray(ps) / numpy.linalg.norm(ps)
    sim.sun.pos = u * 50.0
    sim.sun.look_anchor()
    sim.camera.pos = u * 30.0
    sim.camera.dir = -u
    sim.camera.anchor = [0.0, 0.0, 0.0]


def incident(name):
    """Direct sunlight per facet, before albedo. No shadowing: this is a
    screening estimate and an unshadowed facet is the larger contribution."""
    s = state[name]
    (ps, _lt) = spice.spkpos("SUN", et, getattr(kalast.entity, name).frame,
                             "none", name)
    ps = numpy.asarray(ps, dtype=numpy.float64) * 1e3
    v = ps[None, :] - s["positions"]
    d = numpy.linalg.norm(v, axis=1)
    cosi = numpy.einsum("ij,ij->i", s["normals"], v / d[:, None])
    numpy.maximum(cosi, 0.0, out=cosi)
    return SOLAR_CONSTANT * cosi / (d / AU) ** 2


def before_render(sim, _dt):
    place(sim)
    if sim.state.iteration < 2:
        return
    if current[0] is None:
        if not queue:
            return
        name = queue[0]
        current[0] = (name, heating.ViewFactorBuilder(
            body=state[name]["index"], n_facets=state[name]["n"],
            resolution=VF_RES, chunk=VF_CHUNK))
        print(f"  building {name} view factors ...", flush=True)
    current[0][1].request(sim)


def report():
    inc = {n: incident(n) for n in BODIES}
    emit = [heating.emitted(state[n]["T"], state[n]["prop"].emissivity)
            for n in BODIES]
    refl = [state[n]["prop"].albedo * inc[n] for n in BODIES]

    print(f"\n{'='*74}\nheating pre-flight at {EPOCH}\n{'='*74}")
    verdict = {}
    for name in BODIES:
        vf, prop, T = rows[name], state[name]["prop"], state[name]["T"]
        me = state[name]["index"]
        print(f"\n{name}   ({state[name]['n']:,} facets, surface T "
              f"{routine.area_mean(T, state[name]['areas']):.1f} K "
              f"area-weighted)")
        print(f"  view factor row sums: self {vf.row_sums(me).mean():.4f}, "
              f"mutual {vf.row_sums(1 - me).mean():.4f}")
        bad = heating.pathological_facets(vf, me)
        if bad.size:
            print(f"  !! {bad.size} facet(s) with self VF > 0.5 -- suspect "
                  f"shape model: {bad[:6].tolist()}")
        print(f"  {'term':22s} {'flux W/m2':>10} {'dT mean':>9} {'dT p99':>9} "
              f"{'dT max':>9}")
        totals = {}
        for tag, label, own in (("ir", "re-emission", True),
                                ("vis", "reflection", True),
                                ("ir", "re-emission", False),
                                ("vis", "reflection", False)):
            src = emit if tag == "ir" else refl
            sel = [src[i] if (BODIES[i] == name) == own else None
                   for i in range(len(BODIES))]
            coef = prop.emissivity if tag == "ir" else 1.0 - prop.albedo
            f = coef * vf.dot(vf.stack(sel))
            dt = heating.delta_t_estimate(f, T, prop.emissivity)
            kind = "self" if own else "mutual"
            totals.setdefault(kind, numpy.zeros_like(dt))
            totals[kind] += dt
            print(f"  {kind + ' ' + label:22s} {f.mean():10.3f} "
                  f"{dt.mean():9.3f} {numpy.percentile(dt, 99):9.3f} "
                  f"{dt.max():9.3f}")
        verdict[name] = {k: (v.mean(), v.max()) for k, v in totals.items()}

    print(f"\n{'='*74}\nrecommendation  (keep a term above "
          f"{KEEP_MEAN} K mean or {KEEP_MAX} K max)\n{'='*74}")
    for name in BODIES:
        for kind in ("self", "mutual"):
            mean, mx = verdict[name][kind]
            keep = mean > KEEP_MEAN or mx > KEEP_MAX
            print(f"  {name:10s} {kind:7s} {mean:7.3f} K mean, {mx:7.3f} K max"
                  f"   -> {'KEEP' if keep else 'negligible, may drop'}")
    print("\nUpper bounds: conduction into the column damps the real response,\n"
          "measured about 2x. A term called negligible here really is.")


def after_render(sim, _dt):
    if current[0] is None:
        return
    name, builder = current[0]
    if not builder.collect(sim, NFACE):
        return
    rows[name] = builder.result
    queue.pop(0)
    current[0] = None
    if not queue:
        report()
        sys.stdout.flush()
        import os
        os._exit(0)


app.before_render = before_render
app.after_render = after_render
app.start()
