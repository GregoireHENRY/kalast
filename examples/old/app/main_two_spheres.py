#!/usr/bin/env python

import numpy

import kalast


app = kalast.app.App()

app.config.global_color_mode = 0
# app.config.debug_depth_show = True
# app.config.debug_light_cube_show = True

app.config.ambient_strength = 0.002
app.config.shadow_resolution = 4096

# Set camera pos/up
# Different methods to set dir
app.simulation.camera.pos = [0.0, 5.0, 0.0]
app.simulation.camera.up = [0.0, 0.0, 1.0]
app.simulation.camera.look_anchor()

# app.simulation.camera.projection.zfar = 100.0
# app.simulation.camera.projection.side = 1.0
# app.simulation.camera.projection.set_orthographic()
# app.simulation.camera.projection.fovy = 45.0 * kalast.util.RPD

app.simulation.sun = [0.0, 20.0, 0.0]

mesh = kalast.mesh.Mesh(path="res/ico4.obj", update_pos=lambda x: x * 0.3)
mesh.flatten()
mat = numpy.eye(4, dtype=numpy.float32)
# mat[0:3, 3] = [2.5, 0.0, 0.0]
app.simulation.add_body(mesh, mat=mat)

mesh = kalast.mesh.Mesh(path="res/ico4.obj")
mesh.flatten()
mat = numpy.eye(4, dtype=numpy.float32)
mat[0:3, 3] = [0.0, -3.0, 0.0]
app.simulation.add_body(mesh, mat=mat)


def tick(sim):
    # print(f"#{sim.state.iteration}")

    pass


app.tick = tick
app.start()
