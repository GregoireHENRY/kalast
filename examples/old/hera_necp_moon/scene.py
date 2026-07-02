#!/usr/bin/env python

from pathlib import Path  # noqa

import trimesh
import pandas
import numpy
import scipy
import spiceypy as spice

import kalast  # noqa
from kalast.util import DPR, RPD  # noqa
from kalast.spice_entities import hera, tiri, earth, moon  # noqa


FW_POS = [
    "CLOSE",
    "Filter a (7.8um)",
    "Filter b (8.6um)",
    "Filter c (9.6um)",
    "Filter d (10.6um)",
    "Filter e (11.6um)",
    "Filter f (13.0um)",
    "Filter g (wide)",
]

# Load spice
spice.kclear()
spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm")
frame = "j2000"

obs = hera
cam = tiri
bod = moon

df = pandas.read_excel("in/summary.xlsx")
images = df["image"].to_numpy()
et = df["et"].to_numpy()
filters = df["filter"].to_numpy()

ii_close = [ii for ii, filter_ in enumerate(filters) if filter_ == "CLOSE"]
ii_open = [ii for ii, filter_ in enumerate(filters) if filter_ != "CLOSE"]
ii_open_wide = [ii for ii, jj in enumerate(ii_open) if filters[jj] == "Filter g (wide)"]
fwpos = [FW_POS.index(filters[ii]) for ii in ii_open]
n = len(ii_open)

images = images[ii_open]
et = et[ii_open]
filters = filters[ii_open]

resp = numpy.genfromtxt("/Users/gregoireh/data/hera/tiri/response.csv", delimiter=",")
wlu = resp[:1201, 0]
resp = resp[:1201, 2:9]
# 3-14.99 mu

rh = numpy.genfromtxt("in/Moon_spectral_reflectivity.csv", delimiter=",")
wlu2 = rh[:, 0]
f = scipy.interpolate.interp1d(rh[:, 0], rh[:, 1], fill_value="extrapolate")
rh = f(wlu)
emi = 1 - rh

mesh = trimesh.load("in/moon.obj")
sph = numpy.array([kalast.util.cart2sph(v) for v in mesh.triangles_center])
nf = mesh.faces.shape[0]
R = numpy.ones((n, nf))

po = numpy.zeros((n, 3))
do = numpy.zeros(n)
ps = numpy.zeros((n, 3))
ds = numpy.zeros(n)
pha = numpy.zeros(n)
ster = numpy.zeros(n)
lola = numpy.zeros((n, 2))

for it, _ in enumerate(images):
    (p_, _lt) = spice.spkpos(obs.name, et[it], bod.frame, "none", bod.name)
    po[it] = p_ * 1e3
    do[it] = numpy.linalg.norm(po[it])

    (p_, _lt) = spice.spkpos("sun", et[it], bod.frame, "none", bod.name)
    ps[it] = p_ * 1e3
    ds[it] = numpy.linalg.norm(ps[it])

    area = numpy.pi * (bod.radius * 1e3) ** 2
    ster[it] = area / do[it] ** 2

    sp_, h_, lo, la, pha[it] = kalast.spice.subobs(obs, bod, et[it])
    lola[it] = [lo, la]


numpy.save("images.npy", images)
numpy.save("et_images.npy", et)
numpy.save("filters.npy", filters)
numpy.save("fwpos.npy", fwpos)
numpy.save("ii_close.npy", ii_close)
numpy.save("ii_open.npy", ii_open)
numpy.save("ii_open_wide.npy", ii_open_wide)
numpy.save("po.npy", po)
numpy.save("do.npy", do)
numpy.save("ps.npy", ps)
numpy.save("ds.npy", ds)
numpy.save("pha.npy", pha)
numpy.save("ster.npy", ster)
numpy.save("lola.npy", lola)
numpy.save("wlu.npy", wlu)
numpy.save("resp.npy", resp)
numpy.save("spec_emi.npy", emi)
numpy.save("R.npy", R)
