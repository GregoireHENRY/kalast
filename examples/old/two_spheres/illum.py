#!/usr/bin/env python

from pathlib import Path

import numpy
import glm
import matplotlib
from matplotlib import pyplot

import kalast


# rust is column-major
# kalast is coded with column vector and matrix multiplication from the left such as: v2 = m @ v
#
# numpy is row-major like C but can add `order="F"` to arrays for column-major


# Params

a = 0.1

# The simulation is centered on 1st body
# A 2nd body orbit around it
# The sun is fixed at 1 AU on X axis
# The two bodies are spinning
# We are in a non-inertial fixed frame similar to ecliptic frame (called global frame)

orbit_period = 6.0 * 3600.0
orbit_axis = numpy.array([0.0, 0.0, 1.0])
spin_periods = [2.0 * 3600.0, 6.0 * 3600.0]
spin_axes = [
    numpy.array([0.0, 0.0, 1.0]),
    numpy.array([0.0, 0.0, 1.0]),
]

# Position center of the two bodies
ps = [
    numpy.array([0.0, 0.0, 0.0]),
    numpy.array([5000.0, 0.0, 0.0]),
]

t0 = 0.0 * 3600.0
dt = 120.0
tf = 1.0 * orbit_period

# dt = 2.0 * 3600.0
# tf = 240.0
# dt = 1350.0
# tf = 5400.0

sun = numpy.array([1.0, 0.0, 0.0]) * kalast.util.AU
# sun = numpy.array([3000.0, 0.0, 0.0])

# Computed automatically

t = numpy.arange(0, tf + dt, dt)
nt = t.size
# nt = int(numpy.ceil(tf / dt)) + 1
# t = numpy.linspace(0, tf, nt, endpoint=True)

m_orbit = kalast.util.mat_axis_angle(orbit_axis, 2.0 * numpy.pi * dt / orbit_period)
m_spins = [
    kalast.util.mat_axis_angle(spin_axes[iib], 2.0 * numpy.pi * dt / spin_periods[iib])
    for iib in range(0, 2)
]


m_orbit_init = kalast.util.mat_axis_angle(
    orbit_axis, 2.0 * numpy.pi * t0 / orbit_period
)
ps[1] = m_orbit_init @ ps[1]

m_spins_init = [
    kalast.util.mat_axis_angle(spin_axes[iib], 2.0 * numpy.pi * t0 / spin_periods[iib])
    for iib in range(0, 2)
]

# m_states = [numpy.eye(3) for iib_ in range(0, 2)]
m_states = m_spins_init.copy()

# Will be computed in loop
m_states_inv = []

# Load meshes

nfs = []
meshes = []
meshes_data = []
paths = ["/Users/gregoireh/data/mesh/ico1.obj", "/Users/gregoireh/data/mesh/ico1.obj"]
extents = [1000.0, 1000.0]
for p, ext in zip(paths, extents):
    mesh = kalast.mesh.Mesh(p, update_pos=lambda x: x * ext)
    nf = len(mesh.facets)
    nfs.append(nf)
    meshes.append(mesh)
    data = {}
    data["fp"] = numpy.array([f.p for f in mesh.facets])
    data["fn"] = numpy.array([f.n for f in mesh.facets])
    data["xyz"] = numpy.array([v.pos for v in mesh.vertices])
    data["tri"] = numpy.array([mesh.get_indices_facet(iif) for iif in range(0, nf)])
    data["inc"] = numpy.zeros(nf)
    data["shdw"] = numpy.zeros(nf)
    data["fx"] = numpy.zeros(nf)
    data["fxmrf"] = numpy.zeros(nf)
    data["fxmre"] = numpy.zeros(nf)
    meshes_data.append(data)

# Time loop
# Computation illumination angles

vfs = numpy.zeros(nfs)
vfs2 = numpy.zeros(nfs)

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


