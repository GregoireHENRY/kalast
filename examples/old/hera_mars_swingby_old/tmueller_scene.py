#!/usr/bin/env python

# from IPython import embed as bp
from pathlib import Path  # noqa

import pandas
import numpy
import spiceypy as spice

import kalast  # noqa
from kalast.util import DPR, RPD, AU  # noqa
from kalast.entity import HERA, TIRI, EARTH, MOON, DEIMOS  # noqa

spice.kclear()

# spice.furnsh(r"D:\data\spice\hera\kernels\mk\hera_ops.tm")
spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm")


frame = "j2000"

# df = pandas.read_csv(r"D:\data\hera\tiri\tiri_images_mars_swing-by.csv")
df = pandas.read_csv("/Users/gregoireh/data/hera/tiri/tiri_images_mars_swing-by.csv")

images = df["image"].to_numpy()
et = df["et"].to_numpy()
filters = df["filter"].to_numpy()
n = len(images)

pos_au_sun_from_hera = numpy.zeros((n, 3))

pos_au_hera_from_sun = numpy.zeros((n, 3))
dist_au_hera_sun = numpy.zeros(n)

pos_au_deimos_from_sun = numpy.zeros((n, 3))
dist_au_deimos_sun = numpy.zeros(n)

pos_km_deimos_from_hera = numpy.zeros((n, 3))
dist_km_deimos_hera = numpy.zeros(n)

sr_deimos_from_hera = numpy.zeros(n)
lolapha_deimos_sub_hera = numpy.zeros((n, 3))

for it, _ in enumerate(images):
    (p, _lt) = spice.spkpos("sun", et[it], TIRI.frame, "none", TIRI.name)
    pos_au_sun_from_hera[it] = p * 1e3 / AU

    (p, _lt) = spice.spkpos(TIRI.name, et[it], "eclipj2000", "none", "sun")
    pos_au_hera_from_sun[it] = p * 1e3 / AU
    dist_au_hera_sun[it] = numpy.linalg.norm(pos_au_hera_from_sun[it])

    (p, _lt) = spice.spkpos(DEIMOS.name, et[it], "eclipj2000", "none", "sun")
    pos_au_deimos_from_sun[it] = p * 1e3 / AU
    dist_au_deimos_sun[it] = numpy.linalg.norm(pos_au_deimos_from_sun[it])

    (p, _lt) = spice.spkpos(DEIMOS.name, et[it], TIRI.frame, "none", TIRI.name)
    pos_km_deimos_from_hera[it] = p
    dist_km_deimos_hera[it] = numpy.linalg.norm(pos_km_deimos_from_hera[it])

    area = numpy.pi * (DEIMOS.radius() * 1e3) ** 2
    sr_deimos_from_hera[it] = area / dist_km_deimos_hera[it] ** 2

    sp_, h_, lo, la, pha = kalast.spice.subobs(TIRI, DEIMOS, et[it])
    lolapha_deimos_sub_hera[it] = [lo * DPR, la * DPR, pha * DPR]

df = {}
df["image"] = images
df["et"] = et
df["filter"] = filters
df["x0[au]"] = pos_au_sun_from_hera[:, 0]
df["y0[au]"] = pos_au_sun_from_hera[:, 1]
df["z0[au]"] = pos_au_sun_from_hera[:, 2]
df["x1[au]"] = pos_au_hera_from_sun[:, 0]
df["y1[au]"] = pos_au_hera_from_sun[:, 1]
df["z1[au]"] = pos_au_hera_from_sun[:, 2]
df["d1[au]"] = dist_au_hera_sun
df["x2[au]"] = pos_au_deimos_from_sun[:, 0]
df["y2[au]"] = pos_au_deimos_from_sun[:, 1]
df["z2[au]"] = pos_au_deimos_from_sun[:, 2]
df["d2[au]"] = dist_au_deimos_sun
df["x3[km]"] = pos_km_deimos_from_hera[:, 0]
df["y3[km]"] = pos_km_deimos_from_hera[:, 1]
df["z3[km]"] = pos_km_deimos_from_hera[:, 2]
df["d3[km]"] = dist_km_deimos_hera
df["sr"] = sr_deimos_from_hera
df["lo[°]"] = lolapha_deimos_sub_hera[:, 0]
df["la[°]"] = lolapha_deimos_sub_hera[:, 1]
df["pha[°]"] = lolapha_deimos_sub_hera[:, 2]
df = pandas.DataFrame(df)
# df.to_csv("geometry.csv", index=False, encoding="utf-8-sig")


# lt = 828.1663639006587
#
# >>> spice.spkpos(HERA.name, et[0], "eclipj2000", "none", "sun")[0]
# -1.91916089e+08,  1.57309475e+08,  7.99524645e+06
#
# >>> spice.spkpos(HERA.name, et[0], "eclipj2000", "lt", "sun")[0]
# -1.91898927e+08,  1.57318067e+08,  7.99540102e+06
#
# >>> -spice.spkpos("sun", et[0], "eclipj2000", "none", HERA.name)[0]
# -1.91916089e+08,  1.57309475e+08,  7.99524645e+06
#
# >>> -spice.spkpos("sun", et[0], "eclipj2000", "lt", HERA.name)[0]
# -1.91916078e+08,  1.57309471e+08,  7.99524627e+06
#
# >>> -spice.spkpos("sun", et[0]-lt, "eclipj2000", "none", HERA.name)[0]
# -1.91898916e+08,  1.57318063e+08,  7.99540084e+06
#
# >>> -spice.spkpos("sun", et[0]+lt, "eclipj2000", "none", HERA.name)[0]
# -1.91933283e+08,  1.57300907e+08,  7.99510119e+06