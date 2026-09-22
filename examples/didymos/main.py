#!/usr/bin/env python

import numpy  # noqa
import spiceypy as spice

import kalast  # noqa
from kalast.app import App

import os
from pathlib import Path

# Where the data is. Set the variables to your own locations, or keep the
# defaults; res/README.md says how to get each set.
HERA = Path(os.environ.get("KALAST_HERA", "~/data/spice/hera")).expanduser()  # HERA.zip, unpacked
MESH_ROOT = Path(os.environ.get("KALAST_MESH", "~/data/mesh")).expanduser()  # meshes not in HERA.zip


app = App()
app.simulation.config.wireframe.mode = 0
app.simulation.config.wireframe.color = [0.05, 0.05, 0.05, 1.0]
app.simulation.config.axes.style = "blender"

app.simulation.camera.pos = [-1.1002305, -3.005702, 2.0494902]
app.simulation.camera.up = [0.18536071, 0.50638384, 0.8421501]
app.simulation.camera.dir = [0.2894826, 0.7908329, -0.53924316]

app.simulation.load_mesh(
    path=f"{MESH_ROOT}/didymos/g_01165mm_spc_didy_v003_100k.obj"
)
app.simulation.load_mesh(
    path=f"{MESH_ROOT}/dimorphos/g_00243mm_spc_dimo_v004_100k.obj",
)

spice.kclear()
spice.furnsh(f"{HERA}/kernels/mk/hera_plan_local.tm")
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