fig = pyplot.figure()
fig.subplots_adjust(left=0.0, bottom=0.0, right=1.0, top=1.0, wspace=0.0, hspace=0.0)
ax = fig.add_subplot(projection="3d")

cmap = matplotlib.cm.cividis_r  # .resampled(100)
norm = matplotlib.colors.Normalize(vmin=0.0, vmax=90.0)
mappable_ang = matplotlib.cm.ScalarMappable(cmap=cmap, norm=norm)

cmap = matplotlib.cm.Greys
norm = matplotlib.colors.Normalize(vmin=0.0, vmax=1.0)
mappable_bw = matplotlib.cm.ScalarMappable(cmap=cmap, norm=norm)

cmap = matplotlib.cm.cividis
norm = matplotlib.colors.Normalize(vmin=0.0, vmax=1.0)
mappable_norm = matplotlib.cm.ScalarMappable(cmap=cmap, norm=norm)

cmap = matplotlib.cm.inferno
norm = matplotlib.colors.Normalize(vmin=0.0, vmax=1000.0)
mappable_sf = matplotlib.cm.ScalarMappable(cmap=cmap, norm=norm)

cmap = matplotlib.cm.inferno
norm = matplotlib.colors.Normalize(vmin=0.0, vmax=1.0)
mappable_norm_inferno = matplotlib.cm.ScalarMappable(cmap=cmap, norm=norm)

cmap = matplotlib.cm.inferno
norm = matplotlib.colors.Normalize(vmin=0.0, vmax=5.0)
mappable_inferno_mutual = matplotlib.cm.ScalarMappable(cmap=cmap, norm=norm)

lim = 5000
ax.set_xlim(-lim, lim)
ax.set_ylim(-lim, lim)
ax.set_zlim(-lim, lim)
# ax.set_box_aspect([1, 1, 1])
ax.set_aspect("equal")
ax.set_xlabel("x")
ax.set_ylabel("y")
ax.set_zlabel("z")
ax.set_proj_type("persp", focal_length=0.4)  # 136°
# ax.set_axis_off()

surfs = []

