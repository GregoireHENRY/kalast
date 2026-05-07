#!/usr/bin/env python

import numpy

import kalast


app = kalast.app.App()

app.config.global_color_mode = 0
app.config.debug_light_cube_show = True

app.config.shadow_normal_offset_scale = 2e-4
app.config.shadow_bias_scale = 1e-3
app.config.shadow_bias_minimum = 5e-4

app.simulation.sun.pos = [0.0, 50.0, 0.0]
app.simulation.sun.look_anchor()
app.simulation.sun.projection.set_orthographic()
app.simulation.sun.projection.side = 2.0
app.simulation.sun.projection.near = 0.1
app.simulation.sun.projection.far = 100.0

app.simulation.camera.pos = [0.0, 10.0, 0.0]
app.simulation.camera.look_anchor()

mat_spin_tilt = numpy.eye(4)
mat_spin_tilt[:3, :3] = kalast.util.mat_axis_angle(
    numpy.array([0.0, 1.0, 0.0]), kalast.util.PI
)

mat = mat_spin_tilt.copy()
app.simulation.load_mesh(
    # path="/Users/gregoireh/data/mesh/didymos/didymos_g_9309mm_spc_obj_0000n00000_v003_decimated_3072.obj",
    # path="/Users/gregoireh/data/mesh/didymos/didymos_g_9309mm_spc_obj_0000n00000_v003_decimated_1k.obj",
    path="/Users/gregoireh/data/mesh/didymos/didymos_g_9309mm_spc_obj_0000n00000_v003.obj",
    # path="/Users/gregoireh/data/mesh/didymos/didymos_g_1165mm_spc_obj_0000n00000_v003.obj",
    mat=mat,
    flatten=True,
)

mat = mat_spin_tilt.copy()
mat[0:3, 3] = [0.0, 1.2, 0.0]
app.simulation.load_mesh(
    # path="/Users/gregoireh/data/mesh/dimorphos/dimorphos_g_1940mm_spc_obj_0000n00000_v004_decimated_3072.obj",
    # path="/Users/gregoireh/data/mesh/dimorphos/dimorphos_g_1940mm_spc_obj_0000n00000_v004_decimated_1k.obj",
    path="/Users/gregoireh/data/mesh/dimorphos/dimorphos_g_1940mm_spc_obj_0000n00000_v004.obj",
    # path="/Users/gregoireh/data/mesh/dimorphos/dimorphos_g_0243mm_spc_obj_0000n00000_v004.obj",
    mat=mat,
    flatten=True,
)

mat = numpy.eye(4)
mat[:3, :3] = kalast.util.mat_axis_angle(numpy.array([0.0, 0.0, 1.0]), 0.01)


def tick(sim: kalast.app.simulation.Simulation, dt: float):
    if sim.state.is_paused:
        return

    sim.bodies[1].mat = mat @ sim.bodies[1].mat

    # p1 = sim.bodies[1].mat[:3, 3]
    # print(f"#{sim.state.iteration} {p1}")


app.tick = tick
app.start()
