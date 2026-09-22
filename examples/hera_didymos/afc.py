#!/usr/bin/env python

import numpy
import spiceypy as spice

import kalast  # noqa
from kalast.app import App, Hud
from kalast.util import AU, AU_KM, RPD, DPR, PI  # noqa
from kalast.entity import MARS, DIDYMOS, DIMORPHOS  # noqa


app = App()
app.config.width = 1020
app.config.height = 1020
app.config.panels_folded = True

app.simulation.huds = [Hud("", size=16)]
app.simulation.camera.pos = [0.0, 0.0, 0.0]
app.simulation.camera.up = [1.0, 0.0, 0.0]
app.simulation.camera.dir = [0.0, 0.0, 1.0]
app.simulation.camera.projection.fovy = 5.5 * RPD

app.simulation.load_mesh(
    path="/Users/gregoireh/data/mesh/didymos/g_01165mm_spc_didy_v003.obj"
)
app.simulation.load_mesh(
    path="/Users/gregoireh/data/mesh/dimorphos/g_00243mm_spc_dimo_v004.obj",
)

spice.kclear()
spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_plan_local.tm")
et0 = spice.str2et("2026-11-05 00:00:00 UTC")
etf = spice.str2et("2027-04-30 00:00:00 UTC")
dur = etf - et0
dt = 15.0 * 60.0
instr = "hera_afc-1"

while app.running:
    et = et0 + (app.simulation.state.iteration * dt) % dur
    date = spice.timout(et, kalast.util.SPICE_PICTUR_3)

    # sim.export_once()

    (p_sun, _lt) = spice.spkpos("sun", et, instr, "none", instr)
    (p_earth, _lt) = spice.spkpos("earth", et, instr, "none", instr)
    (p_didymos, _lt) = spice.spkpos("didymos", et, instr, "none", instr)
    (p_dimorphos, _lt) = spice.spkpos("dimorphos", et, instr, "none", instr)
    d_earth = numpy.linalg.norm(p_earth)
    d_didymos = numpy.linalg.norm(p_didymos)
    d_dimorphos = numpy.linalg.norm(p_dimorphos)
    m_didymos_instr = spice.pxform("didymos_fixed", instr, et)
    m_dimorphos_instr = spice.pxform("dimorphos_fixed", instr, et)

    app.simulation.sun.pos = p_sun
    app.simulation.bodies[0].mat[:3, :3] = m_didymos_instr
    app.simulation.bodies[0].mat[:3, 3] = p_didymos
    app.simulation.bodies[1].mat[:3, :3] = m_dimorphos_instr
    app.simulation.bodies[1].mat[:3, 3] = p_dimorphos

    app.simulation.huds[
        0
    ].text = f"{date} Earth={d_earth:.3e}km Didymos={d_didymos:.3e}km Dimorphos={d_dimorphos:.3e}km"

    app.step()

spice.kclear()