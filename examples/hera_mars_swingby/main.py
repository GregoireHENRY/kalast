#!/usr/bin/env python

import numpy  # noqa
import pandas  # noqa
import spiceypy as spice

import kalast
from kalast.util import AU, AU_KM, RPD, DPR, PI  # noqa
from kalast.entity import MARS, DEIMOS, PHOBOS  # noqa


def tick(sim: kalast.app.simulation.Simulation, dt: float):
    global et

    if sim.state.is_paused:
        return

    if et - et0 > 240.0 * 60.0:
        return

    sim.export_once()

    if sim.state.iteration > 0:
        et += 60.0

    (p_sun, _lt) = spice.spkpos("sun", et, instr, "none", instr)
    (p_mars, _lt) = spice.spkpos("mars", et, instr, "none", instr)
    (p_phobos, _lt) = spice.spkpos("phobos", et, instr, "none", instr)
    (p_deimos, _lt) = spice.spkpos("deimos", et, instr, "none", instr)

    d_mars = numpy.linalg.norm(p_mars)
    d_deimos = numpy.linalg.norm(p_deimos)
    d_phobos = numpy.linalg.norm(p_phobos)

    m_mars_tiri = spice.pxform("iau_mars", instr, et)
    m_deimos_tiri = spice.pxform("iau_deimos", instr, et)
    m_phobos_tiri = spice.pxform("iau_phobos", instr, et)

    sim.sun.pos = p_sun
    sim.sun.look_anchor()

    sim.bodies[0].mat[:3, :3] = mat_radii_mars @ m_mars_tiri
    sim.bodies[0].mat[:3, 3] = p_mars

    sim.bodies[1].mat[:3, :3] = m_phobos_tiri
    sim.bodies[1].mat[:3, 3] = p_phobos

    sim.bodies[2].mat[:3, :3] = m_deimos_tiri
    sim.bodies[2].mat[:3, 3] = p_deimos

    print(
        f"it={sim.state.iteration} d_mars={d_mars:.5e} d_deimos={d_deimos:.5e} d_phobos={d_phobos:.5e}"
    )


app = kalast.app.App()
app.config.width = 1024
app.config.height = 768
app.config.color_mode = 0

app.config.shadow_normal_offset_scale = 2e-4
app.config.shadow_bias_scale = 1e-3
app.config.shadow_bias_minimum = 5e-4

spice.kclear()
spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm")

# Load list of images later and reproduce them
# df = pandas.read_csv(
#     "/Users/gregoireh/data/hera/tiri/tiri_images_mars_swing-by_deimos.csv"
# )
# images = df["image"].to_list()
# et_images = df["et"].to_numpy()

et0 = spice.str2et("2025-03-12 10:00:00 UTC")
et = et0

# deltet = spice.deltet(et, "et")
# print(et, deltet)

instr = "hera_tiri"

(p_sun, _lt) = spice.spkpos("sun", et, instr, "none", instr)
(p_mars, _lt) = spice.spkpos("mars", et, instr, "none", instr)
(p_phobos, _lt) = spice.spkpos("phobos", et, instr, "none", instr)
(p_deimos, _lt) = spice.spkpos("deimos", et, instr, "none", instr)

d_sun = numpy.linalg.norm(p_sun)
d_sun_au = d_sun / AU_KM
d_mars = numpy.linalg.norm(p_mars)
d_phobos = numpy.linalg.norm(p_phobos)
d_deimos = numpy.linalg.norm(p_deimos)

m_mars_tiri = spice.pxform("iau_mars", instr, et)
m_deimos_tiri = spice.pxform("iau_deimos", instr, et)
m_phobos_tiri = spice.pxform("iau_phobos", instr, et)

print(f"mars_radius={MARS.radii.mean() * 1e-3:.5e}km")
print(f"deimos_radius={DEIMOS.radii.mean() * 1e-3:.5e}km")
print(f"d_sun_au={d_sun_au:.5f}AU, d_sun={d_sun:.5e}, p={p_sun}")
print(f"d_mars={d_mars:.5e}km p={p_mars}")
print(f"d_phobos={d_phobos:.5e}km p={p_phobos} ")
print(f"d_deimos={d_deimos:.5e}km p={p_deimos} ")
print()

# app.config.debug_light_cube_show = True
# app.config.light_cube_scale = 10.0

app.simulation.sun.pos = p_sun
app.simulation.sun.up = [0.0, 1.0, 0.0]
app.simulation.sun.set_target(p_mars)
app.simulation.sun.projection.side = 5.0e4
app.simulation.sun.projection.near = 1.0e7
app.simulation.sun.projection.far = 1.0e9

app.simulation.camera.pos = [0.0, 0.0, 0.0]
app.simulation.camera.up = [0.0, 1.0, 0.0]
app.simulation.camera.dir = [0.0, 0.0, 1.0]
app.simulation.camera.anchor = p_deimos
app.simulation.camera.set_control_none()
app.simulation.camera.projection.near = 1.0e2
app.simulation.camera.projection.far = 1.0e6
app.simulation.camera.projection.fovy = 11 * RPD
app.simulation.camera.up_world = [0.0, 1.0, 0.0]


mat = numpy.eye(4)
mat_radii_mars = numpy.eye(3) * MARS.radii * 1e-3
mat[:3, :3] = mat_radii_mars @ m_mars_tiri
mat[0:3, 3] = p_mars
app.simulation.load_mesh(
    path="res/ico5.obj",
    mat=mat,
    flatten=True,
)

mat = numpy.eye(4)
mat[:3, :3] = m_phobos_tiri
mat[0:3, 3] = p_phobos
app.simulation.load_mesh(
    path="/Users/gregoireh/data/mesh/phobos/phobos_m003_gas_v01_simplified_10000.obj",
    mat=mat,
    flatten=True,
)

mat = numpy.eye(4)
mat[:3, :3] = m_deimos_tiri
mat[0:3, 3] = p_deimos
app.simulation.load_mesh(
    path="/Users/gregoireh/data/mesh/deimos/deimos_k005_tho_v02.obj",
    mat=mat,
    flatten=True,
)

app.tick = tick
app.start()
