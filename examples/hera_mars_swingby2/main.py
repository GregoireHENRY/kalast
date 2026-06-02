#!/usr/bin/env python

import numpy
import pandas
import spiceypy as spice

import kalast


app = kalast.app.App()
app.config.debug_app = True
# app.config.debug_window = True
# app.config.debug_simulation = True
app.config.debug_light_cube_show = True

app.config.width = 1024
app.config.height = 768

app.config.global_color_mode = 0

app.config.shadow_normal_offset_scale = 2e-4
app.config.shadow_bias_scale = 1e-3
app.config.shadow_bias_minimum = 5e-4


spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm")

df = pandas.read_csv(
    "/Users/gregoireh/data/hera/tiri/tiri_images_mars_swing-by_deimos.csv"
)
images = df["image"].to_list()
et_images = df["et"].to_numpy()

et0 = spice.str2et("2025-03-12 12:09:00 UTC")
et = et0

# deltet = spice.deltet(et, "et")
# print(et, deltet)

instr = "hera_tiri"

(p_sun, _lt) = spice.spkpos("sun", et, instr, "none", instr)
(p_mars, _lt) = spice.spkpos("mars", et, instr, "none", instr)
(p_phobos, _lt) = spice.spkpos("phobos", et, instr, "none", instr)
(p_deimos, _lt) = spice.spkpos("deimos", et, instr, "none", instr)

p_sun = [0.0, 0.0, -1.0 * kalast.util.AU / 1e3]

d_sun_au = numpy.linalg.norm(p_sun) * 1e3 / kalast.util.AU
d_mars = numpy.linalg.norm(p_mars)
d_phobos = numpy.linalg.norm(p_phobos)
d_deimos = numpy.linalg.norm(p_deimos)

m_mars_tiri = spice.pxform("iau_mars", instr, et)
m_deimos_tiri = spice.pxform("iau_deimos", instr, et)
m_phobos_tiri = spice.pxform("iau_phobos", instr, et)

print(f"sun={d_sun_au:.5f}AU, p={p_sun}")
print(f"mars={d_mars:.5e}km p={p_mars}")
print(f"phobos={d_phobos:.5e}km p={p_phobos} ")
print(f"deimos={d_deimos:.5e}km p={p_deimos} ")
print()

app.config.light_cube_scale = 10.0

app.simulation.sun.pos = p_sun
app.simulation.sun.up = [0.0, 1.0, 0.0]
app.simulation.sun.set_target(p_mars)
app.simulation.sun.projection.side = 4.0e4
app.simulation.sun.projection.near = 1.0e7
app.simulation.sun.projection.far = 1.0e9

app.simulation.camera.pos = [0.0, 0.0, 0.0]
app.simulation.camera.up = [0.0, 1.0, 0.0]
app.simulation.camera.dir = [0.0, 0.0, 1.0]
app.simulation.camera.anchor = p_deimos
app.simulation.camera.set_control_none()
app.simulation.camera.projection.near = 1.0e2
app.simulation.camera.projection.far = 1.0e5
app.simulation.camera.projection.fovy = 10.0 * kalast.util.RPD
# app.simulation.camera.projection.fovy = 5.5 * kalast.util.RPD
app.simulation.camera.up_world = [0.0, 1.0, 0.0]

mat_resize = numpy.eye(4)
mat_resize[:3, :3] *= kalast.entity.MARS.radii * 1e-3

mat_spin_tilt = numpy.eye(4)
mat_spin_tilt[:3, :3] = kalast.util.mat_axis_angle(
    numpy.array([0.0, 1.0, 0.0]),
    0.0,
    # kalast.util.PI
)

mat = mat_resize @ mat_spin_tilt
mat[0:3, 3] = p_mars
app.simulation.load_mesh(
    path="res/ico5.obj",
    mat=mat,
    flatten=True,
)

mat_resize = numpy.eye(4)
mat_resize[:3, :3] *= 1.0

mat = mat_resize @ mat_spin_tilt
mat[0:3, 3] = p_phobos

app.simulation.load_mesh(
    path="/Users/gregoireh/data/mesh/phobos/phobos_m003_gas_v01_simplified_10000.obj",
    mat=mat,
    flatten=True,
)

mat_resize = numpy.eye(4)
mat_resize[:3, :3] *= 1.0

mat = mat_resize @ mat_spin_tilt
mat[0:3, 0:3] = mat[0:3, 0:3] @ m_deimos_tiri
mat[0:3, 3] = p_deimos

app.simulation.load_mesh(
    path="/Users/gregoireh/data/mesh/deimos/deimos_k005_tho_v02.obj",
    mat=mat,
    flatten=True,
)


def tick(sim: kalast.app.simulation.Simulation, dt: float):
    if sim.state.iteration == 0:
        sim.export_once()

    if sim.state.is_paused:
        return

    # p1 = sim.bodies[1].mat[:3, 3]
    # print(f"#{sim.state.iteration} {p1}")


app.tick = tick
app.start()
