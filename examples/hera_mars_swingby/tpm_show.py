#!/usr/bin/env python

import matplotlib
import numpy
import pandas

import kalast

from kalast.util import AU, RPD, DPR  # noqa


app = kalast.app.App()
app.config.debug_app = True
# app.config.debug_window = True
app.config.debug_light_cube_show = True
app.config.width = 1024
app.config.height = 768
app.config.color_mode = 1

app.simulation.camera.pos = [50.0, 0.0, 0.0]
app.simulation.camera.dir = [-1.0, 0.0, 0.0]

mat = numpy.eye(4)
app.simulation.load_mesh(
    path="/Users/gregoireh/data/mesh/deimos/deimos_k005_tho_v02.obj",
    mat=mat,
    flatten=True,
)
mesh = app.simulation.bodies[0].mesh
ets = pandas.read_csv("out/ets_sim.csv").to_numpy()
state = pandas.read_csv("out/state.csv").to_numpy()
tmp_surf = pandas.read_csv("out/tmp_surf.csv").to_numpy()

nface = len(mesh.facets)
nit = ets.size

mappable = matplotlib.cm.ScalarMappable(
    cmap=matplotlib.cm.inferno.resampled(100), norm=None
)

# app.simulation.state.is_paused = True


# Should be in time loop
# colors = mappable.to_rgba(tmp_surf[0, :])
# for iif in range(nface):
#     mesh.colors[iif * 3 + 0, :] = colors[iif, :3]
#     mesh.colors[iif * 3 + 1, :] = colors[iif, :3]
#     mesh.colors[iif * 3 + 2, :] = colors[iif, :3]


def tick(sim: kalast.app.simulation.Simulation, dt: float):
    if sim.state.iteration == nit and not sim.state.is_paused:
        sim.state.is_paused = True

    if sim.state.is_paused:
        return

    print(f"{sim.state.iteration}/{nit}")

    # sun = state[sim.state.iteration, :3]
    colors = mappable.to_rgba(tmp_surf[sim.state.iteration, :])
    for iif in range(nface):
        mesh.colors[iif * 3 + 0, :] = colors[iif, :3]
        mesh.colors[iif * 3 + 1, :] = colors[iif, :3]
        mesh.colors[iif * 3 + 2, :] = colors[iif, :3]

    sim.export_once()


app.tick = tick
app.start()
