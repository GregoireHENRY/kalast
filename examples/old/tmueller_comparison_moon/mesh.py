#!/usr/bin/env python

import numpy
import trimesh

import kalast
from kalast.util import RPD


mesh = trimesh.load("moon.obj")
mesh.vertices = mesh.vertices * 1e-3
nface = mesh.faces.shape[0]
nvert = mesh.vertices.shape[0]
sph = numpy.array([kalast.util.cart2sph(v) for v in mesh.triangles_center])
print(f"nfaces={nface} nvert={nvert}")

equator = numpy.where(numpy.abs(sph[:, 1]) < 6 * RPD)[0]
meridian0 = numpy.where(numpy.abs(sph[:, 0]) < 6 * RPD)[0]
mix = numpy.intersect1d(equator, meridian0)

mesh.unmerge_vertices()

for iif in equator:
    print(iif, mesh.faces[iif])
    mesh.visual.face_colors[iif] = [255, 0, 0, 255]

for iif in meridian0:
    print(iif, mesh.faces[iif])
    mesh.visual.face_colors[iif] = [0, 255, 0, 255]

for iif in mix:
    print(iif, mesh.faces[iif])
    mesh.visual.face_colors[iif] = [0, 0, 255, 255]

numpy.save("equator.npy", equator)
numpy.save("meridian0.npy", meridian0)
numpy.save("equator_meridian0.npy", mix)

mesh.show()
