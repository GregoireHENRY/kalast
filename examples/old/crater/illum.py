#!/usr/bin/env python

import numpy
import glm
import matplotlib
from matplotlib import pyplot

import kalast


# params

a = 0.1

spin_period = 6.0 * 3600.0

dt = 120.0
tf = 1.0 * spin_period

# tf = 240.0

# dt = 1350.0
# tf = 5400.0

sun = numpy.array([0.0, 0.0, 1.0]) * kalast.util.AU
spin_axis_sun = numpy.array([0.0, 1.0, 0.0])

# computed automatically

t = numpy.arange(0, tf + dt, dt)
nt = t.size
# nt = int(numpy.ceil(tf / dt)) + 1
# t = numpy.linspace(0, tf, nt, endpoint=True)

m_spin_sun = numpy.array(glm.rotate(2.0 * numpy.pi * dt / spin_period, spin_axis_sun))[
    :3, :3
]

# load mesh

mesh = kalast.mesh.Mesh("res/plane_crater_1024-5000_h=0.437.obj", lambda x: x * 1e3)
nf = len(mesh.facets)


fp = numpy.array([f.p for f in mesh.facets])
fn = numpy.array([f.n for f in mesh.facets])
xyz = numpy.array([v.pos for v in mesh.vertices])
triangles = numpy.array([mesh.get_indices_facet(iif) for iif in range(0, nf)])
ii_center_4 = numpy.array([496, 528, 1519, 1551])
ii_flat = 0

# time loop
# computation illumination angles

inc = numpy.zeros((nt, nf))
shadowed = numpy.zeros((nt, nf))

THRESHOLD_PARALLEl = 1e-6

# Time loop progress.
progress_freq = "5"
digits = [len(_d) for _d in progress_freq.split(".")]
digits_full = 3
digits_decimal = 0
if len(digits) == 2:
    digits_decimal = digits[1]
    if digits_decimal > 0:
        digits_full += digits_decimal + 1
freqv = float(progress_freq)
last_freq_reached = -freqv
ndigits = kalast.util.numdigits_comma(freqv)
digit = 10**ndigits

for iit in range(0, nt):
    if iit > 0:
        sun = sun @ m_spin_sun

    text = ""
    # text = f": {sun / glm.length(sun) * 1000.0}"

    for iif in range(0, nf):
        p = fp[iif]
        n = fn[iif]

        v_sun = sun - p
        u_sun = v_sun / glm.length(v_sun)
        cos_inc = kalast.astro.cosine_incidence(u_sun, n)
        inc_ = numpy.acos(cos_inc)
        inc[iit, iif] = inc_

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
            shadowed[iit, iif] = 1.0

    # Show progress
    progress = iit / (nt - 1) * 100
    if ndigits > 0:
        progress = numpy.floor(progress * digit) / digit
    if progress >= last_freq_reached + freqv:
        last_freq_reached += freqv
        print(
            f"{progress:{digits_full}.{digits_decimal}f}% ({iit:,}/{nt - 1:,}it){text}"
        )


# cmap = matplotlib.cm.cividis_r  # .resampled(100)
# norm = matplotlib.colors.Normalize(vmin=0.0, vmax=90.0)

cmap = matplotlib.cm.Greys
norm = matplotlib.colors.Normalize(vmin=0.0, vmax=1.0)

mappable = matplotlib.cm.ScalarMappable(cmap=cmap, norm=norm)

# iit = 20
# kalast.plot.style.load()
# colors = mappable.to_rgba(inc[iit, :] * kalast.util.DPR)
# kalast.plot.util.smap(
#     mesh, colors, label="incidence [deg]", mappable=mappable, name="map.png"
# )


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
    # focal_length=0.5,  # 127°
    # focal_length=1.0  # 90°
    # focal_length=2.4142  # 45°
)

last_freq_reached = -freqv
for iit in range(0, nt):
    # data = inc[iit, :] * kalast.util.DPR
    # idx_shadow = numpy.where(shadowed[iit, :] == 1.0)[0]
    # data[idx_shadow] = 90.0

    data = shadowed[iit, :]

    colors = mappable.to_rgba(data)
    p_.set_fc(colors)

    ax.view_init(elev=30.0, azim=-60.0)
    fig.savefig(f"out/vid/1/{iit}.png", bbox_inches="tight", dpi=400)
    ax.view_init(elev=75.0, azim=-70.0)
    fig.savefig(f"out/vid/2/{iit}.png", bbox_inches="tight", dpi=400)

    # Show progress
    progress = iit / (nt - 1) * 100
    if ndigits > 0:
        progress = numpy.floor(progress * digit) / digit
    if progress >= last_freq_reached + freqv:
        last_freq_reached += freqv
        print(f"{progress:{digits_full}.{digits_decimal}f}% ({iit:,}/{nt - 1:,}it)")

pyplot.show()


# kalast.plot.style.load()
# fig, ax = pyplot.subplots(figsize=(6, 4))
# ax.set_xlabel("Time [h]")
# ax.set_ylabel("Incidence [deg]")
# ax.plot(
#     t / 3600.0,
#     inc[:, ii_center_4].mean(axis=1) * kalast.util.DPR,
#     lw=1,
#     color="k",
#     marker="s",
#     ms=2,
# )
# # ax.set_xlim(4, 16)
# # ax.set_yscale("log")
# fig.savefig("plot2d.png", bbox_inches="tight", dpi=300)
# pyplot.show()
