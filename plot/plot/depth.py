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

    ax.set_xlabel("Temperature (K)")
    ax.set_ylabel("Depth (m)")

    x1 = d["tmp-cols"].min(axis=0)
    x2 = d["tmp-cols"].mean(axis=0)
    x3 = d["tmp-cols"].max(axis=0)
    y = d["depth"]

    ax.plot(x1, y)
    ax.plot(x2, y)
    ax.plot(x3, y)

    ax.grid(True)

    ax.set_xlim(200, 400)
    ax.set_ylim(1.5, y[1])

    ax.set(yscale="log")

    # loc = ticker.MultipleLocator(base=30.0)
    # ax.xaxis.set_major_locator(loc)

    loc = ticker.MultipleLocator(base=50)
    ax.xaxis.set_major_locator(loc)

    if save:
        out = Path("out/surface")
        if not out.exists():
            out.mkdir(parents=True)

        fig.savefig(out / "depth.png", bbox_inches="tight", dpi=300)

    pyplot.show()