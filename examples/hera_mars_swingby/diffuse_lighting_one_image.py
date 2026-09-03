#!/usr/bin/env python

# This line before simulation starts:
# `app.simulation.sun.look_anchor()`
# makes the Sun look origin cus anchor of the Sun is not set.
#
# This is "correct" for quick result but will be incorrect
# for deeper and further analysis.

from pathlib import Path  # noqa

import matplotlib  # noqa
import matplotlib.pyplot as plt  # noqa
import numpy
import pandas  # noqa
import spiceypy as spice
from astropy.io import fits  # noqa

import kalast
from kalast.util import DPR, RPD, AU, SOLAR_CONSTANT  # noqa

# Phobos is drawn 10x oversized, deliberately. All three meshes are already in
# km at true scale -- the mesh here has a mean radius of 11.4 km, which is
# Phobos -- so this is a visibility hack, not a unit conversion. **Nothing in
# the Phobos body is to scale**: its size, its limb and its terminator position
# on the sky are all wrong by construction. Mars and Deimos are true scale.
M4_RESCALE = numpy.eye(4)
M4_RESCALE[:3, :3] = numpy.eye(3) * 10.0


def pos_mat(target, frame, et):
    p, _ = spice.spkpos(target, et, "HERA_TIRI", "none", "HERA")
    m = spice.pxform(frame, "HERA_TIRI", et)
    m4 = numpy.eye(4)
    m4[:3, :3] = m
    m4[:3, 3] = p
    return m4


def before_render(sim, _dt):
    pass


def after_render(sim, _dt):
    pass


spice.kclear()
spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm")
et0 = spice.str2et("2025-03-12 08:10:50 UTC")

app = kalast.app.App()
app.config.width = 1024
app.config.height = 768
app.config.vsync = False
app.simulation.camera.pos = [0.0, 0.0, 0.0]
app.simulation.camera.dir = [0.0, 0.0, 1.0]
app.simulation.camera.up = [0.0, 1.0, 0.0]
app.simulation.camera.projection.fovy = 10.0 * RPD
app.simulation.load_mesh(
    path="/Users/gregoireh/data/mesh/mars/mars_dtm_10x.obj",
    mat=numpy.eye(4),
    flatten=True,
)
app.simulation.load_mesh(
    path="/Users/gregoireh/data/mesh/phobos/phobos_m003_gas_v01_10k.obj",
    mat=numpy.eye(4),
    flatten=True,
)
app.simulation.load_mesh(
    path="/Users/gregoireh/data/mesh/deimos/deimos_10k.obj",
    mat=numpy.eye(4),
    flatten=True,
)
app.simulation.bodies[0].mat = pos_mat("MARS", "IAU_MARS", et0)
app.simulation.bodies[1].mat = pos_mat("PHOBOS", "IAU_PHOBOS", et0) @ M4_RESCALE
app.simulation.bodies[2].mat = pos_mat("DEIMOS", "IAU_DEIMOS", et0)
app.simulation.sun.pos = spice.spkpos("SUN", et0, "HERA_TIRI", "none", "HERA")[0]
app.simulation.sun.look_anchor()
app.simulation.camera.anchor = app.simulation.bodies[0].mat[:3, 3].copy()

app.simulation.export_once()
app.before_render = before_render
app.after_render = after_render
app.start()
spice.kclear()
