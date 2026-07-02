from pathlib import Path

import pandas  # noqa
import numpy
import trimesh
import matplotlib  # noqa
from matplotlib import pyplot  # noqa
import spiceypy as spice
import scipy  # noqa

import kalast
from kalast.util import DPR, RPD, SPEED_LIGHT, JANSKY  # noqa
from kalast.spice_entities import earth, moon


spice.kclear()
spice.furnsh("/Users/gregoireh/data/spice/wgc/mk/solar_system_v0060.tm")

obs = earth
bod = moon

mesh = trimesh.load("moon.obj")
sph = numpy.array([kalast.util.cart2sph(v) for v in mesh.triangles_center])

path = Path("out")
et_dates = numpy.load(path / "et_dates.npy")
pe = numpy.load(path / "pe.npy")
de = numpy.load(path / "de.npy")
ps = numpy.load(path / "ps.npy")
ds = numpy.load(path / "ds.npy")
ster = numpy.load(path / "ster.npy")

et_simu = numpy.load(path / "et_simu.npy")
tmp_surf = numpy.load(path / "tmp_surf.npy")

n = 1

wl = numpy.linspace(0.9, 29, num=1000, endpoint=True) * 1e-6
nw = wl.size

ii_simu_all = numpy.zeros(n)
spec_rad = numpy.zeros((n, nw))
spec_irrad = numpy.zeros((n, nw))

for it, et_ in enumerate(et_dates[:n]):
    ii_simu = numpy.argmin(abs(et_simu - et_))
    ii_simu_all[it] = ii_simu

    for iif in range(0, mesh.faces.shape[0]):
        p_ = mesh.triangles_center[iif]
        n_ = mesh.face_normals[iif]
        area = mesh.area_faces[iif] * 1e6

        v_sc = pe[it] - p_
        d_sc = numpy.linalg.norm(v_sc)
        u_sc = v_sc / d_sc
        cose = kalast.astro.cosinc(u_sc, n_)

        ster_ = area / (d_sc * d_sc)

        spec_rad_ = kalast.tpm.core.planck(tmp_surf[ii_simu, iif], wl) * 0.95 * cose
        spec_irrad_ = spec_rad_ * ster_

        spec_rad[it] += spec_rad_
        spec_irrad[it] += spec_irrad_

# spec_rad_jy = spec_rad * wl**2 / SPEED_LIGHT * JANSKY
# spec_irrad_jy = spec_irrad * wl**2 / SPEED_LIGHT * JANSKY

path = Path("out")
numpy.save(path / "ii_simu_all.npy", ii_simu_all)
numpy.save(path / "wl.npy", wl)
numpy.save(path / "spec_rad.npy", spec_rad)
numpy.save(path / "spec_irrad.npy", spec_irrad)