for iit in range(0, nt):
    # if True:
    if iit > 0:
        # Position of 2nd body is updated to orbit the 1st body
        ps[1] = m_orbit @ ps[1]

        # Update the rotation/state matrix of the two bodies for the spin
        m_states = [m_spins[iib] @ m_states[iib] for iib in range(0, 2)]

    m_states_inv = [numpy.linalg.inv(m) for m in m_states]

    # Transformation model matrix from 2nd body to 1st
    # usually matrix model is applied as T @ R @ v
    # but here translation is before R so we have R @ T @ v
    # can still add T2 @ R @ T @ v if needed
    rot_b2g = numpy.eye(4)
    rot_g2a = numpy.eye(4)
    trans_b2a_before_rot = numpy.eye(4)
    rot_b2g[:3, :3] = m_states[1]
    rot_g2a[:3, :3] = m_states_inv[0]
    trans_b2a_before_rot[:3, 3] = ps[1] - ps[0]
    trans_b2a = rot_g2a @ trans_b2a_before_rot @ rot_b2g

    text = ""
    # text = f": {sun / glm.length(sun) * 1000.0}"

    for iib in range(0, 2):
        # iib is current body
        # iib2 is the other body
        iib2 = (iib + 1) % 2

        meshes_data[iib]["inc"][:] = 0.0
        meshes_data[iib]["shdw"][:] = 0.0
        meshes_data[iib]["fx"][:] = 0.0
        meshes_data[iib]["fxmrf"][:] = 0.0
        meshes_data[iib]["fxmre"][:] = 0.0

        # We compute eclipse on current body casted by other body.
        # Intersect routine is called on other mesh.
        # We iterate over every facets of current mesh and calculate ray from sun to these facets and check
        # if any of these rays is intercepted by other mesh.
        # So we need position of current body facets in frame of other mesh.

        # Facet positions of current body in global frame
        fps = numpy.matvec(m_states[iib], meshes_data[iib]["fp"]) + ps[iib]
        fns = numpy.matvec(m_states[iib], meshes_data[iib]["fn"])

        # Facet positions of current body in other body fixed frame
        fps2 = numpy.matvec(m_states_inv[iib2], (fps - ps[iib2]))
        fns2 = numpy.matvec(m_states_inv[iib2], fns)

        # Position of sun in bodies fixed frame
        # sun1 = m_states_inv[iib] @ (sun - ps[iib])
        sun2 = m_states_inv[iib2] @ (sun - ps[iib2])

        for iif in range(0, nfs[iib]):
            # Calculations for incidence angle
            p = fps[iif]
            n = fns[iif]
            v_sun = sun - p
            d_sun = glm.length(v_sun)
            dau_sun = d_sun / kalast.util.AU
            u_sun = v_sun / d_sun
            cos_inc = kalast.astro.cosine_incidence(u_sun, n)
            inc_ = numpy.acos(cos_inc)
            meshes_data[iib]["inc"][iif] = inc_

            # Solar flux
            meshes_data[iib]["fx"][iif] = kalast.tpm.core.radiation_sun(
                dau_sun, cos_inc, a
            )

            # Below are calculations for mutual shadowing

            # If other body is not between facet and sun
            if d_sun < glm.length(sun - ps[iib2]):
                continue

            p = fps2[iif]
            n = fns2[iif]
            v_sun = sun2 - p
            d_sun = glm.length(v_sun)
            u_sun = v_sun / d_sun

            # If ray is parallel to facet normal, no interception test
            det = -u_sun @ n
            if det > -THRESHOLD_PARALLEl and det < THRESHOLD_PARALLEl:
                continue

            # If ray is not facing facet normal, no interception test
            if det >= 0.0:
                continue

            r = meshes[iib2].intersect(sun2, -u_sun)

            # If ray is intercepted by another facet, mark it shadowed
            #
            # Careful, this:
            # r[0] != iif
            #
            # is a condition for self shadowing only!
            #
            # if r is not None and r[0] != iif:

            if r is not None:
                meshes_data[iib]["shdw"][iif] = 1.0

    # View factor between two bodies
    # TODO: compute ray interception to check visibility between facets
    # also, add areas when needed for computation mutual heating
    vfs[:] = 0.0
    for iif in range(0, nfs[0]):
        for iif2 in range(0, nfs[1]):
            vfs[iif, iif2] = kalast.mesh.view_factor_facets(
                meshes[0].facets[iif], meshes[1].facets[iif2], trans_b2a
            )

    # Create view factor with areas of facets of body 1 or 2.
    # vfs2 = vfs.copy().T
    # for iif2 in range(0, nfs[1]):
    #     vfs2[iif2, :] *= meshes[1].facets[iif2].a
    # for iif in range(0, nfs[0]):
    #     vfs2[:, iif] *= meshes[0].facets[iif].a

    # Mutual fluxes
    for iib in range(0, 2):
        iib2 = (iib + 1) % 2

        for iif in range(0, nfs[iib]):
            for iif2 in range(0, nfs[iib2]):
                if meshes_data[iib2]["shdw"][iif2] == 1.0:
                    continue
                if iib == 0:
                    vf_ = vfs[iif, iif2]
                else:
                    vf_ = vfs[iif2, iif]

                # Care using the `reuse` function, as albedo needs to be correct
                meshes_data[iib]["fxmrf"][iif] += (
                    kalast.tpm.core.radiation_sun_reflected_reuse(
                        vf_, meshes_data[iib2]["fx"][iif2], a
                    )
                    * meshes[iib2].facets[iif2].a
                )

    for iib in range(0, len(surfs)):
        surfs[iib].remove()
    surfs.clear()

    for iib in range(0, 2):
        xyz = numpy.matvec(m_states[iib], meshes_data[iib]["xyz"]) + ps[iib]

        surfs.append(
            ax.plot_trisurf(
                xyz[:, 0],
                xyz[:, 1],
                xyz[:, 2],
                triangles=meshes_data[iib]["tri"],
                lw=0.5,
                ec="#101010",
            )
        )

        # data = inc[iib, :] * kalast.util.DPR
        # idx_shadow = numpy.where(shadowed[iib, :] == 1.0)[0]
        # data[idx_shadow] = 90.0
        # colors = mappable_ang.to_rgba(data)  # 0

        # data = shadowed[iib, :]
        # colors = mappable_bw.to_rgba(data)  # 0

        # data = meshes_data[iib]["fx"]
        # idx_shadow = numpy.where(meshes_data[iib]["shdw"] == 1.0)[0]
        # data[idx_shadow] = 0.0
        # colors = mappable_sf.to_rgba(data)  # 0

        # iif = 76  # ico1 front
        # iif = 17  # ico1 back
        # iif = 230  # ico2 front
        # iif = 70  # ico2 back
        # iif = 922 # ico3 front
        # iif = 282  # ico3 back

        # iif = 76
        # if iib == 0:
        #     colors = numpy.tile([0.7, 0.7, 0.7, 1.0], (nfs[0], 1))
        #     colors[iif, :] = [1.0, 0.0, 0.0, 1.0]
        # else:
        #     data = vfs[iif, :]
        #     colors = mappable_norm.to_rgba(data / vfs.max())  # 0
        #     idx0 = numpy.where(data == 0.0)[0]
        #     colors[idx0] = [0.7, 0.7, 0.7, 1.0]
        #     # colors[17, :] = [0.0, 1.0, 0.0, 1.0]

        # iif = 17
        # if iib == 0:
        #     data = vfs[:, iif]
        #     colors = mappable_norm.to_rgba(data / vfs.max())  # 0
        #     idx0 = numpy.where(data == 0.0)[0]
        #     colors[idx0] = [0.7, 0.7, 0.7, 1.0]
        #     # colors[17, :] = [0.0, 1.0, 0.0, 1.0]
        # else:
        #     colors = numpy.tile([0.7, 0.7, 0.7, 1.0], (nfs[1], 1))
        #     colors[iif, :] = [1.0, 0.0, 0.0, 1.0]

        if iib == 0:
            colors = numpy.tile([0.7, 0.7, 0.7, 1.0], (nfs[0], 1))
        else:
            data = meshes_data[iib]["fxmrf"]
            colors = mappable_inferno_mutual.to_rgba(data)  # 0
            idx0 = numpy.where(data == 0.0)[0]
            colors[idx0] = [0.7, 0.7, 0.7, 1.0]

        # if iib == 0:
        #     data = meshes_data[iib]["fxmrf"]
        #     colors = mappable_inferno_mutual.to_rgba(data)  # 0
        #     idx0 = numpy.where(data == 0.0)[0]
        #     colors[idx0] = [0.7, 0.7, 0.7, 1.0]
        # else:
        #     colors = numpy.tile([0.7, 0.7, 0.7, 1.0], (nfs[1], 1))

        surfs[iib].set_fc(colors)

    vid = Path("out/vid")
    vid1 = vid / "1"
    vid2 = vid / "2"
    vid1.mkdir(parents=True, exist_ok=True)
    vid2.mkdir(parents=True, exist_ok=True)
    ax.view_init(elev=30.0, azim=-60.0)
    fig.savefig(vid1 / f"{iit}.png", bbox_inches="tight", dpi=400)
    ax.view_init(elev=75.0, azim=-70.0)
    fig.savefig(vid2 / f"{iit}.png", bbox_inches="tight", dpi=400)

    # Show progress
    progress = iit / (nt - 1) * 100
    if ndigits > 0:
        progress = numpy.floor(progress * digit) / digit
    if progress >= last_freq_reached + freqv:
        last_freq_reached += freqv
        print(
            f"{progress:{digits_full}.{digits_decimal}f}% ({iit:,}/{nt - 1:,}it){text}"
        )

    break

pyplot.show()
