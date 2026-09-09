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
mesh.values = numpy.arange(nface, dtype=float)

while app.running:
    app.step()
