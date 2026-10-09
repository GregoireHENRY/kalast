#!/usr/bin/env python

import numpy
import spiceypy as spice
from PIL import Image

import kalast  # noqa
from kalast.app import App, Hud
from kalast.scattering import Hapke, Atmosphere
from kalast.util import DPR, RPD, AU, SOLAR_CONSTANT  # noqa


LARGER = numpy.diag((10.0, 10.0, 10.0, 1.0))

app = App()
app.simulation.config.image.width = 1018
app.simulation.config.image.height = 768
app.simulation.config.image.flip_y = True
app.simulation.config.axes.style = "off"
app.simulation.config.wireframe.mode = 0
app.simulation.config.shading.srgb_mode = 1
app.simulation.config.light.exposure = 4.5
app.simulation.config.light.sun_radius = 695700.0

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

# app.simulation.bodies[0].atmosphere = Atmosphere(tau=0.45)
mars = app.simulation.bodies[0].mesh
tes = numpy.asarray(Image.open("/Users/gregoireh/data/mars/Mars_MGS_TES_Albedo_mosaic_global_7410m.tif"))
mars.colors_from_map(0.9 * tes)

# Hapke 2012 fits, colours left at 1, the laws' own albedo: Deimos from
# Wargnier et al. (2025), Phobos from Fornasier et al. (2024) at 655 nm.
# app.simulation.bodies[1].scattering = Hapke(w=0.068, b=0.275, c=1.0, b0=2.14, h=0.065, theta_bar=19.4 * RPD, k=1.21)
# app.simulation.bodies[2].scattering = Hapke(w=0.0743, b=0.252, c=1.0, b0=2.283, h=0.0573, theta_bar=22.9 * RPD, k=1.19)

spice.kclear()
spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops.tm")
et0 = spice.str2et("2025-03-12 05:52:00 UTC")
# et0 = spice.str2et("2025-03-12 09:10:25 UTC")
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

    # app.simulation.export_once()

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
