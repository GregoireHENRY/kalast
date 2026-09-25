#!/usr/bin/env python

import numpy  # noqa
import spiceypy as spice

import kalast  # noqa
from kalast.app import App


app = App()
# app.simulation.config.wireframe.mode = 2
app.simulation.config.wireframe.color = [0.05, 0.05, 0.05, 1.0]
app.simulation.config.axes.style = "gizmo"

app.simulation.camera.pos = [-1.0, -3.0, 1.0]
app.simulation.camera.look_anchor()

app.simulation.load_mesh(
    path="/Users/gregoireh/data/mesh/didymos/g_01165mm_spc_didy_v003.obj"
)
app.simulation.load_mesh(
    path="/Users/gregoireh/data/mesh/dimorphos/g_00243mm_spc_dimo_v004.obj",
)

spice.kclear()
spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_plan_local.tm")
et0 = spice.str2et("2027-03-01 12:00:00 UTC")

while app.running:
    et = et0 + app.simulation.state.iteration * 60.0
    (p_sun, _lt) = spice.spkpos("SUN", et, "ECLIPJ2000", "none", "DIDYMOS")
    (p_dimo, _lt) = spice.spkpos("DIMORPHOS", et, "ECLIPJ2000", "none", "DIDYMOS")
    m_didy_ej2k = spice.pxform("DIDYMOS_FIXED", "ECLIPJ2000", et)
    m_dimo_ej2k = spice.pxform("DIMORPHOS_FIXED", "ECLIPJ2000", et)

    app.simulation.sun.pos = p_sun
    app.simulation.bodies[0].mat[:3, :3] = m_didy_ej2k
    app.simulation.bodies[1].mat[:3, 3] = p_dimo
    app.simulation.bodies[1].mat[:3, :3] = m_dimo_ej2k

    app.step()

spice.kclear()