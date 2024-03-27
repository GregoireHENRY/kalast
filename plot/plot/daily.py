#!/usr/bin/env python

from pathlib import Path

import matplotlib
import numpy
import scipy
import util
from matplotlib import pyplot, ticker
from mpl_toolkits.basemap import Basemap

Data = dict[str, numpy.array]
Data_m = dict[str, str]


def plot(d: dict[str, Data], save: bool = False):
    pyplot.style.use("plot/main.mplstyle")
    fig, ax = pyplot.subplots(figsize=(15, 7.3))

    ax.set_xlabel("Time elapsed (h)")
    ax.set_ylabel("Temperature (K)")

    dt = 200
    xmax = d["tmp-cols"].shape[0] * dt
    x = numpy.arange(0, xmax, dt) / 3600
    y = d["tmp-cols"][:, 0]

    ax.plot(x, y)

    ax.grid(True)

    ax.set_xlim(0, x.max())
    ax.set_ylim(200, 400)

    # loc = ticker.MultipleLocator(base=30.0)
    # ax.xaxis.set_major_locator(loc)

    loc = ticker.MultipleLocator(base=50)
    ax.yaxis.set_major_locator(loc)

    if save:
        out = Path("out/surface")
        if not out.exists():
            out.mkdir(parents=True)

        fig.savefig(out / "daily.png", bbox_inches="tight", dpi=300)

    pyplot.show()