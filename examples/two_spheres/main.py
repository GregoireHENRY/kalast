#!/usr/bin/env python

import numpy

import kalast
from kalast.app import App, Hud


app = App()
app.simulation.config.access_shadow_map = True
app.simulation.config.wireframe_mode = 2
app.simulation.config.wireframe_color = [0.05, 0.05, 0.05, 1.0]
app.simulation.huds = [Hud("", size=16)]
app.simulation.config.axes = "blender"

app.simulation.sun.pos = [0.0, 20.0, 0.0]
app.simulation.camera.pos = [-0.9687278, 13.656183, 7.445293]
app.simulation.camera.up = [0.03380529, -0.47655377, 0.8784952]
app.simulation.camera.dir = [0.06216155, -0.8762931, -0.47775126]

app.simulation.load_mesh(path="res/ico3.obj", mat=numpy.eye(4), flatten=True)

mat = numpy.eye(4)
mat[:3, :3] *= numpy.eye(3) * 0.2
mat[0:3, 3] = [0.0, 5.0, 0.0]
app.simulation.load_mesh(path="res/ico3.obj", mat=mat, flatten=True)

mat = numpy.eye(4)
mat[:3, :3] = kalast.util.mat_axis_angle(numpy.array([0.0, 0.0, 1.0]), 0.01)

while app.running:
    if not app.simulation.state.is_paused:
        bod = app.simulation.bodies[1]
        bod.mat = mat @ bod.mat
        app.simulation.huds[0].text = f"it {app.simulation.state.iteration} %"
    app.step()
