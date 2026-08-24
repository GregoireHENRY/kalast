#!/usr/bin/env python

import numpy  # noqa
import pandas  # noqa
import spiceypy as spice

import kalast
from kalast.util import AU, AU_KM, RPD, DPR, PI  # noqa
from kalast.entity import MARS, DIDYMOS, DIMORPHOS  # noqa


def tick(sim: kalast.app.simulation.Simulation, dt: float):
    global et

    if sim.state.is_paused:
        return

    if et - et0 > simu_dur:
        return

    sim.export_once()

    if sim.state.iteration > 0:
        et += simu_dt

    (p_sun, _lt) = spice.spkpos("sun", et, instr, "none", instr)
    (p_earth, _lt) = spice.spkpos("earth", et, instr, "none", instr)
    (p_didymos, _lt) = spice.spkpos("didymos", et, instr, "none", instr)
    (p_dimorphos, _lt) = spice.spkpos("dimorphos", et, instr, "none", instr)

    d_earth = numpy.linalg.norm(p_earth)
    d_dimorphos = numpy.linalg.norm(p_dimorphos)
    d_didymos = numpy.linalg.norm(p_didymos)

    m_didymos_tiri = spice.pxform("didymos_fixed", instr, et)
    m_dimorphos_tiri = spice.pxform("dimorphos_fixed", instr, et)

    sim.sun.pos = p_sun
    sim.sun.look_anchor()

    sim.bodies[0].mat[:3, :3] = m_didymos_tiri
    sim.bodies[0].mat[:3, 3] = p_didymos

    sim.bodies[1].mat[:3, :3] = m_dimorphos_tiri
    sim.bodies[1].mat[:3, 3] = p_dimorphos

    print(
        f"it={sim.state.iteration} d_earth={d_earth:.5e} d_didymos={d_didymos:.5e} d_dimorphos={d_dimorphos:.5e}"
    )


app = kalast.app.App()
app.config.width = 1020
app.config.height = 1020
app.config.color_mode = 0

app.config.shadow_normal_offset_scale = 2e-4
app.config.shadow_bias_scale = 1e-3
app.config.shadow_bias_minimum = 5e-4

spice.kclear()
spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_plan_local.tm")

et0 = spice.str2et("2026-11-05 00:00:00 UTC")
et = et0
simu_dur = 1.0 * 86400.0
simu_dt = 15.0 * 60.0

instr = "hera_afc-1"

(p_sun, _lt) = spice.spkpos("sun", et, instr, "none", instr)
(p_earth, _lt) = spice.spkpos("earth", et, instr, "none", instr)
(p_didymos, _lt) = spice.spkpos("didymos", et, instr, "none", instr)
(p_dimorphos, _lt) = spice.spkpos("dimorphos", et, instr, "none", instr)

d_sun = numpy.linalg.norm(p_sun)
d_sun_au = d_sun / AU_KM
d_earth = numpy.linalg.norm(p_earth)
d_didymos = numpy.linalg.norm(p_didymos)
d_dimorphos = numpy.linalg.norm(p_dimorphos)

m_didymos_tiri = spice.pxform("didymos_fixed", instr, et)
m_dimorphos_tiri = spice.pxform("dimorphos_fixed", instr, et)

print(f"d_sun_au={d_sun_au:.5f}AU, d_sun={d_sun:.5e}, p={p_sun}")
print(f"d_earth={d_earth:.5e}km p={p_earth}")
print(f"d_didymos={d_didymos:.5e}km p={p_didymos} ")
print(f"d_dimorphos={d_dimorphos:.5e}km p={p_dimorphos} ")
print()

app.simulation.sun.pos = p_sun
app.simulation.sun.up = [0.0, 1.0, 0.0]
app.simulation.sun.set_target([0.0, 0.0, 0.0])
app.simulation.sun.projection.side = 2.0e1
app.simulation.sun.projection.near = 1.0e7
app.simulation.sun.projection.far = 1.0e9

app.simulation.camera.pos = [0.0, 0.0, 0.0]
app.simulation.camera.up = [1.0, 0.0, 0.0]
app.simulation.camera.dir = [0.0, 0.0, 1.0]
app.simulation.camera.anchor = p_didymos
app.simulation.camera.set_control_none()
app.simulation.camera.projection.near = 1.0e1
app.simulation.camera.projection.far = 1.0e4
app.simulation.camera.projection.fovy = 5.5 * RPD
app.simulation.camera.up_world = [0.0, 1.0, 0.0]

mat = numpy.eye(4)
mat[:3, :3] = m_didymos_tiri
mat[0:3, 3] = p_didymos
app.simulation.load_mesh(
    path="/Users/gregoireh/data/mesh/didymos/g_01165mm_spc_obj_didy_0000n00000_v003_decimated_100k.obj",
    mat=mat,
    flatten=True,
)

mat = numpy.eye(4)
mat[:3, :3] = m_dimorphos_tiri
mat[0:3, 3] = p_dimorphos
app.simulation.load_mesh(
    path="/Users/gregoireh/data/mesh/dimorphos/g_00243mm_spc_obj_dimo_0000n00000_v004_decimated_100k.obj",
    mat=mat,
    flatten=True,
)

app.tick = tick
app.start()
