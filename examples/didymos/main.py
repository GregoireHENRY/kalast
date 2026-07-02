#!/usr/bin/env python

import numpy
import spiceypy as spice

import kalast

from kalast.util import AU_KM


def tick(sim: kalast.app.simulation.Simulation, dt: float):
    if sim.state.is_paused:
        return

    et = et0 + sim.state.iteration * 60.0

    (p_sun, _lt) = spice.spkpos("SUN", et, "ECLIPJ2000", "none", "DIDYMOS")
    (p_dimo, _lt) = spice.spkpos("DIMORPHOS", et, "ECLIPJ2000", "none", "DIDYMOS")
    m_didy_ej2k = spice.pxform("DIDYMOS_FIXED", "ECLIPJ2000", et)
    m_dimo_ej2k = spice.pxform("DIMORPHOS_FIXED", "ECLIPJ2000", et)

    sim.sun.pos = p_sun / AU_KM * 10.0
    sim.sun.look_anchor()

    sim.bodies[0].mat[:3, :3] = m_didy_ej2k
    sim.bodies[1].mat[:3, 3] = p_dimo
    sim.bodies[1].mat[:3, :3] = m_dimo_ej2k

    # sim.bodies[1].mat = mat @ sim.bodies[1].mat
    # p1 = sim.bodies[1].mat[:3, 3]
    # print(f"#{sim.state.iteration} {p1}")


app = kalast.app.App()

app.config.color_mode = 0
# app.config.debug_light_cube_show = True

app.config.shadow_normal_offset_scale = 2e-4
app.config.shadow_bias_scale = 1e-3
app.config.shadow_bias_minimum = 5e-4

app.simulation.sun.pos = [0.0, 50.0, 0.0]
app.simulation.sun.look_anchor()
app.simulation.sun.projection.side = 2.0
app.simulation.sun.projection.near = 0.1
app.simulation.sun.projection.far = 100.0

# app.simulation.camera.pos = [0.0, 10.0, 0.0]
# app.simulation.camera.look_anchor()

app.simulation.camera.pos=[-1.1002305, -3.005702, 2.0494902]
app.simulation.camera.up=[0.18536071, 0.50638384, 0.8421501]
app.simulation.camera.dir=[0.2894826, 0.7908329, -0.53924316]

# Can set orthographic camera projection to create ground-based telescope image.
# app.simulation.camera.projection.set_orthographic()
# app.simulation.camera.projection.side = 2.0

spice.kclear()
spice.furnsh(
    "/Users/gregoireh/data/spice/hera/kernels/mk/hera_study_PO_EMA_2024_local.tm"
)

et0 = spice.str2et("2027-03-01 12:00:00 UTC")
et = et0

(p_sun, _lt) = spice.spkpos("SUN", et, "ECLIPJ2000", "none", "DIDYMOS")
(p_dimo, _lt) = spice.spkpos("DIMORPHOS", et, "ECLIPJ2000", "none", "DIDYMOS")

m_didy_ej2k = spice.pxform("DIDYMOS_FIXED", "ECLIPJ2000", et)
m_dimo_ej2k = spice.pxform("DIMORPHOS_FIXED", "ECLIPJ2000", et)

# Custom spin axis
# mat_spin_tilt = numpy.eye(4)
# mat_spin_tilt[:3, :3] = kalast.util.mat_axis_angle(
#     numpy.array([0.0, 1.0, 0.0]), kalast.util.PI
# )
# mat = mat_spin_tilt.copy()

mat = numpy.eye(4)
mat[:3, :3] = m_didy_ej2k
app.simulation.load_mesh(
    # path="/Users/gregoireh/data/mesh/didymos/didymos_g_9309mm_spc_obj_0000n00000_v003_decimated_3072.obj",
    # path="/Users/gregoireh/data/mesh/didymos/didymos_g_9309mm_spc_obj_0000n00000_v003_decimated_1k.obj",
    path="/Users/gregoireh/data/mesh/didymos/didymos_g_9309mm_spc_obj_0000n00000_v003.obj",
    # path="/Users/gregoireh/data/mesh/didymos/didymos_g_1165mm_spc_obj_0000n00000_v003.obj",
    mat=mat,
    flatten=True,
)

# Custom dimorphos position
# mat[0:3, 3] = [0.0, 1.2, 0.0]

# matmul -> new = old @ mat

mat = numpy.eye(4)
mat[:3, 3] = p_dimo
mat[:3, :3] = m_dimo_ej2k
app.simulation.load_mesh(
    # path="/Users/gregoireh/data/mesh/dimorphos/dimorphos_g_1940mm_spc_obj_0000n00000_v004_decimated_3072.obj",
    # path="/Users/gregoireh/data/mesh/dimorphos/dimorphos_g_1940mm_spc_obj_0000n00000_v004_decimated_1k.obj",
    path="/Users/gregoireh/data/mesh/dimorphos/dimorphos_g_1940mm_spc_obj_0000n00000_v004.obj",
    # path="/Users/gregoireh/data/mesh/dimorphos/dimorphos_g_0243mm_spc_obj_0000n00000_v004.obj",
    mat=mat,
    flatten=True,
)

# mat = numpy.eye(4)
# mat[:3, :3] = kalast.util.mat_axis_angle(numpy.array([0.0, 0.0, 1.0]), 0.01)

app.tick = tick
app.start()

spice.kclear()