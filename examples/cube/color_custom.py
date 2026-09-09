#!/usr/bin/env python

import numpy

import kalast


app = kalast.app.App()
app.simulation.config.color_mode = 1
app.simulation.config.wireframe_mode = 2
app.simulation.config.wireframe_color = [1.0, 0.0, 1.0, 1.0]
app.simulation.camera.pos = [10.0, 0.0, 0.0]
app.simulation.camera.dir = [-1.0, 0.0, 0.0]
app.simulation.load_mesh(path="res/cube.obj", mat=numpy.eye(4), flatten=True)

mesh = app.simulation.bodies[0].mesh
nface = len(mesh.facets)
for iface in range(0, nface):
    for k in range(3):
        mesh.colors[iface * 3 + k, :] = numpy.array([1.0, 1.0, 1.0]) * iface / (nface - 1)

while app.running:
    app.step()
