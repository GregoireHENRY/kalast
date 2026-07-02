#!/usr/bin/env python

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
from kalast.spice_entities import hera, tiri, earth, moon, deimos  # noqa


spice.kclear()
spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm")

obs = hera
cam = tiri
bod = deimos

mesh = trimesh.load("work/deimos.obj")
sph = numpy.array([kalast.util.cart2sph(v) for v in mesh.triangles_center])
nf = mesh.faces.shape[0]

images = numpy.load("work/scene/images.npy", allow_pickle=True)
et_images = numpy.load("work/scene/et_images.npy")
filters = numpy.load("work/scene/filters.npy", allow_pickle=True)
po = numpy.load("work/scene/po.npy")
do = numpy.load("work/scene/do.npy")
ps = numpy.load("work/scene/ps.npy")
ds = numpy.load("work/scene/ds.npy")
fwpos = numpy.load("work/scene/fwpos.npy")
resp = numpy.load("work/scene/resp.npy")
wlu = numpy.load("work/scene/wlu.npy")
et_simu = numpy.load("work/tpm/et_simu.npy")
tmp_surf = numpy.load("work/tpm/tmp_surf.npy")

# R = numpy.load("work/rad/R.npy")
R = numpy.ones_like(tmp_surf)

n = len(images)

wl = wlu * 1e-6
nw = wl.size

ii_simu = numpy.zeros(n, dtype=int)
ster_all = numpy.zeros((n, nf))
inc_all = numpy.zeros((n, nf))
emi_all = numpy.zeros((n, nf))
ppha_all = numpy.zeros((n, nf))
tmp_all = numpy.zeros((n, nf))
spec_rad = numpy.zeros((n, nw))
spec_irrad = numpy.zeros((n, nw))
rad = numpy.zeros(n)
irrad = numpy.zeros(n)
rad_all = numpy.zeros((n, nf))
irrad_all = numpy.zeros((n, nf))

for it, image in enumerate(images):
    ii_simu[it] = numpy.argmin(abs(et_simu - et_images[it]))

    for iif in range(0, nf):
        p_ = mesh.triangles_center[iif]
        n_ = mesh.face_normals[iif]
        area = mesh.area_faces[iif] * 1e6

        v_sun = ps[it] - p_
        d_sun = numpy.linalg.norm(v_sun)
        dau_sun = d_sun / AU
        u_sun = v_sun / d_sun
        cosi = kalast.astro.cosinc(u_sun, n_)

        v_sc = po[it] - p_
        d_sc = numpy.linalg.norm(v_sc)
        u_sc = v_sc / d_sc
        cose = kalast.astro.cosinc(u_sc, n_)

        u_sun_proj = ps[it] - glm.dot(u_sun, n_) * n_ - p_
        u_sun_proj = u_sun_proj / glm.length(u_sun_proj)
        u_sc_proj = po[it] - glm.dot(u_sc, n_) * n_ - p_
        u_sc_proj = u_sc_proj / glm.length(u_sc_proj)
        cos_ppha = kalast.astro.cosinc(u_sun_proj, u_sc_proj)

        ster_ = area / (d_sc * d_sc)

        tmp_ = tmp_surf[ii_simu[it], iif]
        spec_rad_ = kalast.tpm.core.planck(tmp_, wl) * 0.95 * cose * R[it, iif]

        rad_ = scipy.integrate.simpson(spec_rad_ * resp[:, fwpos[it] - 1], wl)

        spec_rad[it] += spec_rad_
        spec_irrad[it] += spec_rad_ * ster_

        rad[it] += rad_
        irrad[it] += rad_ * ster_

        rad_all[it, iif] = rad_
        irrad_all[it, iif] = rad_ * ster_

        ster_all[it, iif] = ster_
        inc_all[it, iif] = numpy.acos(cosi)
        emi_all[it, iif] = numpy.acos(cose)
        ppha_all[it, iif] = numpy.acos(cos_ppha)
        tmp_all[it, iif] = tmp_

numpy.save("ii_simu.npy", ii_simu)
numpy.save("ster_all.npy", ster_all)
numpy.save("inc_all.npy", inc_all)
numpy.save("emi_all.npy", emi_all)
numpy.save("ppha_all.npy", ppha_all)
numpy.save("tmp_all.npy", tmp_all)
numpy.save("spec_rad.npy", spec_rad)
numpy.save("spec_irrad.npy", spec_irrad)
numpy.save("rad.npy", rad)
numpy.save("irrad.npy", irrad)
numpy.save("rad_all.npy", rad_all)
numpy.save("irrad_all.npy", irrad_all)
