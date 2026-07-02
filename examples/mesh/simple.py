#!/usr/bin/env python

import kalast


# create a vertex
v = kalast.mesh.Vertex(
    pos=[0.0, 0.0, 0.0],
    normal=[0.0, 0.0, 1.0],
)

# create a facet
# the plane of the facet is defined by a point (its center) and a normal
f = kalast.mesh.Facet(
    pos=[0.0, 0.0, 0.0],
    normal=[0.0, 0.0, 1.0],
    area=0.05,
)

# create a mesh from vertices
m = kalast.mesh.Mesh(
    vertices=[
        kalast.mesh.Vertex(
            pos=[0.0, 0.0, 0.0],
            normal=[0.0, 0.0, 1.0],
        ),
        kalast.mesh.Vertex(
            pos=[1.0, 0.0, 0.0],
            normal=[0.0, 0.0, 1.0],
        ),
        kalast.mesh.Vertex(
            pos=[0.0, 1.0, 0.0],
            normal=[0.0, 0.0, 1.0],
        ),
    ],
    indices=[0, 1, 2],
)

# can access all positions
m.positions

# all normals
m.normals

# all facets
m.facets

# all indices
m.indices

# can change position of 1st vertex
m.vertices[0].pos[:] = [1.0, 1.0, 0.0]

# can call this after update vertices to keep facets up to date.
m.recompute_facets()

# load a mesh wavefront file
cube = kalast.mesh.Mesh.load("res/cube.obj")