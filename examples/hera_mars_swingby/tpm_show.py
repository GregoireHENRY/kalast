#!/usr/bin/env python

import matplotlib
import numpy
import pandas
from pathlib import Path  # noqa

import kalast

from kalast.util import AU, RPD, DPR  # noqa


def tick(sim: kalast.app.simulation.Simulation, dt: float):
    if sim.state.iteration == nit and not sim.state.is_paused:
        sim.state.is_paused = True

    if sim.state.is_paused:
        return

    print(f"{sim.state.iteration}/{nit - 1}")

    # sun = state[sim.state.iteration, :3]
    colors = mappable.to_rgba(tmp_surf[sim.state.iteration, :])
    for iif in range(nf):
        mesh.colors[iif * 3 + 0, :] = colors[iif, :3]
        mesh.colors[iif * 3 + 1, :] = colors[iif, :3]
        mesh.colors[iif * 3 + 2, :] = colors[iif, :3]

    sim.export_once()


app = kalast.app.App()
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

# Load TPM
#
# ets:
#   date_start_sim = "2025-03-12 00:00"
#   date_stop = "2025-03-12 15:00"
#   dt_sim = 300
path = Path("out/hera_mars_swingby/deimos_tpm_2")
ets = pandas.read_csv(path / "ets_sim.csv").to_numpy()
state = pandas.read_csv(path / "state.csv").to_numpy()
tmp_surf = pandas.read_csv(path / "tmp_surf.csv").to_numpy()
nf = len(mesh.facets)
nit = ets.size
mappable = matplotlib.cm.ScalarMappable(
    cmap=matplotlib.cm.inferno.resampled(100), norm=None
)

app.tick = tick
app.start()
