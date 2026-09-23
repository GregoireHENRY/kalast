#!/usr/bin/env python

import numpy  # noqa

import kalast


app = kalast.app.App()
app.simulation.config.shading.color_mode = 1
app.simulation.config.wireframe.mode = 2
app.simulation.config.wireframe.width = 2.0
app.simulation.config.wireframe.color = [1.0, 0.0, 1.0, 1.0]
app.simulation.camera.pos = [10.0, 0.0, 0.0]
app.simulation.camera.dir = [-1.0, 0.0, 0.0]
app.simulation.load_mesh(path="res/cube.obj")

while app.running:
    app.step()
