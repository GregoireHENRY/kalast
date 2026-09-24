#!/usr/bin/env python

from pathlib import Path

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
    path="/Users/gregoireh/data/mesh/didymos/g_01165mm_spc_didy_v003_100k.obj"
)
app.simulation.load_mesh(
    path="/Users/gregoireh/data/mesh/dimorphos/g_00243mm_spc_dimo_v004_100k.obj",
)

spice.kclear()
spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_plan_local.tm")
et0 = spice.str2et("2026-11-05 00:00:00 UTC")
etf = spice.str2et("2027-04-30 00:00:00 UTC")
dur = etf - et0
dt = 15.0 * 60.0
instr = "hera_afc-1"

# Where each body's centre and each selected facet's centre land in the image:
# pixels from its top-left corner, x right and y down, one row per point per
# frame. Select facets by clicking them, or app.simulation.toggle_facet(b, f).
out = Path("out/hera_didymos/afc")
out.mkdir(parents=True, exist_ok=True)
screen = open(out / "screen.csv", "w")
screen.write("iteration,utc,body,facet,x,y\n")

while app.running:
    it = app.simulation.state.iteration
    et = et0 + (it * dt) % dur
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

    if not app.step():
        break
    if app.simulation.state.iteration == it:
        continue  # paused: the same epoch again

    # After the step, so these are the positions in the frame just drawn.
    sim = app.simulation
    rows = [(b, "", sim.project_body(b)) for b in range(len(sim.bodies))]
    rows += [(b, f, sim.project_facet(b, f)) for b, f in sim.selected_facets]
    for b, f, xy in rows:
        if xy is not None:  # None: behind the camera
            screen.write(f"{it},{date},{b},{f},{xy[0]:.3f},{xy[1]:.3f}\n")

screen.close()
spice.kclear()