#!/usr/bin/env python

import numpy  # noqa
import trimesh

import kalast
from kalast.util import RPD  # noqa

mesh = trimesh.creation.icosphere(subdivisions=4)

mesh.vertices *= kalast.spice_entities.mars.radii * 1e-3
mesh.export("mars.obj")

# sph = numpy.array([kalast.util.cart2sph(v) for v in mesh.triangles_center])

# mesh.show()
