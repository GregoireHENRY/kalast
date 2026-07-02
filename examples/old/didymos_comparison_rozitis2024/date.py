#!/usr/bin/env python

from pathlib import Path  # noqa

import trimesh
import pandas  # noqa
import numpy
import spiceypy as spice

import kalast  # noqa
from kalast.util import DPR, RPD, AU  # noqa
from kalast.spice_entities import earth, didymos, dimorphos_pre as dimorphos  # noqa


# Load spice
spice.kclear()
spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm")

obs = earth
bods = [didymos, dimorphos]

date_obs = "2022-08-24 00:23"
# date_obs = "2022-09-26 05:07"

et = spice.str2et(date_obs)

meshes = [trimesh.load(p) for p in ["in/didymos.obj", "in/dimorphos.obj"]]
sphs = [
    numpy.array([kalast.util.cart2sph(v) for v in mesh.triangles_center])
    for mesh in meshes
]

bod = bods[0]

pha = spice.phaseq(et, bod.name, "sun", obs.name, "none")

(p_, _lt) = spice.spkpos(obs.name, et, bod.frame, "none", bod.name)
po = p_ * 1e3
do = numpy.linalg.norm(po)

(p_, _lt) = spice.spkpos(bod.name, et, "EARTH_FIXED", "none", obs.name)
pob = p_ * 1e3
dob = numpy.linalg.norm(pob)

r, lo, la = spice.reclat(pob)
az = -lo
if az < 0.0:
    az += 2 * numpy.pi
el = la
print(f"geocentric coords: {az * DPR}°, {el * DPR}°")

(p_, _lt) = spice.spkpos("sun", et, bod.frame, "none", bod.name)
ps = p_ * 1e3
ds = numpy.linalg.norm(ps)

(p_, _lt) = spice.spkpos(bod.name, et, "ECLIPJ2000", "none", "SUN")
pos = p_ * 1e3
dos = numpy.linalg.norm(pos)

r, lo, la = spice.reclat(pos)
# az = -lo
az = lo
if az < 0.0:
    az += 2 * numpy.pi
el = la
print(f"heliocentric coords: {az * DPR}°, {el * DPR}°")

area = numpy.pi * (bod.radius * 1e3) ** 2
ster = area / do**2

sp_, h_, lo, la, pha = kalast.spice.subobs(obs, bod, et)
lola = [lo, la]

df = {}
df["date"] = date_obs
df["et"] = et
df["do[AU]"] = do / AU
df["pob_x[km]"] = pob[0]
df["pob_y[km]"] = pob[1]
df["pob_z[km]"] = pob[2]
df["po_x[km]"] = po[0]
df["po_y[km]"] = po[1]
df["po_z[km]"] = po[2]
df["ds[AU]"] = ds / AU
df["pos_x[km]"] = pos[0]
df["pos_y[km]"] = pos[1]
df["pos_z[km]"] = pos[2]
df["ps_x[km]"] = ps[0]
df["ps_y[km]"] = ps[1]
df["ps_z[km]"] = ps[2]
df["ster"] = ster
df["lo[°]"] = lola[0] * DPR
df["la[°]"] = lola[1] * DPR
df["pha[°]"] = pha * DPR
df = pandas.DataFrame(df, index=[0])
df.to_csv("date.csv", index=False, encoding="utf-8-sig")
