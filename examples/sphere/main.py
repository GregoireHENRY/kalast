#!/usr/bin/env python

import numpy

import kalast
from kalast.app import App


app = App()
app.simulation.sun.pos = [10.0, 0.0, 0.0]
app.simulation.camera.pos = [13.5, -6.0, 5.0]
app.simulation.camera.look_anchor()

app.simulation.load_mesh(path="res/ico4.obj")  # try also res/sph2.obj

dt = 60.0  # seconds
period = 6.0 * 3600.0  # spin period (s)
spin_axis = numpy.array([0.0, 0.0, 1.0])

while app.running:
    if app.simulation.state.is_paused:
        app.step()
        continue

    t = app.simulation.state.iteration * dt

    bod = app.simulation.bodies[0]
    bod.mat[:3, :3] = kalast.util.mat_axis_angle(spin_axis, 2.0 * numpy.pi * t / period)

    app.step()