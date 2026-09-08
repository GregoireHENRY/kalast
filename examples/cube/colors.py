#!/usr/bin/env python

import numpy

import kalast


app = kalast.app.App()
app.simulation.config.color_mode = 1
app.simulation.config.wireframe_mode = 2
app.simulation.camera.pos = [10.0, 0.0, 0.0]
app.simulation.camera.dir = [-1.0, 0.0, 0.0]

app.simulation.load_mesh(path="res/cube.obj", mat=numpy.eye(4), flatten=True)
mesh = app.simulation.bodies[0].mesh
nface = len(mesh.facets)

# color facets from index 0 to 12 from black to white
white = numpy.array([1.0, 1.0, 1.0])
for iif in range(nface):
    color = iif / nface * white
    mesh.colors[iif * 3 + 0, :] = color
    mesh.colors[iif * 3 + 1, :] = color
    mesh.colors[iif * 3 + 2, :] = color

while app.running:
    app.step()
