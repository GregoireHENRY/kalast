#!/usr/bin/env python

import math  # noqa
from pathlib import Path  # noqa

import glm
import pandas  # noqa
import numpy
import trimesh
import matplotlib  # noqa
from matplotlib import pyplot  # noqa
import spiceypy as spice
import scipy  # noqa

import kalast
from kalast.util import DPR, RPD, SPEED_LIGHT, JANSKY, AU  # noqa
from kalast.spice_entities import earth  # noqa

# do rotation average of both bodies separately, and sum them

data_x = numpy.array(
    [
        4.732777595187587,
        8.706444769786966,
        10.640015138386765,
        12.390889081438546,
        17.757208162777495,
    ]
)

data_y = numpy.array(
    [
        0.07572309870550162,
        0.3812544245550161,
        0.43403304510517793,
        0.40045383292880254,
        0.22198624595469255,
    ]
)

data_y2 = numpy.array(
    [
        0.415023261,
        1.906477549,
        2.190976436,
        2.005619185,
        1.211569579,
    ]
)

spice.kclear()
spice.furnsh("/Users/gregoireh/data/spice/dart/mk/d520_v03.tm")

obs = earth

body = "didymos"
# body = "dimorphos"

fixed_frame = f"{body}_fixed"

i1 = 0
i2 = 271
i3 = 542
istep = 10
nii = i2 - i1

mesh = trimesh.load(f"work/{body}/mesh.obj")
sph = numpy.array([kalast.util.cart2sph(v) for v in mesh.triangles_center])
nf = mesh.faces.shape[0]

et_simu = numpy.load("work/et_simu.npy")
sun = numpy.load(f"work/{body}/sun.npy")
tmp_surf = numpy.load(f"work/{body}/tmp_surf.npy")
print(f"ntime={et_simu.size}")

wlu = numpy.linspace(4, 20, num=100, endpoint=True)
wl = wlu * 1e-6
nw = wl.size

e_ = 0.9

# df = scipy.io.loadmat(
#     "/Users/gregoireh/projects/kalast-utils/roughness-kuehrt/R_6d.mat"
# )
# R = df["R"]

df = scipy.io.loadmat(
    "/Users/gregoireh/projects/kalast-utils/roughness-kuehrt/R_8mu.mat"
)
R = df["R_8mu"]

# df_setup = scipy.io.loadmat(
#     "/Users/gregoireh/projects/kalast-utils/roughness-kuehrt/setup_6d.mat"
# )
# setup_keys = [
#     "sun_all",
#     "det_all",
#     "psi_all",
#     "gamma_all",
#     "craterdens_all",
#     "lambda_all",
# ]
# (19, 19, 19, 9, 5, 15)

df_setup = scipy.io.loadmat(
    "/Users/gregoireh/projects/kalast-utils/roughness-kuehrt/setup.mat"
)
setup_keys = [
    "sun_all",
    "det_all",
    "psi_all",
    "craterdens_all",
]

# iir_lambda = 6
# iir_gamma = 8
iir_craterdens = 42  # = 0.9 = 39.7°rms

spec_rad = numpy.zeros(nw)
spec_irrad = numpy.zeros(nw)

for ii in range(i1, i2):
    (p_obs, _lt) = spice.spkpos(obs.name, et_simu[ii], fixed_frame, "none", body)
    p_obs *= 1e3

    p_sun = sun[ii, :]

    for iif in range(0, nf):
        p_ = mesh.triangles_center[iif] * 1e3
        n_ = mesh.face_normals[iif]
        area = mesh.area_faces[iif] * 1e6

        v_sun = p_sun - p_
        d_sun = numpy.linalg.norm(v_sun)
        dau_sun = d_sun / AU
        u_sun = v_sun / d_sun
        cosi = kalast.astro.cosinc(u_sun, n_)

        v_obs = p_obs - p_
        d_obs = numpy.linalg.norm(v_obs)
        u_obs = v_obs / d_obs
        cose = kalast.astro.cosinc(u_obs, n_)

        u_sun_proj = p_sun - glm.dot(u_sun, n_) * n_ - p_
        u_sun_proj = u_sun_proj / glm.length(u_sun_proj)
        u_sc_proj = p_obs - glm.dot(u_obs, n_) * n_ - p_
        u_sc_proj = u_sc_proj / glm.length(u_sc_proj)
        cos_ppha = kalast.astro.cosinc(u_sun_proj, u_sc_proj)

        ster_ = area / (d_obs * d_obs)
        tmp_ = tmp_surf[ii, iif]

        iir_sun = numpy.argmin(numpy.abs(df_setup["sun_all"] - numpy.acos(cosi)))
        iir_emi = numpy.argmin(numpy.abs(df_setup["det_all"] - numpy.acos(cose)))
        iir_ppha = numpy.argmin(numpy.abs(df_setup["psi_all"] - numpy.acos(cos_ppha)))
        # R_ = R[iir_sun, iir_emi, iir_ppha, iir_gamma, iir_craterdens, iir_lambda]
        R_ = R[iir_sun, iir_emi, iir_ppha, iir_craterdens]
        # R_ = 1.0

        spec_rad_ = kalast.tpm.core.planck(tmp_, wl) * e_ * cose * R_

        # rad_ = scipy.integrate.simpson(spec_rad_ * resp[:, fwpos[it] - 1], wl)

        spec_rad += spec_rad_
        spec_irrad += spec_rad_ * ster_

spec_rad /= nii
spec_irrad /= nii

numpy.save("spec_rad.npy", spec_rad)
numpy.save("spec_irrad.npy", spec_irrad)

kalast.plot.style.load()

fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel(r"Wavelength ($\mu m$)")
ax.set_ylabel(r"Spectral Irradiance ($10^{-14}$ $Wm^{-2}\mu m^{-1}$)")

ax.plot(wlu, spec_irrad * 1e-6 * 1e14, lw=1, color="k", label="kalast")
ax.scatter(data_x, data_y, s=10, lw=1, marker="*", color="k", label="VLT")

ax.set_xlim(4, 20)
ax.set_ylim(0, 2.5)

pyplot.legend(frameon=False)
fig.savefig("spec_irrad.png", bbox_inches="tight", dpi=300)
pyplot.show()
