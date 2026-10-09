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
app.simulation.config.image.width = 1020
app.simulation.config.image.height = 1020
app.simulation.config.axes.style = "off"
app.simulation.config.wireframe.mode = 0
app.simulation.config.shading.srgb_mode = 1
app.simulation.config.light.exposure = 4.5

app.simulation.huds = [Hud("", size=16)]

# Like IK and ShapeViewer
# app.simulation.camera.pos = [0.0, 0.0, 0.0]
# app.simulation.camera.dir = [0.0, 0.0, 1.0]
# app.simulation.camera.up = [0.0, 1.0, 0.0]

# Like real AFC image
app.simulation.camera.pos = [0.0, 0.0, 0.0]
app.simulation.camera.dir = [0.0, 0.0, 1.0]
app.simulation.camera.up = [1.0, 0.0, 0.0]

app.simulation.camera.projection.fovy = 5.5 * RPD

app.simulation.load_mesh(path="/Users/gregoireh/data/mesh/mars/mars_mola_afc_20250312.obj")
app.simulation.load_mesh(path="/Users/gregoireh/data/mesh/deimos/deimos_g_083m_spc_obj_0000n00000_v002.obj")
app.simulation.load_mesh(path="/Users/gregoireh/data/mesh/phobos/phobos_m003_gas_v01_10k.obj")

tes = numpy.asarray(Image.open("/Users/gregoireh/data/mars/Mars_MGS_TES_Albedo_mosaic_global_7410m.tif"))
mars = app.simulation.bodies[0]
law = Hapke(w=0.70, b=0.141, c=1.0, b0=1.0, h=0.052, theta_bar=21.5 * RPD, k=1.0)
mars.scattering = law
mars.atmosphere = Atmosphere(tau=0.36, scale_height=8.7, g2=-0.248, q=0.929)
ref = numpy.pi * law.reflectance(numpy.cos(45 * RPD), 1.0, 45 * RPD)
mars.mesh.colors_from_map(0.799 * (0.19 + 0.754 * (tes - 0.19)) / ref)
mars.horizon_map = True

# Hapke 2012 fits, colours left at 1, the laws' own albedo: Deimos from
# Wargnier et al. (2025), Phobos from Fornasier et al. (2024) at 655 nm.

deimos = app.simulation.bodies[1]
deimos.scattering = Hapke(w=0.0929, b=0.275, c=1.0, b0=1.154, h=0.0500, theta_bar=19.4 * RPD, k=1.21)
albedo = numpy.load("notes/2026-10-08_deimos_afc_photometry/deimos_afc_albedo.npy")  # path from the repo root
deimos.mesh.colors[:] = numpy.nan_to_num(albedo, nan=1.0)[:, None]
deimos.mesh.update_gpu_colors()

app.simulation.bodies[2].scattering = Hapke(w=0.0743, b=0.252, c=1.0, b0=2.283, h=0.0573, theta_bar=22.9 * RPD, k=1.19)

spice.kclear()
spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops.tm")
et0 = spice.str2et("2025-03-12 05:52:00 UTC")
# et0 = spice.str2et("2025-03-12 09:10:25")
# et0 = spice.str2et("2025-03-12 12:08:31.777")
# et0 = spice.str2et("2025-03-12 12:08:58")
# et0 = spice.str2et("2025-03-12 12:09:18.7503")
et1 = spice.str2et("2025-03-12 12:07:00 UTC")
et2 = spice.str2et("2025-03-12 12:10:00 UTC")
etf = spice.str2et("2025-03-12 16:00:00 UTC")
dur = etf - et0
dt0 = 60.0
dt1 = 5.0
dt = dt0
et = et0
instr = "hera_afc-1"

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
    # app.simulation.bodies[2].mat[:3, :3] = LARGER[:3, :3] @ m_phobos_instr
    app.simulation.bodies[2].mat[:3, :3] = m_phobos_instr
    app.simulation.bodies[2].mat[:3, 3] = p_phobos

    app.simulation.huds[
        0
    ].text = f"{date} Earth={d_earth:.3e}km Mars={d_mars:.3e}km Deimos={d_deimos:.3e}km Phobos={d_phobos:.3e}km"

    app.step()

spice.kclear()