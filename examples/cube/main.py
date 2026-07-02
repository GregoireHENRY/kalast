#!/usr/bin/env python

import numpy

import kalast


def tick(sim: kalast.app.simulation.Simulation, dt: float):
    pass


app = kalast.app.App()
app.config.debug_app = True
app.config.debug_window = True
app.config.width = 1024
app.config.height = 768
app.config.color_mode = 1

app.simulation.camera.pos = [10.0, 0.0, 0.0]
app.simulation.camera.dir = [-1.0, 0.0, 0.0]

mat = numpy.eye(4)
app.simulation.load_mesh(path="res/cube.obj", mat=mat, flatten=True)
mesh = app.simulation.bodies[0].mesh
nface = len(mesh.facets)

white = numpy.array([1.0, 1.0, 1.0])

# color facets from index 0 to 12 from black to white

for iif in range(nface):
    color = iif / nface * white
    mesh.colors[iif * 3 + 0, :] = color
    mesh.colors[iif * 3 + 1, :] = color
    mesh.colors[iif * 3 + 2, :] = color

app.tick = tick
app.start()
