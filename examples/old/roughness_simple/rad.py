#!/usr/bin/env python

from pathlib import Path  # noqa

import glm
import pandas  # noqa
import numpy
import trimesh
import matplotlib  # noqa
from matplotlib import pyplot  # noqa
import scipy  # noqa

import kalast
from kalast.util import DPR, RPD, SPEED_LIGHT, JANSKY, AU  # noqa


mesh = trimesh.load("work/mesh.obj")
sph = numpy.array([kalast.util.cart2sph(v) for v in mesh.triangles_center])
nf = mesh.faces.shape[0]

et = numpy.load("work/et_simu.npy")
sun = numpy.load("work/sun_simu.npy")
tmp_surf = numpy.load("work/tmp_surf.npy")

it_et = 0
et = et[it_et]
sun = sun[it_et, :]
tmp_surf = tmp_surf[it_et, :]

sun_dir = sun / numpy.linalg.norm(sun)
obs_dist = 100e3

# R = numpy.load("work/rad/R.npy")
# R = numpy.ones_like(tmp_surf)
df = scipy.io.loadmat("work/R_6d.mat")
R = df["R"]
# numpy.save("R.npy", df["R"])

df_setup = scipy.io.loadmat("work/setup_6d.mat")
setup_keys = [
    "sun_all",
    "det_all",
    "psi_all",
    "gamma_all",
    "craterdens_all",
    "lambda_all",
]
# (19, 19, 19, 9, 5, 15)

setup_craterdens = df_setup["craterdens_all"][0]

angorb = numpy.linspace(0, 360, num=37, endpoint=True)
n1 = angorb.size
n2 = df_setup["craterdens_all"].size

ster_all = numpy.zeros((n1, nf))
inc_all = numpy.zeros((n1, nf))
emi_all = numpy.zeros((n1, nf))
ppha_all = numpy.zeros((n1, nf))
tmp_all = numpy.zeros((n1, nf))
R_all = numpy.zeros((n1, n2, nf))
spec_rad_all = numpy.zeros((n1, n2, nf))
spec_irrad_all = numpy.zeros((n1, n2, nf))
spec_rad = numpy.zeros((n1, n2))
spec_irrad = numpy.zeros((n1, n2))

it = 0
resp = 1.0
emi = 0.9

wl = 11.0e-6
iir_lambda = 6
iir_gamma = 8

for ang in angorb:
    mat = kalast.util.mataxisang(ang * RPD, [0, 0, 1])
    mat = numpy.array(mat)[:3, :3]

    obs = sun_dir @ mat * obs_dist

    # calculate pha for ii=0 between obs body sun
    u1 = sun / numpy.linalg.norm(sun)
    u2 = obs / numpy.linalg.norm(obs)
    pha = kalast.util.angvec(u1, u2)
    print(f"pha={pha * DPR:.1f}°")

    for iif in range(0, nf):
        p_ = mesh.triangles_center[iif]
        n_ = mesh.face_normals[iif]
        area = mesh.area_faces[iif] * 1e6

        v_sun = sun - p_
        d_sun = numpy.linalg.norm(v_sun)
        dau_sun = d_sun / AU
        u_sun = v_sun / d_sun
        cosi = kalast.astro.cosinc(u_sun, n_)

        v_sc = obs - p_
        d_sc = numpy.linalg.norm(v_sc)
        u_sc = v_sc / d_sc
        cose = kalast.astro.cosinc(u_sc, n_)

        u_sun_proj = sun - glm.dot(u_sun, n_) * n_ - p_
        u_sun_proj = u_sun_proj / glm.length(u_sun_proj)
        u_sc_proj = obs - glm.dot(u_sc, n_) * n_ - p_
        u_sc_proj = u_sc_proj / glm.length(u_sc_proj)
        cos_ppha = kalast.astro.cosinc(u_sun_proj, u_sc_proj)

        ster_ = area / (d_sc * d_sc)

        tmp_ = tmp_surf[iif]

        iir_sun = numpy.argmin(numpy.abs(df_setup["sun_all"] - numpy.acos(cosi)))
        iir_emi = numpy.argmin(numpy.abs(df_setup["det_all"] - numpy.acos(cose)))
        iir_ppha = numpy.argmin(numpy.abs(df_setup["psi_all"] - numpy.acos(cos_ppha)))

        for iir_craterdens, craterdens in enumerate(df_setup["craterdens_all"][0]):
            R_ = R[iir_sun, iir_emi, iir_ppha, iir_gamma, iir_craterdens, iir_lambda]
            spec_rad_ = kalast.tpm.core.planck(tmp_, wl) * emi * cose * R_
            spec_irrad_ = spec_rad_ * ster_

            spec_rad[it, iir_craterdens] += spec_rad_
            spec_irrad[it, iir_craterdens] += spec_irrad_
            spec_rad_all[it, iir_craterdens, iif] = spec_rad_
            spec_irrad_all[it, iir_craterdens, iif] = spec_irrad_
            R_all[it, iir_craterdens, iif] = R_

        ster_all[it, iif] = ster_
        inc_all[it, iif] = numpy.acos(cosi)
        emi_all[it, iif] = numpy.acos(cose)
        ppha_all[it, iif] = numpy.acos(cos_ppha)
        tmp_all[it, iif] = tmp_

    it += 1

numpy.save("ster_all.npy", ster_all)
numpy.save("inc_all.npy", inc_all)
numpy.save("emi_all.npy", emi_all)
numpy.save("ppha_all.npy", ppha_all)
numpy.save("tmp_all.npy", tmp_all)
numpy.save("R_all.npy", R_all)
numpy.save("spec_rad_all.npy", spec_rad_all)
numpy.save("spec_irrad_all.npy", spec_irrad_all)
numpy.save("spec_rad.npy", spec_rad)
numpy.save("spec_irrad.npy", spec_irrad)

# spec_irrad_jy = spec_irrad * wl**2 / SPEED_LIGHT * JANSKY
