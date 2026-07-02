#!/usr/bin/env python

import numpy

import kalast


def tick(sim, dt):
    if sim.state.is_paused:
        return

    sim.bodies[1].mat = mat @ sim.bodies[1].mat

    # p1 = sim.bodies[1].mat[:3, 3]
    # print(f"#{sim.state.iteration} {p1}")


app = kalast.app.App()

app.config.color_mode = 0
app.config.debug_light_cube_show = True

app.config.shadow_normal_offset_scale = 2e-4
app.config.shadow_bias_scale = 1e-3
app.config.shadow_bias_minimum = 5e-4

app.simulation.sun.pos = [0.0, 20.0, 0.0]
# app.simulation.sun.up = [0.0, 1.0, 0.0]
app.simulation.sun.look_anchor()
app.simulation.sun.projection.side = 6.0
app.simulation.sun.projection.near = 10.0
app.simulation.sun.projection.far = 30.0

# app.simulation.camera.pos = [0.0, 30.0, 0.0]
# app.simulation.camera.up = [0.0, 0.0, 1.0]
# app.simulation.camera.look_anchor()

app.simulation.camera.pos = [-0.9687278, 13.656183, 7.445293]
app.simulation.camera.up = [0.03380529, -0.47655377, 0.8784952]
app.simulation.camera.dir = [0.06216155, -0.8762931, -0.47775126]

mat = numpy.eye(4)
app.simulation.load_mesh(path="res/ico3.obj", mat=mat, flatten=True)

mat = numpy.eye(4)
mat[:3, :3] *= numpy.eye(3) * 0.2
mat[0:3, 3] = [0.0, 5.0, 0.0]
app.simulation.load_mesh(path="res/ico3.obj", mat=mat, flatten=True)

mat = numpy.eye(4)
mat[:3, :3] = kalast.util.mat_axis_angle(numpy.array([0.0, 0.0, 1.0]), 0.01)

app.tick = tick
app.start()
