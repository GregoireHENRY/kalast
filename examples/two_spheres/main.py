#!/usr/bin/env python

import numpy

import kalast
from kalast.app import App


app = App()
app.simulation.config.wireframe.mode = 2
app.simulation.config.wireframe.color = [0.05, 0.05, 0.05, 1.0]
app.simulation.config.axes.style = "gizmo"

app.simulation.sun.pos = [10.0, 0.0, 0.0]
app.simulation.camera.pos = [13.5, -6.0, 5.0]
app.simulation.camera.look_anchor()

mat = numpy.eye(4)
mat[:3, 3] = [2.5, 0.0, 0.0]
mat[:3, :3] = numpy.eye(3) * 0.3
app.simulation.load_mesh(path="res/ico3.obj")
app.simulation.load_mesh(path="res/ico3.obj", mat=mat)

mat = numpy.eye(4)
mat[:3, :3] = kalast.util.mat_axis_angle(numpy.array([0.0, 0.0, 1.0]), 0.001)

while app.running:
    if app.simulation.state.is_paused:
        app.step()
        continue

    bod = app.simulation.bodies[1]
    bod.mat = mat @ bod.mat

    app.step()
