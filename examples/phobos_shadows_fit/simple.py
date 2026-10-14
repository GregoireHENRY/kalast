#!/usr/bin/env python

import os

import numpy
import spiceypy as spice

from kalast.app import App
from kalast.scattering import Hapke
from kalast.util import RPD

HERE = os.path.dirname(os.path.abspath(__file__))

# The four images' time with boresight correction and smear. Pick one.
IMAGES = {
    "h8870": ("2010-12-10T06:44:31.077", 20.7, 14.1, 1.86),
    "h8951": ("2011-01-02T21:16:29.361", 15.4, -4.8, 4.90),
    "h8963": ("2011-01-06T09:21:16.092", 19.8, -2.2, 2.34),
    "hc069": ("2013-06-29T06:02:43.085", -44.1, 44.5, 6.98),
}
time, dx, dy, smear = IMAGES["h8870"]

spice.kclear()
spice.furnsh(os.path.join(HERE, "phobos_hrsc.tm"))
spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/pck/pck00011.tpc")
et = spice.str2et(time)

app = App()
app.simulation.config.axes.style = "off"
app.simulation.config.wireframe.mode = 0
app.simulation.config.shading.srgb_mode = "linear"
app.simulation.config.light.exposure = 40.0
app.simulation.config.light.sun_as_point = False
app.simulation.config.shadows.method = "reference"
app.simulation.config.reference.max_samples = 1024
app.simulation.config.reference.bounces = 1

# Fitted.
app.simulation.config.response.enabled = True
app.simulation.config.response.core = 2.19
app.simulation.config.response.halo_share = 0.012
app.simulation.config.response.halo_scale = 53.0
app.simulation.config.response.halo2_share = 0.282
app.simulation.config.response.halo2_scale = 434.0
app.simulation.config.response.smear = smear

# SRC is 1008 x 1018 pixels of a 1024 x 1024 CCD
# Each pixel is 9 microns with 984.76 mm focal length. 
ifov = 9e-3 / 984.76
app.simulation.config.image.width = 1008
app.simulation.config.image.height = 1018
direction = numpy.array([(4.0 - dx) * ifov, (-1.0 - dy) * ifov, 1.0])
app.simulation.camera.pos = [0.0, 0.0, 0.0]
app.simulation.camera.dir = (direction / numpy.linalg.norm(direction)).tolist()
app.simulation.camera.up = [0.0, -1.0, 0.0]
app.simulation.camera.projection.fovy = 1018 * ifov

# Phobos: Ernst et al. (2023)'s shape, under Fornasier et al. (2024)'s Hapke
# fit of HRSC images.
app.simulation.load_mesh(path="/Users/gregoireh/data/mesh/phobos/ernst2023/phobos_g_018m_spc_obj_0000n00000_v004.obj")
app.simulation.load_mesh(path=os.path.join(HERE, "..", "..", "res", "ico5.obj"))
phobos, mars = app.simulation.bodies
phobos.scattering = Hapke(w=0.0736, b=0.265, c=1.0, b0=2.283, h=0.0573, theta_bar=24.0 * RPD, k=1.19)
mars.mesh.colors[:] = 0.25
mars.mesh.update_gpu_colors()

to_src = numpy.array(spice.pxform("J2000", "MEX_HRSC_SRC", et))
p_phobos, lt_phobos = spice.spkpos("PHOBOS", et, "J2000", "LT+S", "MEX")
p_mars, lt_mars = spice.spkpos("MARS", et, "J2000", "LT+S", "MEX")
p_sun, _ = spice.spkpos("SUN", et, "J2000", "LT+S", "MEX")
mars_radii = spice.bodvrd("MARS", "RADII", 3)[1]

phobos.mat[:3, :3] = to_src @ numpy.array(spice.pxform("IAU_PHOBOS", "J2000", et - lt_phobos))
phobos.mat[:3, 3] = to_src @ p_phobos
mars.mat[:3, :3] = to_src @ numpy.array(spice.pxform("IAU_MARS", "J2000", et - lt_mars)) @ numpy.diag(mars_radii)
mars.mat[:3, 3] = to_src @ p_mars
app.simulation.sun.pos = (to_src @ p_sun).tolist()
spice.kclear()

app.start()
