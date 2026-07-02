#!/usr/bin/env python

import numpy
import trimesh
import pyrender

import kalast
from kalast.util import RPD


mesh = trimesh.load("work/didymos/mesh.obj")
# mesh = trimesh.load("work/dimorphos/mesh.obj")

# mesh.vertices = mesh.vertices * 1e-3
nface = mesh.faces.shape[0]
nvert = mesh.vertices.shape[0]
sph = numpy.array([kalast.util.cart2sph(v) for v in mesh.triangles_center])
print(f"nfaces={nface} nvert={nvert}")

equator = numpy.where(numpy.abs(sph[:, 1]) < 5 * RPD)[0]
meridian0 = numpy.where(numpy.abs(sph[:, 0]) < 5 * RPD)[0]
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

bg_color = None
# bg_color = [0.0, 0.0, 0.0]
scene = pyrender.Scene(bg_color=bg_color)
# ren = pyrender.OffscreenRenderer(1024, 768)

rcam = pyrender.PerspectiveCamera(yfov=5.0 * RPD)
pose = numpy.eye(4)
pose[:3, 3] = [0, 0, 3]
nc = scene.add(rcam, pose=pose)

rmesh = pyrender.Mesh.from_trimesh(mesh, smooth=False)
pose = numpy.eye(4)
nb = scene.add(rmesh, pose=pose)

pyrender.Viewer(scene, viewport_size=[1024, 768], use_raymond_lighting=True)
