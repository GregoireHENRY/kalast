#!/usr/bin/env python

import numpy
import pandas

import kalast

from kalast.util import AU, RPD, DPR  # noqa

app = kalast.app.App()
app.config.debug_app = True
app.config.debug_light_cube_show = True
app.config.width = 1024
app.config.height = 768
app.config.global_color_mode = 1

app.simulation.camera.pos = [50.0, 0.0, 0.0]
app.simulation.camera.dir = [-1.0, 0.0, 0.0]

mat = numpy.eye(4)
app.simulation.load_mesh(
    path="/Users/gregoireh/data/mesh/deimos/deimos_k005_tho_v02.obj",
    mat=mat,
    flatten=True,
)

mesh = app.simulation.bodies[0].mesh
centers = numpy.array([f.pos for f in mesh.facets])
sph = numpy.array([kalast.math.cart2sph(v) for v in centers])

equator = numpy.where(numpy.abs(sph[:, 2]) < 4 * RPD)[0]
meridian0 = numpy.where(numpy.abs(sph[:, 1]) < 4 * RPD)[0]
mix = numpy.intersect1d(equator, meridian0)

df = {}
df["index"] = equator
df = pandas.DataFrame(df)
df.to_csv("out/equator.csv", index=False, encoding="utf-8-sig")

df = {}
df["index"] = meridian0
df = pandas.DataFrame(df)
df.to_csv("out/meridian0.csv", index=False, encoding="utf-8-sig")

df = {}
df["index"] = mix
df = pandas.DataFrame(df)
df.to_csv("out/mix_equator_meridian0.csv", index=False, encoding="utf-8-sig")

# When mesh is flatten, vertices are duplicated by 3 times number of facets and their normals are fixed to facet's normal.
# This is so GPU can read mesh without a list of indices.
# So to edit colors per facet on a flattened mesh, you need to edit the 3 vertices of each facet.
# TODO: Can be shortened with a call: mesh.set_facet_colors(ii, color)
for iif in equator:
    mesh.colors[iif * 3 + 0, :] = [1.0, 0.0, 0.0]
    mesh.colors[iif * 3 + 1, :] = [1.0, 0.0, 0.0]
    mesh.colors[iif * 3 + 2, :] = [1.0, 0.0, 0.0]

for iif in meridian0:
    mesh.colors[iif * 3 + 0, :] = [0.0, 1.0, 0.0]
    mesh.colors[iif * 3 + 1, :] = [0.0, 1.0, 0.0]
    mesh.colors[iif * 3 + 2, :] = [0.0, 1.0, 0.0]

for iif in mix:
    mesh.colors[iif * 3 + 0, :] = [0.0, 0.0, 1.0]
    mesh.colors[iif * 3 + 1, :] = [0.0, 0.0, 1.0]
    mesh.colors[iif * 3 + 2, :] = [0.0, 0.0, 1.0]

# This is supposed to work as an alternative but doesnt work for some reason. Wrong facets are colored.
# for iif in equator:
#     for f in mesh.get_facet_colors(iif):
#         f[:] = [1.0, 0.0, 0.0]
#
# for iif in meridian0:
#     for f in mesh.get_facet_colors(iif):
#         f[:] = [0.0, 1.0, 0.0]
#
# for iif in mix:
#     for f in mesh.get_facet_colors(iif):
#         f[:] = [0.0, 0.0, 1.0]


def tick(sim: kalast.app.simulation.Simulation, dt: float):
    if sim.state.iteration == 0:
        sim.export_once()

    if sim.state.is_paused:
        return


app.tick = tick
app.start()
