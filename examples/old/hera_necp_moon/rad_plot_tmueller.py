#!/usr/bin/env python

from pathlib import Path  # noqa
import csv

import pandas  # noqa
import numpy  # noqa
import matplotlib  # noqa
from matplotlib import pyplot  # noqa

import kalast
from kalast.util import DPR, RPD, SPEED_LIGHT, JANSKY  # noqa


def split_time(name: str) -> str:
    split = name.split("_")
    yyyymmdd, hhmmss = split[2].split("T")
    yyyy = yyyymmdd[0:4]
    mm = yyyymmdd[4:6]
    dd = yyyymmdd[6:8]
    hr = hhmmss[0:2]
    mn = hhmmss[2:4]
    sc = hhmmss[4:6]
    return yyyy, mm, dd, hr, mn, sc


def split_time_tmueller(name: str) -> str:
    split = name.split("_")
    yyyymmdd, hhmmss = split[:2]
    yyyy = yyyymmdd[0:4]
    mm = yyyymmdd[4:6]
    dd = yyyymmdd[6:8]
    hr = hhmmss[0:2]
    mn = hhmmss[2:4]
    sc = hhmmss[4:6]
    return yyyy, mm, dd, hr, mn, sc


df = pandas.read_csv("work/rad/irrad.csv")
df2 = pandas.read_csv("work/rad/spec_irrad_jy.csv")

dm = []
with open("in/EarthMoon_list_LunarInbandGeom_20250130.csv") as f:
    for ii, row in enumerate(csv.reader(f)):
        row = [c.strip() for c in row]
        if ii > 40:
            dm.append(row)

dm2 = []
with open("in/EarthMoon_list_TPMpredictions_20241106_fluxdensity.csv") as f:
    for ii, row in enumerate(csv.reader(f)):
        row = [c.strip() for c in row]
        if ii > 9:
            dm2.append(row)

images = []
et = []
filters = []
spec_irrad_jy1 = []
spec_irrad_jy2 = []
irrad2 = []
irrad1 = []
irrad2 = []
d1 = []
d2 = []
pha1 = []
pha2 = []
sr1 = []
sr2 = []

# Iterating tmueller csv and incrementing a counter of kalast row to compare
# corresponding images.
ii2 = 0
for ii1, row1 in enumerate(dm):
    if row1[2] == "FW-close":
        continue

    row1_2 = dm2[ii1]

    date1 = split_time_tmueller(row1[0])
    while True:
        row2 = df.iloc[ii2].to_list()
        row2_2 = df2.iloc[ii2].to_list()
        date2 = split_time(row2[0])
        if date1 == date2:
            break
        ii2 += 1

    images.append(row2[0])
    et.append(row2[1])
    filters.append(row1[2])
    spec_irrad_jy1.append([float(e) for e in row1_2[1:]])
    spec_irrad_jy2.append([float(e) for e in row2_2[1:]])
    irrad1.append(float(row1[3]))
    irrad2.append(float(row2[4]))
    d1.append(float(row1[5]))
    d2.append(float(row2[6]))
    pha1.append(float(row1[7]))
    pha2.append(float(row2[7]))
    sr1.append(float(row1[9]))
    sr2.append(float(row2[8]))

spec_irrad_jy1 = numpy.array(spec_irrad_jy1)
spec_irrad_jy2 = numpy.array(spec_irrad_jy2)
irrad1 = numpy.array(irrad1)
irrad2 = numpy.array(irrad2)
d1 = numpy.array(d1)
d2 = numpy.array(d2)
pha1 = numpy.array(pha1)
pha2 = numpy.array(pha2)
sr1 = numpy.array(sr1)
sr2 = numpy.array(sr2)

wl = numpy.linspace(5, 15, 21)
spec_irrad_ratio_mean_v_w = (spec_irrad_jy2 / spec_irrad_jy1).mean(axis=0)
spec_irrad_ratio_mean = (spec_irrad_jy2 / spec_irrad_jy1).mean(axis=1)

ii_wide = [ii for ii, filter_ in enumerate(filters) if filter_ == "filter-wide"]

kalast.plot.style.load()

iit = 0
fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Wavelength [um]")
ax.set_ylabel("Spectral irradiance [Jy]")
ax.scatter(
    wl, spec_irrad_jy1[iit], s=10, fc="none", marker="s", ec="r", label="TMueller"
)
ax.scatter(wl, spec_irrad_jy2[iit], s=10, fc="none", marker="o", ec="b", label="kalast")
ax.set_xlim(4, 16)
# ax.set_ylim(0, 1.0e9)
# ax.set_yscale("log")
ax.legend()
fig.savefig("tmueller_spec_irrad_jy.png", bbox_inches="tight", dpi=300)

fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Ephemeris time images")
ax.set_ylabel("Irradiance [W/m2]")
ax.scatter(et, irrad1, s=10, fc="none", marker="s", ec="r", label="TMueller")
ax.scatter(et, irrad2, s=10, fc="none", marker="o", ec="b", label="kalast")
# ax.set_xlim(5, 15)
# ax.set_ylim(0, 6e-5)
ax.legend()
fig.savefig("tmueller_irrad.png", bbox_inches="tight", dpi=300)

fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Distance [AU]")
ax.set_ylabel("Irradiance [W/m2]")
ax.scatter(d1, irrad1, s=10, fc="none", marker="s", ec="r", label="TMueller")
ax.scatter(d2, irrad2, s=10, fc="none", marker="o", ec="b", label="kalast")
ax.set_xlim(0.01, 0.04)
# ax.set_ylim(0, 6e-5)
# ax.set_yscale("log")
ax.legend()
fig.savefig("tmueller_irrad_v_d.png", bbox_inches="tight", dpi=300)

iit = 0
fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Wavelength [um]")
ax.set_ylabel("Spectral irradiance [Jy] kalast/TMueller")
ax.scatter(
    wl,
    spec_irrad_jy2[iit] / spec_irrad_jy1[iit],
    s=10,
    fc="none",
    marker="s",
    ec="k",
)
ax.set_xlim(4, 16)
# ax.set_ylim(0.9, 1.3)
fig.savefig("tmueller_spec_irrad_jy_ratio.png", bbox_inches="tight", dpi=300)

fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Wavelength [um]")
ax.set_ylabel("Spectral irradiance [Jy] kalast/TMueller")
ax.scatter(wl, spec_irrad_ratio_mean_v_w, s=10, fc="none", marker="s", ec="k")
ax.set_xlim(4, 16)
# ax.set_ylim(0.9, 1.3)
fig.savefig("tmueller_spec_irrad_jy_ratio_mean_v_w.png", bbox_inches="tight", dpi=300)

fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Distance [AU]")
ax.set_ylabel("Spectral irradiance [Jy] kalast/TMueller")
ax.scatter(
    d2[ii_wide], spec_irrad_ratio_mean[ii_wide], s=10, fc="none", marker="s", ec="k"
)
ax.set_xlim(0.01, 0.04)
# ax.set_ylim(0.9, 1.3)
fig.savefig("tmueller_spec_irrad_jy_ratio_mean_v_d.png", bbox_inches="tight", dpi=300)

fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Phase angle [°]")
ax.set_ylabel("Spectral irradiance [Jy] kalast/TMueller")
ax.scatter(
    pha2[ii_wide], spec_irrad_ratio_mean[ii_wide], s=10, fc="none", marker="s", ec="k"
)
ax.set_xlim(76, 83)
# ax.set_ylim(0.9, 1.3)
fig.savefig("tmueller_spec_irrad_jy_ratio_mean_v_pha.png", bbox_inches="tight", dpi=300)

fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Distance [AU]")
ax.set_ylabel("Irradiance [W/m2] kalast/TMueller")
ax.scatter(d2[ii_wide], (irrad2 / irrad1)[ii_wide], s=10, fc="none", marker="s", ec="k")
ax.set_xlim(0.01, 0.04)
# ax.set_ylim(0.9, 1.2)
fig.savefig("tmueller_irrad_ratio_v_d.png", bbox_inches="tight", dpi=300)

fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Phase angle [°]")
ax.set_ylabel("Irradiance [W/m2] kalast/TMueller")
ax.scatter(
    pha2[ii_wide], (irrad2 / irrad1)[ii_wide], s=10, fc="none", marker="s", ec="k"
)
ax.set_xlim(76, 83)
# ax.set_ylim(0.9, 1.2)
fig.savefig("tmueller_irrad_ratio_v_pha.png", bbox_inches="tight", dpi=300)

# fig, ax = pyplot.subplots(figsize=(6, 4))
# ax.set_xlabel("Distance [AU]")
# ax.set_ylabel("Phase angle [°]")
# ax.scatter(d1, -pha1, s=10, fc="none", marker="s", ec="r", label="TMueller")
# ax.scatter(d2, pha2, s=10, fc="none", marker="o", ec="b", label="kalast")
# ax.set_xlim(0.01, 0.04)
# ax.set_ylim(76, 83)
# ax.legend()
# fig.savefig("tmueller_pha_v_d.png", bbox_inches="tight", dpi=300)
#
# fig, ax = pyplot.subplots(figsize=(6, 4))
# ax.set_xlabel("Distance [AU]")
# ax.set_ylabel("Steradian")
# ax.scatter(d1, sr1, s=10, fc="none", marker="s", ec="r", label="TMueller")
# ax.scatter(d2, sr2, s=10, fc="none", marker="o", ec="b", label="kalast")
# ax.set_xlim(0.01, 0.04)
# ax.set_ylim(0, 3.5e-6)
# ax.legend()
# fig.savefig("tmueller_sr_v_d.png", bbox_inches="tight", dpi=300)
