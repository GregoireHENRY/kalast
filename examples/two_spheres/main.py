#!/usr/bin/env python

import numpy

import kalast
from kalast.app import App, Hud


app = App()
app.simulation.config.wireframe.mode = 2
app.simulation.config.wireframe.color = [0.05, 0.05, 0.05, 1.0]
app.simulation.huds = [Hud("", size=16)]
app.simulation.config.axes.style = "blender"

app.simulation.sun.pos = [10.0, 0.0, 0.0]
app.simulation.camera.pos = [13.567162, -5.9675856, 4.814754]
app.simulation.camera.up = [-0.2828058, 0.124393575, 0.9510768]
app.simulation.camera.dir = [-0.87058145, 0.3829297, -0.3089545]

mat = numpy.eye(4)
mat[:3, 3] = [2.5, 0.0, 0.0]
mat[:3, :3] = numpy.eye(3) * 0.3
app.simulation.load_mesh(path="res/ico3.obj")
app.simulation.load_mesh(path="res/ico3.obj", mat=mat)

mat = numpy.eye(4)
mat[:3, :3] = kalast.util.mat_axis_angle(numpy.array([0.0, 0.0, 1.0]), 0.01)

while app.running:
    if not app.simulation.state.is_paused:
        bod = app.simulation.bodies[1]
        bod.mat = mat @ bod.mat
        app.simulation.huds[0].text = f"it {app.simulation.state.iteration} %"
    app.step()
