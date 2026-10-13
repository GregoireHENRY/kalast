#!/usr/bin/env python
"""One HRSC SRC image of Phobos, h8963, taken on 6 January 2011 at 09:21:16
UTC, drawn by kalast in its window: Phobos where SPICE puts it, seen through
the SRC as Mars Express pointed it, lit by the Sun and by Mars.

`main.py` does the same for the four images, with its options; `compare.py`
sets them beside the real ones.
"""

import os

import numpy
import spiceypy as spice

from kalast.app import App
from kalast.scattering import Hapke
from kalast.util import RPD

HERE = os.path.dirname(os.path.abspath(__file__))

spice.kclear()
spice.furnsh(os.path.join(HERE, "phobos_hrsc.tm"))
# Phobos turned as PCK00011 has it, the frame of Ernst et al.'s shape model.
spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/pck/pck00011.tpc")
et = spice.str2et("2011-01-06T09:21:16.092")  # h8963's IMAGE_TIME

app = App()
app.simulation.config.axes.style = "off"
app.simulation.config.wireframe.mode = 0
app.simulation.config.shading.srgb_mode = 1  # a pixel is exposure x I/F x 255
app.simulation.config.light.exposure = 12.0
app.simulation.config.light.sun_as_point = False
# Traced, progressively, to a still image: the Sun's disc, and its light
# bounced once off the surfaces -- Mars's on Phobos's night side.
app.simulation.config.shadows.method = "reference"
app.simulation.config.reference.max_samples = 1024
app.simulation.config.reference.bounces = 1

# The SRC: 1008 x 1018 pixels of its 1024 x 1024 CCD, each 9 microns behind
# a 984.76 mm focal length. Looking along its boresight, turned to the
# image's centre on the CCD (4 pixels along the samples, -1 along the lines)
# and by where h8963's craters put Phobos against SPICE's pointing (19.8 and
# -2.2 pixels, main.py's POINTING).
ifov = 9e-3 / 984.76
app.simulation.config.image.width = 1008
app.simulation.config.image.height = 1018
direction = numpy.array([(4.0 - 19.8) * ifov, (-1.0 + 2.2) * ifov, 1.0])
app.simulation.camera.pos = [0.0, 0.0, 0.0]
app.simulation.camera.dir = (direction / numpy.linalg.norm(direction)).tolist()
app.simulation.camera.up = [0.0, -1.0, 0.0]
app.simulation.camera.projection.fovy = 1018 * ifov

# Phobos: Ernst et al. (2023)'s shape, under Fornasier et al. (2024)'s Hapke
# fit of HRSC images. Mars: a sphere of its Bond albedo.
app.simulation.load_mesh(path="/Users/gregoireh/data/mesh/phobos/ernst2023/phobos_g_018m_spc_obj_0000n00000_v004.obj")
app.simulation.load_mesh(path=os.path.join(HERE, "..", "..", "res", "ico5.obj"))
phobos, mars = app.simulation.bodies
phobos.scattering = Hapke(w=0.0736, b=0.265, c=1.0, b0=2.283, h=0.0573, theta_bar=24.0 * RPD, k=1.19)
mars.mesh.colors[:] = 0.25
mars.mesh.update_gpu_colors()

# Everything in the SRC's frame, as seen from Mars Express: the light time
# taken off, each body turned as it was when its light left it.
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
