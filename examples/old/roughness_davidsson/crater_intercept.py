#!/usr/bin/env python

import numpy
import glm
import matplotlib
from matplotlib import pyplot

import kalast


# params

spin_period = 6.0 * 3600.0

# sun = numpy.array([0.0, 0.0, 1.0])
# sun = numpy.array([-1.0, 0.0, 1.0])
sun = numpy.array([-1.0, 0.0, 0.0])

sun /= glm.length(sun)
sun *= kalast.util.AU
# sun *= 1000.0

spin_axis_sun = numpy.array([0.0, 1.0, 0.0])

# load mesh

mesh = kalast.mesh.Mesh("res/plane_crater_1024-5000_h=0.437.obj", lambda x: x * 1e3)
nf = len(mesh.facets)

fp = numpy.array([f.p for f in mesh.facets])
fn = numpy.array([f.n for f in mesh.facets])
xyz = numpy.array([v.pos for v in mesh.vertices])
triangles = numpy.array([mesh.get_indices_facet(iif) for iif in range(0, nf)])
ii_center_4 = numpy.array([496, 528, 1519, 1551])
ii_flat = 0

# facet #517
# a [-343.75, 0, -269.484]
# b [-312.5, 31.25, -303.497]
# c [-343.75, 31.25, -267.44]

# facet #539
# a [375, 0, -223.962]
# b [343.75, 31.25, -267.44]
# c [343.75, 0, -269.484]

# time loop

inc = numpy.zeros(nf)
shadowed = numpy.zeros(nf)

THRESHOLD_PARALLEl = 1e-6

for iif in range(0, nf):
    p = fp[iif]
    n = fn[iif]

    v_sun = sun - p
    u_sun = v_sun / glm.length(v_sun)
    cos_inc = kalast.astro.cosine_incidence(u_sun, n)
    inc_ = numpy.acos(cos_inc)
    inc[iif] = inc_

    # If ray is parallel to facet normal, no interception test
    det = -u_sun @ n
    if det > -THRESHOLD_PARALLEl and det < THRESHOLD_PARALLEl:
        continue

    # If ray is not facing facet normal, no interception test
    if det >= 0.0:
        continue

    r = mesh.intersect(sun, -u_sun)

    # If ray is intercepted by another facet, mark it shadowed
    if r is not None and r[0] != iif:
        shadowed[iif] = 1.0

cmap = matplotlib.cm.cividis_r  # .resampled(100)
norm = matplotlib.colors.Normalize(vmin=0.0, vmax=90.0)

# cmap = matplotlib.cm.Greys
# norm = matplotlib.colors.Normalize(vmin=0.0, vmax=1.0)

mappable = matplotlib.cm.ScalarMappable(cmap=cmap, norm=norm)

# kalast.plot.style.load()
fig = pyplot.figure()
ax = fig.add_subplot(projection="3d")
p_ = ax.plot_trisurf(
    xyz[:, 0], xyz[:, 1], xyz[:, 2], triangles=triangles, lw=0.5, ec="k"
)
# p_.set_fc(colors)
# ax.set_zlim(-1, 1)

ax.set_aspect("equal")
ax.set_xlabel("x")
ax.set_ylabel("y")
ax.set_zlabel("z")
ax.set_proj_type(
    "persp",
    focal_length=0.4,  # 136°
)

data = inc * kalast.util.DPR
idx_shadow = numpy.where(shadowed == 1.0)[0]
data[idx_shadow] = 90.0

# data = shadowed

colors = mappable.to_rgba(data)
p_.set_fc(colors)

ax.view_init(elev=30.0, azim=-60.0)
fig.savefig("out/1.png", bbox_inches="tight", dpi=400)
ax.view_init(elev=75.0, azim=-70.0)
fig.savefig("out/2.png", bbox_inches="tight", dpi=400)

pyplot.show()
