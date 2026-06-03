#!/usr/bin/env python

import numpy
import pandas
import spiceypy as spice

import kalast


app = kalast.app.App()
app.config.debug_app = True
app.config.debug_light_cube_show = True
app.config.width = 1024
app.config.height = 768
app.config.global_color_mode = 1

app.simulation.sun.pos = [50.0, 0.0, 0.0]

app.simulation.camera.pos = [50.0, 0.0, 0.0]
app.simulation.camera.dir = [-1.0, 0.0, 0.0]

mat = numpy.eye(4)
app.simulation.load_mesh(
    path="/Users/gregoireh/data/mesh/deimos/deimos_k005_tho_v02.obj",
    mat=mat,
    flatten=True,
)

# app.simulation.bodies[0].mesh


def tick(sim: kalast.app.simulation.Simulation, dt: float):
    if sim.state.iteration == 0:
        sim.export_once()

    if sim.state.is_paused:
        return

    # p1 = sim.bodies[1].mat[:3, 3]
    # print(f"#{sim.state.iteration} {p1}")


app.tick = tick
app.start()
