#!/usr/bin/env python

import matplotlib
import numpy  # noqa
import pandas  # noqa
import spiceypy as spice
from pathlib import Path  # noqa

import kalast
from kalast.util import AU, AU_KM, RPD, DPR, PI  # noqa
from kalast.entity import MARS, DEIMOS, PHOBOS  # noqa


def tick(sim: kalast.app.simulation.Simulation, dt: float):
    global et

    if sim.state.is_paused:
        return

    if et - et0 >= 3600.0 * 4.0:
        return

    sim.export_once()

    if sim.state.iteration > 0:
        et += 60.0

    (p_deimos, _lt) = spice.spkpos("deimos", et, instr, "none", instr)
    d_deimos = numpy.linalg.norm(p_deimos)
    m_deimos_tiri = spice.pxform("iau_deimos", instr, et)

    sim.bodies[0].mat[:3, :3] = m_deimos_tiri
    sim.bodies[0].mat[:3, 3] = p_deimos

    colors = mappable.to_rgba(data[sim.state.iteration, :])
    for iif in range(nf):
        mesh.colors[iif * 3 + 0, :] = colors[iif, :3]
        mesh.colors[iif * 3 + 1, :] = colors[iif, :3]
        mesh.colors[iif * 3 + 2, :] = colors[iif, :3]

    print(f"it={sim.state.iteration} d_deimos={d_deimos:.5e}")


app = kalast.app.App()
app.config.width = 768
app.config.height = 1024
app.config.color_mode = 1

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

(p_deimos, _lt) = spice.spkpos("deimos", et, instr, "none", instr)
d_deimos = numpy.linalg.norm(p_deimos)
m_deimos_tiri = spice.pxform("iau_deimos", instr, et)

print(f"deimos_radius={DEIMOS.radii.mean() * 1e-3:.5e}km")
print(f"d_deimos={d_deimos:.5e}km p={p_deimos} ")
print()

# app.config.debug_light_cube_show = True
# app.config.light_cube_scale = 10.0

app.simulation.camera.pos = [0.0, 0.0, 0.0]
app.simulation.camera.up = [1.0, 0.0, 0.0]
app.simulation.camera.dir = [0.0, 0.0, 1.0]
app.simulation.camera.anchor = p_deimos
app.simulation.camera.set_control_none()
app.simulation.camera.projection.near = 1.0e2
app.simulation.camera.projection.far = 1.0e6
app.simulation.camera.projection.fovy = 13.3 * RPD
app.simulation.camera.up_world = [0.0, 1.0, 0.0]

mat = numpy.eye(4)
mat[:3, :3] = m_deimos_tiri
mat[0:3, 3] = p_deimos
app.simulation.load_mesh(
    path="/Users/gregoireh/data/mesh/deimos/deimos_k005_tho_v02.obj",
    mat=mat,
    flatten=True,
)

# Get Deimos mesh
mesh = app.simulation.bodies[0].mesh

# Load TPM
#
# deimos_tpm_2:
#   date_start_sim = "2025-03-12 00:00"
#   date_stop = "2025-03-12 15:00"
#   dt_sim = 300.0
#
# deimos_tpm_3:
#   date_start_sim = "2025-03-12 10:00"
#   date_stop = "2025-03-12 14:00"
#   dt_sim = 60.0
#
# deimos_tpm_4 (on-going):
#   date_start_sim = "2025-03-11 00:00"
#   date_stop = "2025-03-13 00:00"
#   dt_sim = 300.0
path = Path("out/hera_mars_swingby/deimos_tpm_3")
ets = pandas.read_csv(path / "ets_sim.csv").to_numpy()
state = pandas.read_csv(path / "state.csv").to_numpy()

# data = pandas.read_csv(path / "tmp_surf.csv").to_numpy()
data = pandas.read_csv(path / "rad_all.csv").to_numpy()
# data = pandas.read_csv(path / "irrad_all.csv").to_numpy()

nf = len(mesh.facets)
nit = ets.size

# Fixed bounds over the whole animation (min/max across all timesteps and
# facets), so colors are comparable frame to frame. With norm=None,
# ScalarMappable would instead autoscale once on the first tick's data and
# then keep those bounds locked for every later frame -- not a per-frame
# rescale, and not the global range either.
vmin = data.min()
vmax = data.max()
norm = matplotlib.colors.Normalize(vmin=vmin, vmax=vmax)
mappable = matplotlib.cm.ScalarMappable(
    cmap=matplotlib.cm.gray.resampled(100), norm=norm
)

app.tick = tick
app.start()

# To export colormap:
# cbar_params = kalast.plot.cbar.Params()
# cbar_params.vmin = 0
# cbar_params.vmax = 25.0
# cbar_params.dv = 5.0
# cbar_params.label = "Radiance [W/m2/sr]"
# cbar_params.mappable = mappable
# cbar_params.path = path / "cbar.png"
# kalast.plot.style.load()
# kalast.plot.cbar.create(cbar_params)
# matplotlib.pyplot.show()
