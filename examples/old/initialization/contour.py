#!/usr/bin/env python

import numpy
import pandas
from matplotlib import pyplot

import kalast


a = 0.1
e = 0.9
x = numpy.linspace(0.1, 0.625, 100)
y = numpy.linspace(0.4, 5, 100)
X, Y = numpy.meshgrid(y, x)
tmp = numpy.array(
    [
        [
            kalast.tpm.core.effective_temperature(y[jj], x[ii], 0, 1)
            for jj in range(0, y.size)
        ]
        for ii in range(0, x.size)
    ]
)

df = {}
df["x"] = X.flatten()
df["y"] = Y.flatten()
df["tmp"] = tmp.flatten()
df = pandas.DataFrame(df)
df.to_csv("contour.csv", index=False, encoding="utf-8-sig")


kalast.plot.style.load("paper")
fig, ax = pyplot.subplots()
ax.set_xlabel("Heliocentric distance (AU)")
ax.set_ylabel("Surface area ratios and properties")
levels = numpy.arange(0, 601, 5)
cticks = numpy.arange(90, 561, 30)
cf = ax.contour(X, Y, tmp)
ax.clabel(cf)
ax.set_xlim(0.4, 5.0)
ax.set_ylim(0.1, 0.625)
# kalast.plot.style.use_formatter_decimal_round(ax)
fig.savefig("contour.png")
fig.savefig("contour.svg")
