#!/usr/bin/env python

import numpy  # noqa
import spiceypy as spice

import kalast  # noqa
from kalast.app import App
from kalast.scattering import Hapke
from kalast.util import RPD


app = App()
app.simulation.config.wireframe.mode = 0
app.simulation.config.shading.color_mode = "shaded"
app.simulation.config.shading.srgb_mode = "linear"
app.simulation.config.light.exposure = 5.0

app.simulation.camera.pos = [-1.0, -3.0, 1.0]
app.simulation.camera.look_anchor()

app.simulation.load_mesh(
    path="/Users/gregoireh/data/mesh/didymos/g_01165mm_spc_didy_v003.obj"
)
app.simulation.load_mesh(
    path="/Users/gregoireh/data/mesh/dimorphos/g_00243mm_spc_dimo_v004.obj",
)

# Buratti et al. (2024), DART's DRACO images and the system's phase curve.
law = Hapke(w=0.126, b=0.36, c=1.0, b0=0.97, h=0.020, theta_bar=18.0 * RPD, k=1.0)
app.simulation.bodies[0].scattering = law
app.simulation.bodies[1].scattering = law

spice.kclear()
spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_plan.tm")
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