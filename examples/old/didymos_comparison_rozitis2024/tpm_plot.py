#!/usr/bin/env python

from pathlib import Path  # noqa

import pyrender  # noqa
import numpy
import trimesh
import matplotlib  # noqa
from matplotlib import pyplot  # noqa

import kalast
from kalast.util import DPR, RPD  # noqa


def ticks_short_formatter(x, pos):
    if x.is_integer():
        return str(int(x))
    else:
        return str(x)


body = "didymos"
# body = "dimorphos"

i1 = 0
i2 = 271
i3 = 542
istep = 10

mesh = trimesh.load(f"work/{body}/mesh.obj")
sph = numpy.array([kalast.util.cart2sph(v) for v in mesh.triangles_center])
print(f"nfaces={mesh.faces.shape[0]} nvert={mesh.vertices.shape[0]}")

equator = numpy.load(f"work/{body}/equator.npy")
meridian0 = numpy.load(f"work/{body}/meridian0.npy")
equator_meridian0 = numpy.load(f"work/{body}/equator_meridian0.npy")

et_simu = numpy.load("work/et_simu.npy")
z = numpy.load(f"work/{body}/z.npy")
sun = numpy.load(f"work/{body}/sun.npy")
tmp_cols = numpy.load(f"work/{body}/tmp_cols.npy")
tmp_state = numpy.load(f"work/{body}/tmp_state.npy")
tmp_surf = numpy.load(f"work/{body}/tmp_surf.npy")
tmp_surf_avg = numpy.load(f"work/{body}/tmp_surf_avg.npy")
tmp_surf_max = numpy.load(f"work/{body}/tmp_surf_max.npy")
tmp_surf_min = numpy.load(f"work/{body}/tmp_surf_min.npy")
print(f"ntime={et_simu.size}")

t_daily = (et_simu[i1:i3] - et_simu[i1]) / 3600.0
iif_daily = 0  # it is equator_meridian[0] already
# iif_daily = equator_meridian0[0]

# stmp = tmp_surf[0, :]
# stmp = tmp_surf_max
stmp = tmp_surf_min
# stmp = tmp_surf_avg

# norm = None
# norm = matplotlib.colors.Normalize(vmin=stmp.min(), vmax=stmp.max())
norm = matplotlib.colors.Normalize(vmin=50, vmax=350)

kalast.plot.style.load()

skip_basic_plot = False

if not skip_basic_plot:
    # kalast.plot.util.depth(z * 100, tmp_cols[::400, :], ylim=(z[-1] * 100, 0))
    kalast.plot.util.depth(z * 100, tmp_cols[i1:i2:istep, :], ylim=(z[-1] * 100, 0))
    kalast.plot.util.daily_surf(
        t_daily,
        tmp_cols[i1:i3, 0],
        # xlim=(0, None),
        xlabel="Elapsed hours",
        ylabel="Temperature [K]",
    )

    # kalast.plot.util.smap(mesh, sph, stmp)

    mappable = matplotlib.cm.ScalarMappable(
        cmap=matplotlib.cm.inferno.resampled(100), norm=norm
    )
    colors = mappable.to_rgba(stmp)
    kalast.plot.util.smap(
        mesh,
        colors,
        label="Temperature [K]",
        mappable=mappable,
        name="smap_tmp.png",
    )

# scene = pyrender.Scene(
#     ambient_light=[1.0, 1.0, 1.0], bg_color=[40.0 / 255, 71.0 / 255, 79.0 / 255]
# )
# rcam = pyrender.PerspectiveCamera(yfov=10.0 * RPD)
# ren = pyrender.OffscreenRenderer(1024, 768)
# pose = numpy.eye(4)
# pose[:3, 3] = [0, 0, 25]
# nc = scene.add(rcam, pose=pose)


# stmp = tmp_surf_max

cmap = matplotlib.cm.inferno
mappable = matplotlib.cm.ScalarMappable(cmap=cmap, norm=norm)

ambient_light = None
# ambient_light = [0.5, 0,5, 0.5]
# ambient_light = [1.0, 1.0, 1.0]
bg_color = None
# bg_color = [0.0, 0.0, 0.0]
scene = pyrender.Scene(ambient_light=ambient_light, bg_color=bg_color)

rcam = pyrender.PerspectiveCamera(yfov=5.0 * RPD)
pose = numpy.eye(4)
# pose[:3, 3] = [0, 0, 12]
pose[:3, 3] = [0, 0, -12]

# mat = kalast.util.mataxisang(0.0 * RPD, [1, 0, 0])
mat = kalast.util.mataxisang(180.0 * RPD, [1, 0, 0])

mat = numpy.array(mat)[:3, :3]
pose[:3, :3] = pose[:3, :3] @ mat

nc = scene.add(rcam, pose=pose)

rmesh = pyrender.Mesh.from_trimesh(mesh, smooth=False)
pose = numpy.eye(4)
nb = scene.add(rmesh, pose=pose)

ren = pyrender.OffscreenRenderer(1000, 1000)

no_axes = False

# extent = [-0.5, 0.5, -0.5, 0.5]
extent = [-0.5, 0.5, 0.5, -0.5]

dpi = 200.0
# fig, ax = pyplot.subplots(figsize=(500 / dpi, 500 / dpi))
fig, ax = pyplot.subplots(figsize=(5, 5))

if no_axes:
    ax.set_axis_off()
    ax = pyplot.Axes(fig, [0.0, 0.0, 1.0, 1.0])
    fig.add_axes(ax)

ticks = numpy.linspace(-0.5, 0.5, num=5)
pyplot.xticks(ticks)
pyplot.yticks(ticks)
formatter = matplotlib.ticker.FuncFormatter(ticks_short_formatter)
ax.xaxis.set_major_formatter(formatter)
ax.yaxis.set_major_formatter(formatter)
ax.set_xlabel("X (km)")
ax.set_ylabel("Y (km)")

for ii in range(i1, i3):
    normv = norm(stmp)
    # normv = norm(tmp_surf[ii, :])

    cmapv = cmap(normv)
    # mesh.vertices = mesh.vertices * 1e-3
    mesh.visual.face_colors = cmapv

    scene.remove_node(nb)
    rmesh = pyrender.Mesh.from_trimesh(mesh, smooth=False)
    nb = scene.add(rmesh, pose=pose)

    # pyrender.Viewer(
    #     scene,
    #     viewport_size=[1024, 768],
    #     # use_raymond_lighting=True
    # )

    color, depth = ren.render(scene, flags=pyrender.constants.RenderFlags.FLAT)

    ax.imshow(color, extent=extent, aspect="equal")

    pyplot.savefig(f"day/image_{ii}.png", dpi=dpi)
    # pyplot.show()
    exit()
