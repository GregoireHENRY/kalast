#!/usr/bin/env python

import numpy
import spiceypy as spice

import kalast  # noqa
from kalast.app import App, Hud
from kalast.util import DPR, RPD, AU, SOLAR_CONSTANT  # noqa


LARGER = numpy.diag((10.0, 10.0, 10.0, 1.0))

app = App()
app.simulation.config.image.width = 1018
app.simulation.config.image.height = 768
app.simulation.config.image.flip_y = True
app.simulation.config.axes.style = "off"
app.simulation.config.wireframe.mode = 0

app.simulation.huds = [Hud("", size=16, color=[0.0, 0.0, 0.0, 1.0])]
app.simulation.camera.pos = [0.0, 0.0, 0.0]
app.simulation.camera.dir = [0.0, 0.0, 1.0]
app.simulation.camera.up = [0.0, 1.0, 0.0]
app.simulation.camera.projection.fovy = 10.0 * RPD

app.simulation.load_mesh(path="/Users/gregoireh/data/mesh/mars/mars_dtm_10x.obj")
app.simulation.load_mesh(
    path="/Users/gregoireh/data/mesh/deimos/deimos_10k.obj",
    # mat=LARGER,
)
app.simulation.load_mesh(
    path="/Users/gregoireh/data/mesh/phobos/phobos_m003_gas_v01_10k.obj",
    # mat=LARGER,
)

spice.kclear()
spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops.tm")
et0 = spice.str2et("2025-03-12 05:52:00 UTC")
# et0 = spice.str2et("2025-03-12 12:09:18.7503")
# et0 = spice.str2et("2025-03-12 12:08:58")
et1 = spice.str2et("2025-03-12 12:07:00 UTC")
et2 = spice.str2et("2025-03-12 12:10:00 UTC")
etf = spice.str2et("2025-03-12 16:00:00 UTC")
dur = etf - et0
dt0 = 60.0
dt1 = 5.0
dt = dt0
et = et0
instr = "hera_tiri"

while app.running:
    if app.simulation.state.is_paused:
        app.step()
        continue

    if app.simulation.state.iteration > 0:
        et += dt

    date = spice.timout(et, kalast.util.SPICE_PICTUR_3)
    print(date)

    if etf - et < 1.0:
        app.simulation.state.toggle_pause()
    elif et2 - et < 1.0:
         dt = dt0
    elif et1 - et < 1.0:
        dt = dt1

    app.simulation.export_once()

    (p_sun, _lt) = spice.spkpos("sun", et, instr, "none", instr)
    (p_earth, _lt) = spice.spkpos("earth", et, instr, "none", instr)
    (p_mars, _lt) = spice.spkpos("mars", et, instr, "none", instr)
    (p_deimos, _lt) = spice.spkpos("deimos", et, instr, "none", instr)
    (p_phobos, _lt) = spice.spkpos("phobos", et, instr, "none", instr)
    d_earth = numpy.linalg.norm(p_earth)
    d_mars = numpy.linalg.norm(p_mars)
    d_deimos = numpy.linalg.norm(p_deimos)
    d_phobos = numpy.linalg.norm(p_phobos)
    m_mars_instr = spice.pxform("iau_mars", instr, et)
    m_deimos_instr = spice.pxform("iau_deimos", instr, et)
    m_phobos_instr = spice.pxform("iau_phobos", instr, et)

    app.simulation.sun.pos = p_sun
    app.simulation.bodies[0].mat[:3, :3] = m_mars_instr
    app.simulation.bodies[0].mat[:3, 3] = p_mars
    app.simulation.bodies[1].mat[:3, :3] = m_deimos_instr
    app.simulation.bodies[1].mat[:3, 3] = p_deimos
    app.simulation.bodies[2].mat[:3, :3] = m_phobos_instr
    app.simulation.bodies[2].mat[:3, 3] = p_phobos

    app.simulation.huds[
        0
    ].text = f"{date} Earth={d_earth:.3e}km Mars={d_mars:.3e}km Deimos={d_deimos:.3e}km Phobos={d_phobos:.3e}km"

    app.step()

spice.kclear()
