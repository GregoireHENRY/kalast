#!/usr/bin/env python

import numpy
import pyrender
import trimesh

import kalast
from kalast.util import RPD

# /Users/gregoireh/data/mesh/plane_hemisphere_h=0.437.obj
# /Users/gregoireh/data/mesh/plane_hemisphere_h=0.313.obj
# /Users/gregoireh/data/mesh/plane_hemisphere_h=0.151.obj
# /Users/gregoireh/data/mesh/plane_hemisphere_h=0.0359.obj
# mesh = trimesh.load("/Users/gregoireh/data/mesh/plane_hemisphere_h=0.437.obj")
mesh = trimesh.load("/Users/gregoireh/data/mesh/sphere.obj")

# mesh.vertices = mesh.vertices * 1e-3
nface = mesh.faces.shape[0]
nvert = mesh.vertices.shape[0]
sph = numpy.array([kalast.util.cart2sph(v) for v in mesh.triangles_center])
print(f"nfaces={nface} nvert={nvert}")

mesh.unmerge_vertices()

bg_color = None
# bg_color = [0.0, 0.0, 0.0]
scene = pyrender.Scene(bg_color=bg_color)
# ren = pyrender.OffscreenRenderer(1024, 768)

rcam = pyrender.PerspectiveCamera(yfov=35.0 * RPD)
pose = numpy.eye(4)
pose[:3, 3] = [2, 2, 10]
nc = scene.add(rcam, pose=pose)

rmesh = pyrender.Mesh.from_trimesh(mesh, smooth=False)
pose = numpy.eye(4)
nb = scene.add(rmesh, pose=pose)

pyrender.Viewer(
    scene,
    viewport_size=[1024, 768],
    use_raymond_lighting=True,
    cull_faces=False,
    # flip_wireframe=True,
)
