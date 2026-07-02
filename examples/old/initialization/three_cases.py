#!/usr/bin/env python

import numpy
import pandas
from matplotlib import pyplot

import kalast


a = 0.1
e = 0.9
x = numpy.array([1.0 / 2, 1 / 3, 1 / 4])
y = numpy.linspace(0.4, 5, 100)
tmp = numpy.array(
    [
        [
            kalast.tpm.core.effective_temperature(y[jj], x[ii], a, e)
            for jj in range(0, y.size)
        ]
        for ii in range(0, x.size)
    ]
)
cols = ["#A6B1E1", "#B4869F", "#4E4C67"]
lbls = ["1/2", "1/3", "1/4"]


df = {}
df["dist"] = y
for x_, tmp_ in zip(lbls, tmp):
    df[x_] = tmp_
df = pandas.DataFrame(df)
df.to_csv("three-cases.csv", index=False, encoding="utf-8-sig")


kalast.plot.style.load("paper")
fig, ax = pyplot.subplots()
ax.set_xlabel("Heliocentric distance (AU)")
ax.set_ylabel("Temperature (K)")
for ii in range(0, x.size):
    ax.plot(y, tmp[ii], lw=1, color=cols[ii], label=lbls[ii])
ax.set_xlim(0.4, 5)
ax.set_ylim(100, 600)
ax.legend()
kalast.plot.style.use_formatter_decimal_round(ax)
fig.savefig("three-cases.png")
fig.savefig("three-cases.svg")
