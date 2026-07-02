#!/usr/bin/env python

import numpy
import pandas
from matplotlib import pyplot

import kalast

import setup
import dirichlet
import dirichlet_tpm


Te = numpy.zeros_like(dirichlet_tpm.Te)
err = numpy.zeros(setup.ne)
for iie_ in range(setup.ne):
    Te[iie_] = numpy.interp(dirichlet_tpm.z, dirichlet.z, dirichlet.Te[iie_])
    err[iie_] = (
        numpy.sum(numpy.abs(dirichlet_tpm.Te[iie_, :] - Te[iie_, :])) / dirichlet_tpm.nz
    )


df = {}
df["depth"] = dirichlet.z
df[0] = dirichlet.T0
for ii, time in enumerate(setup.te):
    df[time] = dirichlet.Te[ii, :]
df = pandas.DataFrame(df)
df.to_csv("analytical.csv", index=False, encoding="utf-8-sig")

df = {}
df["depth"] = dirichlet_tpm.z
for ii, time in enumerate(setup.te):
    df[time] = dirichlet_tpm.Te[ii, :]
df = pandas.DataFrame(df)
df.to_csv("numerical.csv", index=False, encoding="utf-8-sig")

df = {}
df["time"] = setup.te
df["err"] = err
df = pandas.DataFrame(df)
df.to_csv("error.csv", index=False, encoding="utf-8-sig")


kalast.plot.style.load("paper")
fig, ax = pyplot.subplots()
ax.set_xlabel("Temperature (K)")
ax.set_ylabel("Depth (m)")
ax.plot(dirichlet.T0, dirichlet.z, lw=2, color="k")
for ii, iie_ in enumerate(range(setup.ne)):
    (l1,) = ax.plot(dirichlet_tpm.Te[iie_, :], dirichlet_tpm.z, lw=1, color="k")
    (l2,) = ax.plot(dirichlet.Te[iie_, :], dirichlet.z, lw=1, ls="--", color="r")
    if ii == 0:
        l1.set_label("Numerical")
        l2.set_label("Analytical")
ax.set_xlim(0, 300)
ax.set_ylim(setup.L, 0)
kalast.plot.style.use_formatter_decimal_round(ax)
ax.legend()
fig.savefig("both.png")
fig.savefig("both.svg")

fig, ax = pyplot.subplots()
ax.set_xlabel("Time (h)")
ax.set_ylabel("Error")
ax.scatter(setup.te / 3600.0, err, marker="o", color="k")
ax.set_xlim(0, 40)
ax.set_ylim(0, 0.07)
kalast.plot.style.use_formatter_decimal_round(ax)
fig.savefig("error.png")
fig.savefig("error.svg")
