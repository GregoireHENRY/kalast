#!/usr/bin/env python

import numpy
import spiceypy as spice

import kalast  # noqa
from kalast.app import App
from kalast.util import AU_KM


app = App()
app.simulation.config.wireframe.mode = 2
app.simulation.config.wireframe.color = [0.05, 0.05, 0.05, 1.0]
app.simulation.config.axes.style = "blender"

app.simulation.sun.pos = [0.0, 50.0, 0.0]
app.simulation.camera.pos = [-1.1002305, -3.005702, 2.0494902]
app.simulation.camera.up = [0.18536071, 0.50638384, 0.8421501]
app.simulation.camera.dir = [0.2894826, 0.7908329, -0.53924316]

spice.kclear()
spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_plan_local.tm")

et0 = spice.str2et("2027-03-01 12:00:00 UTC")
et = et0

(p_sun, _lt) = spice.spkpos("SUN", et, "ECLIPJ2000", "none", "DIDYMOS")
(p_dimo, _lt) = spice.spkpos("DIMORPHOS", et, "ECLIPJ2000", "none", "DIDYMOS")
m_didy_ej2k = spice.pxform("DIDYMOS_FIXED", "ECLIPJ2000", et)
m_dimo_ej2k = spice.pxform("DIMORPHOS_FIXED", "ECLIPJ2000", et)

mat = numpy.eye(4)
mat[:3, :3] = m_didy_ej2k
app.simulation.load_mesh(
    # path="/Users/gregoireh/data/mesh/didymos/didymos_g_9309mm_spc_obj_0000n00000_v003_decimated_3072.obj",
    # path="/Users/gregoireh/data/mesh/didymos/didymos_g_9309mm_spc_obj_0000n00000_v003_decimated_1k.obj",
    path="/Users/gregoireh/data/mesh/didymos/didymos_g_9309mm_spc_obj_0000n00000_v003.obj",
    # path="/Users/gregoireh/data/mesh/didymos/didymos_g_1165mm_spc_obj_0000n00000_v003.obj",
    mat=mat,
    flatten=True,
)

mat = numpy.eye(4)
mat[:3, 3] = p_dimo
mat[:3, :3] = m_dimo_ej2k
app.simulation.load_mesh(
    # path="/Users/gregoireh/data/mesh/dimorphos/dimorphos_g_1940mm_spc_obj_0000n00000_v004_decimated_3072.obj",
    # path="/Users/gregoireh/data/mesh/dimorphos/dimorphos_g_1940mm_spc_obj_0000n00000_v004_decimated_1k.obj",
    path="/Users/gregoireh/data/mesh/dimorphos/dimorphos_g_1940mm_spc_obj_0000n00000_v004.obj",
    # path="/Users/gregoireh/data/mesh/dimorphos/dimorphos_g_0243mm_spc_obj_0000n00000_v004.obj",
    mat=mat,
    flatten=True,
)

while app.running:
    et = et0 + app.simulation.state.iteration * 60.0
    (p_sun, _lt) = spice.spkpos("SUN", et, "ECLIPJ2000", "none", "DIDYMOS")
    (p_dimo, _lt) = spice.spkpos("DIMORPHOS", et, "ECLIPJ2000", "none", "DIDYMOS")
    m_didy_ej2k = spice.pxform("DIDYMOS_FIXED", "ECLIPJ2000", et)
    m_dimo_ej2k = spice.pxform("DIMORPHOS_FIXED", "ECLIPJ2000", et)

    app.simulation.sun.pos = p_sun / AU_KM * 10.0
    app.simulation.bodies[0].mat[:3, :3] = m_didy_ej2k
    app.simulation.bodies[1].mat[:3, 3] = p_dimo
    app.simulation.bodies[1].mat[:3, :3] = m_dimo_ej2k

    app.step()

spice.kclear()