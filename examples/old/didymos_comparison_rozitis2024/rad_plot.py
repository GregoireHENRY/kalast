#!/usr/bin/env python

from pathlib import Path  # noqa

import pandas
import numpy
import matplotlib  # noqa
from matplotlib import pyplot  # noqa

import kalast
from kalast.util import DPR, RPD, SPEED_LIGHT, JANSKY, AU  # noqa


images = numpy.load("work/scene/images.npy", allow_pickle=True)
et_images = numpy.load("work/scene/et_images.npy")
filters = numpy.load("work/scene/filters.npy", allow_pickle=True)
ii_open_wide = numpy.load("work/scene/ii_open_wide.npy")
ds = numpy.load("work/scene/ds.npy")
do = numpy.load("work/scene/do.npy")
pha = numpy.load("work/scene/pha.npy")
ster = numpy.load("work/scene/ster.npy")
wlu = numpy.load("work/scene/wlu.npy")
spec_irrad = numpy.load("work/rad/spec_irrad.npy")
rad = numpy.load("work/rad/rad.npy")
irrad = numpy.load("work/rad/irrad.npy")

wl = wlu * 1e-6

n = et_images.size
nw = wlu.size

spec_irrad_jy = spec_irrad * wl**2 / SPEED_LIGHT * JANSKY

df = {}
df["image"] = images
for iiw in range(200, 1201, 50):
    wl_ = round(wl[iiw] * 1e6, 1)
    df[wl_] = spec_irrad_jy[:, iiw]
df = pandas.DataFrame(df)
df.to_csv("spec_irrad_jy.csv", index=False, float_format="%.5e", encoding="utf-8-sig")

kalast.plot.style.load()
fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Wavelength [um]")
ax.set_ylabel("Spectral Irradiance [Jy]")
# ax.plot(df.keys()[1:], df.iloc[3, 1:], lw=1, color="k")
ax.scatter(df.keys()[1:], df.iloc[3, 1:], s=10, fc="none", marker="s", ec="k")
ax.set_xlim(4, 16)
# ax.set_yscale("log")
fig.savefig("spec_irrad_jy.png", bbox_inches="tight", dpi=300)

df = {}
df["image"] = images
df["et"] = et_images
df["filter"] = filters
df["rad[W/m2/sr]"] = rad
df["irrad[W/m2]"] = irrad
df["r[au]"] = ds / AU
df["d[au]"] = do / AU
df["pha[°]"] = pha * DPR
df["ster"] = ster
df = pandas.DataFrame(df)
df.to_csv("irrad.csv", index=False, encoding="utf-8-sig")

fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Ephemeris time images")
ax.set_ylabel("Irradiance [W/m2]")
ax.scatter(df.iloc[:, 1], df.iloc[:, 4], s=10, fc="none", marker="s", ec="k")
# ax.set_ylim(0, 6e-5)
fig.savefig("irrad.png", bbox_inches="tight", dpi=300)

fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Distance [AU]")
ax.set_ylabel("Irradiance [W/m2]")
ax.scatter(df.iloc[:, 6], df.iloc[:, 4], s=10, fc="none", marker="s", ec="k")
ax.set_xlim(0.01, 0.04)
# ax.set_ylim(0, 6e-5)
fig.savefig("irrad_v_d.png", bbox_inches="tight", dpi=300)

fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Phase angle [°]")
ax.set_ylabel("Irradiance [W/m2/sr]")
ax.scatter(
    df.iloc[ii_open_wide, 7],
    df.iloc[ii_open_wide, 4],
    s=10,
    fc="none",
    marker="s",
    ec="k",
)
ax.set_xlim(76, 83)
# ax.set_ylim(0, 6e-5)
fig.savefig("irrad_v_pha(wide).png", bbox_inches="tight", dpi=300)

fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Ephemeris time images")
ax.set_ylabel("Radiance [W/m2/sr]")
ax.scatter(df.iloc[:, 1], df.iloc[:, 3], s=10, fc="none", marker="s", ec="k")
# ax.set_xlim(5, 15)
# ax.set_ylim(0, 7e3)
fig.savefig("rad.png", bbox_inches="tight", dpi=300)

fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Distance [AU]")
ax.set_ylabel("Radiance [W/m2/sr]")
ax.scatter(df.iloc[:, 6], df.iloc[:, 3], s=10, fc="none", marker="s", ec="k")
ax.set_xlim(0.01, 0.04)
# ax.set_ylim(0, 7e3)
fig.savefig("rad_v_d.png", bbox_inches="tight", dpi=300)

fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Phase angle [°]")
ax.set_ylabel("Radiance [W/m2/sr]")
ax.scatter(df.iloc[:, 7], df.iloc[:, 3], s=10, fc="none", marker="s", ec="k")
ax.set_xlim(76, 83)
# ax.set_ylim(0, 7e3)
fig.savefig("rad_v_pha(wide).png", bbox_inches="tight", dpi=300)
