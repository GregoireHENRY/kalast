#!/usr/bin/env python

import pandas
from matplotlib import pyplot

import kalast

import setup
import tpm

df1 = {}
df1["depth"] = setup.z
df2 = df1.copy()

kalast.plot.style.load("paper")
fig, ax = pyplot.subplots()
ax.set_xlabel("Temperature (K)")
ax.set_ylabel("Depth (m)")

for ii, t_ in enumerate(setup.t):
    T1 = tpm.Te[ii]
    T2 = setup.f(setup.z, t_)
    (l1,) = ax.plot(T1, setup.z, lw=1, color="k")
    (l2,) = ax.plot(T2, setup.z, lw=1, ls="--", color="r")
    if ii == 0:
        l1.set_label("Numerical")
        l2.set_label("Analytical")

    df1[t_] = T1
    df2[t_] = T2

ax.set_xlim(200, 400)
ax.set_ylim(setup.zf, 0)
kalast.plot.style.use_formatter_decimal_round(ax)
ax.legend()
fig.savefig("tvd.png")
fig.savefig("tvd.svg")

df = pandas.DataFrame(df1)
df.to_csv("numerical.csv", index=False, encoding="utf-8-sig")
df = pandas.DataFrame(df2)
df.to_csv("analytical.csv", index=False, encoding="utf-8-sig")
