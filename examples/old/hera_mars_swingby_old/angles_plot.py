#!/usr/bin/env python

from pathlib import Path  # noqa

import numpy
import trimesh
import matplotlib  # noqa
from matplotlib import pyplot  # noqa

import kalast
from kalast.util import DPR, RPD  # noqa


mesh = trimesh.load("work/deimos.obj")
sph = numpy.array([kalast.util.cart2sph(v) for v in mesh.triangles_center])
nf = mesh.faces.shape[0]
print(f"nfaces={nf} nvert={mesh.vertices.shape[0]}")

images = numpy.load("work/scene/images.npy", allow_pickle=True)
lola = numpy.load("work/scene/lola.npy")
inc_all = numpy.load("work/scene/inc_all.npy")
emi_all = numpy.load("work/scene/emi_all.npy")
ppha_all = numpy.load("work/scene/ppha_all.npy")
tmp_all = numpy.load("work/scene/tmp_all.npy")
rad_all = numpy.load("work/rad/rad_all.npy")
irrad_all = numpy.load("work/rad/rad_all.npy")

# R = numpy.load("work/rad/R.npy")
R = numpy.ones((images.size, nf))

cf = {}

it = 5

# dists = [
#     kalast.util.distance_haversine(
#         kalast.spice_entities.moon.diameter,
#         lola[it, 0],
#         lola[it, 1],
#         sph[iif, 0],
#         sph[iif, 1],
#     )
#     for iif in range(nf)
# ]
# iif = numpy.argmin(dists)
# cf[iif] = "#ff0000"

# numpy.interp(value, x, y)
# cmap = matplotlib.cm.bwr

# cticks = numpy.linspace(0, 10, num=11, endpoint=True)

kalast.plot.style.load()

cmap = matplotlib.cm.inferno.resampled(100)

norm = None
# norm = matplotlib.color.normalize(vmin=0, vmax=10)

# cmap1 = matplotlib.cm.Blues_r.resampled(10)
# cmap2 = matplotlib.cm.Reds.resampled(90)
# mappable = kalast.plot.cbar.custom_split_map(cmap1, cmap2, 0, 10)

# cmap = matplotlib.cm.seismic

cmap1 = matplotlib.cm.Blues_r.resampled(100)
cmap2 = matplotlib.cm.Reds.resampled(100)
colors1 = cmap1(numpy.linspace(0.0, 0.8, cmap1.N))
colors2 = cmap2(numpy.linspace(0.2, 1.0, cmap2.N))
colors = numpy.vstack((colors1, colors2))
colors[99:101, :3] = [1, 1, 1]
cmap_R = matplotlib.colors.LinearSegmentedColormap.from_list("newdiv", colors)
norm_R = kalast.plot.util.MidPointLogNorm(vmin=0.1, vmax=10, midpoint=1)


cols = ["inc", "emi", "ppha", "tmp", "rad", "R"]
labels = [
    "incidence [°]",
    "emission [°]",
    "projected phase angle [°]",
    "temperature [K]",
    "radiance [W/m2/sr]",
    "roughness factor",
]
arrs = [inc_all * DPR, emi_all * DPR, ppha_all * DPR, tmp_all, rad_all, R]
cmaps = [
    matplotlib.cm.cividis_r.resampled(100),
    matplotlib.cm.cividis_r.resampled(100),
    matplotlib.cm.cividis_r.resampled(100),
    matplotlib.cm.inferno.resampled(100),
    matplotlib.cm.inferno.resampled(100),
    cmap_R,
]
norms = [
    None,
    None,
    None,
    None,
    None,
    norm_R,
]

for ii in range(len(cols)):
    mappable = matplotlib.cm.ScalarMappable(cmap=cmaps[ii], norm=norms[ii])
    colors = mappable.to_rgba(arrs[ii][it, :])
    kalast.plot.util.smap(
        mesh, colors, label=labels[ii], mappable=mappable, name=f"smap_{cols[ii]}.png"
    )

# Time indices
# 0: 20241010T104607
# 168: 20241014T054521
# 226: 20241018T223118

# labels
# Incidence (°)
# Emission (°)
# Phase angle (°)
# Projected phase angle (°)
# Temperature (K)
# Radiance (W/m2/sr)
# Irradiance (W/m2)
# Roughness factor
