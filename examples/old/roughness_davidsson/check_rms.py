#!/usr/bin/env python

import numpy

import kalast

# work/mesh.obj
# /Users/gregoireh/data/mesh/plane_crater_1024-512_h=0.0359.obj
# /Users/gregoireh/data/mesh/plane_crater_1024-512_h=0.151.obj
# /Users/gregoireh/data/mesh/plane_crater_1024-512_h=0.313.obj
# /Users/gregoireh/data/mesh/plane_crater_1024-512_h=0.437.obj
# /Users/gregoireh/data/mesh/plane_crater_1024-5000_h=0.437.obj
# /Users/gregoireh/data/mesh/plane_crater_4096-5000_h=0.437.obj
# /Users/gregoireh/data/mesh/plane_crater_40000-160000_h=0.437.obj

mesh = kalast.mesh.Mesh("res/plane_crater_1024-5000_h=0.437.obj", lambda x: x)

normal_terrain = numpy.array([0, 0, 1.0])

thetas = numpy.array(
    [
        numpy.acos(kalast.astro.cosine_angle_vectors(facet.n, normal_terrain))
        for facet in mesh.facets
    ]
)


rms = kalast.mesh.rms_slope_terrain(
    thetas, numpy.array([facet.a for facet in mesh.facets])
)
assert rms * kalast.util.DPR - 40.025 <= 1e-2
