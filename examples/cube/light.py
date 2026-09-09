#!/usr/bin/env python

import numpy

import kalast


app = kalast.app.App()
app.simulation.config.debug_light_cube_show = True
# app.simulation.config.debug_light_cube_fit = True
app.simulation.config.wireframe_mode = 2
app.simulation.config.wireframe_color = [0.05, 0.05, 0.05, 1.0]
app.simulation.camera.pos = [18.0, 5.0, 10.0]
app.simulation.camera.look_anchor()
app.simulation.load_mesh(path="res/cube.obj", mat=numpy.eye(4), flatten=True)

while app.running:
    a = 0.01 * app.simulation.state.iteration
    app.simulation.sun.pos = [5.0 * numpy.cos(a), 5.0 * numpy.sin(a), 0.0]

    app.step()
