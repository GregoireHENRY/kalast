from pathlib import Path  # noqa

import pyrender
import numpy
import trimesh
from matplotlib import pyplot  # noqa
import matplotlib
import spiceypy as spice
import glm  # noqa

import kalast
from kalast.util import DPR, RPD, AU  # noqa


spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm")

sc = kalast.spice_entities.hera
cam = kalast.spice_entities.tiri
bod = kalast.spice_entities.deimos
frame = "j2000"

mesh = trimesh.load("work/deimos.obj")
sph = numpy.array([kalast.util.cart2sph(v) for v in mesh.triangles_center])
nf = mesh.faces.shape[0]

images = numpy.load("work/scene/images.npy", allow_pickle=True)
et_images = numpy.load("work/scene/et_images.npy")
inc_all = numpy.load("work/scene/inc_all.npy")

cmap = matplotlib.cm.gray_r.resampled(255)
cnorm = matplotlib.colors.Normalize(vmin=0, vmax=90)
mappable = matplotlib.cm.ScalarMappable(cmap=cmap, norm=cnorm)

scene = pyrender.Scene(ambient_light=[1.0, 1.0, 1.0], bg_color=[0.0, 0.0, 0.0])
rcam = pyrender.PerspectiveCamera(yfov=10.0 * RPD)
ren = pyrender.OffscreenRenderer(1024, 768)
init_nodes = True

ogl_cam_frame = numpy.array([[-1, 0, 0], [0, 1, 0], [0, 0, -1]])

# mesh.unmerge_vertices()
color = None

ii = 5

(sun, _lt) = spice.spkpos("sun", et_images[ii], bod.frame, "none", bod.name)
(psc, _lt) = spice.spkpos(sc.name, et_images[ii], bod.frame, "none", bod.name)
m_s2b = spice.pxform(cam.frame, bod.frame, et_images[ii])

cnormv = cnorm(inc_all[ii] * DPR)
mesh.visual.face_colors = cmap(cnormv)

pose = numpy.eye(4)
pose[:3, 3] = psc
pose[:3, :3] = m_s2b @ ogl_cam_frame

if init_nodes:
    nc = scene.add(rcam, pose=pose)
else:
    scene.set_pose(nc, pose)

m_s2b = spice.pxform(bod.frame, bod.frame, et_images[ii])
(pbod, _lt) = spice.spkpos(bod.name, et_images[ii], cam.frame, "none", sc.name)
pbod

rmesh = pyrender.Mesh.from_trimesh(mesh, smooth=False)
pose = numpy.eye(4)
# pose[:3, 3] = pbod @ ogl_cam_frame
# pose[:3, :3] = m_s2b

if init_nodes:
    nb = scene.add(rmesh, pose=pose)
else:
    scene.set_pose(nb, pose)

color, depth = ren.render(scene, flags=pyrender.constants.RenderFlags.FLAT)
# pyrender.Viewer(scene, viewport_size=[1024, 768])

if color is not None:
    dpi = 100.0
    fig = pyplot.figure(figsize=(1024 / dpi, 768 / dpi))
    # pyplot.axis("off")
    ax = pyplot.Axes(fig, [0.0, 0.0, 1.0, 1.0])
    ax.set_axis_off()
    fig.add_axes(ax)
    pyplot.imshow(color, cmap=pyplot.cm.gray_r)
    pyplot.savefig("scene.png", dpi=dpi)
