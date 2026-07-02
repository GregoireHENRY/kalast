from pathlib import Path

import pyrender  # noqa
import numpy
import trimesh
import matplotlib  # noqa
from matplotlib import pyplot  # noqa

import kalast
from kalast.util import DPR, RPD  # noqa


mesh = trimesh.load("moon.obj")
sph = numpy.array([kalast.util.cart2sph(v) for v in mesh.triangles_center])
print(f"nfaces={mesh.faces.shape[0]} nvert={mesh.vertices.shape[0]}")

equator = numpy.load("equator.npy")
meridian0 = numpy.load("meridian0.npy")
equator_meridian0 = numpy.load("equator_meridian0.npy")

path = Path("out")
et = numpy.load(path / "et.npy")
z = numpy.load(path / "z.npy")
sun = numpy.load(path / "sun.npy")
tmp_surf = numpy.load(path / "tmp_surf.npy")
tmp_cols = numpy.load(path / "tmp_cols.npy")
tmp_state = numpy.load(path / "tmp_state.npy")
print(f"ntime={et.size}")

kalast.plot.style.load()
kalast.plot.util.depth(z * 100, tmp_cols[::400, :], ylim=(z[-1] * 100, 0))
kalast.plot.util.daily_surf(
    et,
    tmp_cols[:, 0],
    # xlim=(0, None),
    xlabel="Ephemeris time",
    ylabel="Temperature [K]",
)

stmp = tmp_surf[-1, :]
kalast.plot.util.smap(mesh, sph, stmp)

# scene = pyrender.Scene(
#     ambient_light=[1.0, 1.0, 1.0], bg_color=[40.0 / 255, 71.0 / 255, 79.0 / 255]
# )
# rcam = pyrender.PerspectiveCamera(yfov=10.0 * RPD)
# ren = pyrender.OffscreenRenderer(1024, 768)
# pose = numpy.eye(4)
# pose[:3, 3] = [0, 0, 25]
# nc = scene.add(rcam, pose=pose)

cnorm = matplotlib.colors.Normalize(vmin=stmp.min(), vmax=stmp.max())
cmap = matplotlib.cm.inferno
mappable = matplotlib.cm.ScalarMappable(cmap=cmap, norm=cnorm)
cnormv = cnorm(stmp)
cmapv = cmap(cnormv)
mesh.vertices = mesh.vertices * 1e-3
mesh.unmerge_vertices()
mesh.visual.face_colors = cmapv
mesh.show()

# rmesh = pyrender.Mesh.from_trimesh(mesh, smooth=False)
# pose = numpy.eye(4)
# nb = scene.add(rmesh, pose=pose)
# pyrender.Viewer(scene)
# color, depth = ren.render(scene, flags=pyrender.constants.RenderFlags.FLAT)
# dpi = 100.0
# fig = pyplot.figure(figsize=(1024 / dpi, 768 / dpi))
# ax = pyplot.Axes(fig, [0.0, 0.0, 1.0, 1.0])
# ax.set_axis_off()
# fig.add_axes(ax)
# pyplot.imshow(color)
# pyplot.savefig("image.png", dpi=dpi)
