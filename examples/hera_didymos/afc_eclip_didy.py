#!/usr/bin/env python

import numpy
import spiceypy as spice

import kalast  # noqa
from kalast.app import App, Hud
from kalast.util import AU, AU_KM, RPD, DPR, PI  # noqa
from kalast.entity import MARS, DIDYMOS, DIMORPHOS  # noqa

import os
from pathlib import Path

# Where the data is. Set the variables to your own locations, or keep the
# defaults; res/README.md says how to get each set.
HERA = Path(os.environ.get("KALAST_HERA", "~/data/spice/hera")).expanduser()  # HERA.zip, unpacked


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
    path=f"{HERA}/kernels/dsk/g_01165mm_spc_obj_didy_0000n00000_v003.obj"
)
app.simulation.load_mesh(
    path=f"{HERA}/kernels/dsk/g_00243mm_spc_obj_dimo_0000n00000_v004.obj",
)

spice.kclear()
spice.furnsh(f"{HERA}/kernels/mk/hera_plan_local.tm")
et0 = spice.str2et("2026-11-05 00:00:00 UTC")
etf = spice.str2et("2027-04-30 00:00:00 UTC")
dur = etf - et0
dt = 15.0 * 60.0
frame = "eclipj2000"
center = "didymos"
instr = "hera_afc-1"
instr_dir = numpy.array([0.0, 0.0, 1.0])
instr_up = numpy.array([1.0, 0.0, 0.0])


while app.running:
    if app.simulation.state.is_paused:
        app.step()
        continue

    et = et0 + (app.simulation.state.iteration * dt) % dur
    date = spice.timout(et, kalast.util.SPICE_PICTUR_3)

    # sim.export_once()

    (p_sun, _lt) = spice.spkpos("sun", et, frame, "none", center)
    (p_dimorphos, _lt) = spice.spkpos("dimorphos", et, frame, "none", center)
    (p_instr, _lt) = spice.spkpos(instr, et, frame, "none", center)
    (p_earth_instr, _lt) = spice.spkpos("earth", et, frame, "none", instr)
    (p_dimorphos_instr, _lt) = spice.spkpos("dimorphos", et, frame, "none", instr)
    d_earth_instr = numpy.linalg.norm(p_earth_instr)
    d_didymos_instr = numpy.linalg.norm(p_instr)
    d_dimorphos_instr = numpy.linalg.norm(p_dimorphos_instr)
    m_didymos = spice.pxform("didymos_fixed", frame, et)
    m_dimorphos = spice.pxform("dimorphos_fixed", frame, et)
    m_instr = spice.pxform(instr, frame, et)

    app.simulation.sun.pos = p_sun
    app.simulation.camera.pos = p_instr
    app.simulation.camera.dir = m_instr @ instr_dir
    app.simulation.camera.up = m_instr @ instr_up
    app.simulation.bodies[0].mat[:3, :3] = m_didymos
    app.simulation.bodies[1].mat[:3, :3] = m_dimorphos
    app.simulation.bodies[1].mat[:3, 3] = p_dimorphos

    app.simulation.huds[
        0
    ].text = f"{date} Earth={d_earth_instr:.3e}km Didymos={d_didymos_instr:.3e}km Dimorphos={d_dimorphos_instr:.3e}km"

    app.step()

spice.kclear()
