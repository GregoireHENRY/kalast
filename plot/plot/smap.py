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

cbar_bottom = True
interp = True
proj = "cyl"


def plot(d: dict[str, Data], save: bool = False):
    pyplot.style.use("plot/main.mplstyle")
    fig, ax = pyplot.subplots(figsize=(15, 7.3))

    if proj == "cyl":
        map = Basemap(
            ax=ax,
            projection="cyl",
            llcrnrlat=-90,
            urcrnrlat=90,
            llcrnrlon=-180,
            urcrnrlon=180,
            resolution="c",
        )
    elif proj == "moll":
        map = Basemap(
            ax=ax,
            projection="moll",
            lon_0=0,
            resolution="c",
        )
    elif proj == "ortho":
        map = Basemap(ax=ax, projection="ortho", lon_0=40, lat_0=20, resolution="l")
    elif proj == "nsper":
        map = Basemap(
            ax=ax,
            projection="nsper",
            lon_0=40,
            lat_0=40,
            satellite_height=3000 * 1000.0,
            resolution="l",
        )

    if proj is not None and proj != "cyl":
        map.drawparallels(numpy.arange(-90.0, 120.0, 30.0))
        map.drawmeridians(numpy.arange(0.0, 420.0, 60.0))

    if proj is None or proj == "cyl":
        # ax.set_xlabel("Longitude (°)")
        # ax.set_ylabel("Latitude (°)")

        ax.set_xlim(-180, 180)
        ax.set_ylim(-90, 90)

    cmin = 0
    cmax = 400
    vmin = d["tmp"].min()
    vmax = d["tmp"].max()

    norm = matplotlib.colors.Normalize(vmin=cmin, vmax=cmax)
    cmap = matplotlib.cm.inferno

    lvl = 1
    lvls = numpy.arange(cmin, cmax + lvl / 10, lvl, dtype=int)

    p = d["sph"] * util.DPR
    xy = p[:, :2]
    z = d["tmp"][0]

    # n = z.size
    nx = 50
    ny = nx

    xgrid = numpy.linspace(-numpy.pi, numpy.pi, nx) * util.DPR
    ygrid = numpy.linspace(-numpy.pi / 2, numpy.pi / 2, ny) * util.DPR
    grid = numpy.array(numpy.meshgrid(xgrid, ygrid))

    fgrid = grid.reshape(2, -1).T
    zflat = scipy.interpolate.RBFInterpolator(xy, z)(fgrid)
    zgrid = zflat.reshape(nx, ny)

    # tmap = map.contourf(*grid, zgrid, levels=lvls, cmap=cmap, norm=norm, latlon=True)

    # pcolormesh
    # contourf
    tmap = map.contourf(
        *grid,
        zgrid,
        cmap=cmap,
        norm=norm,
        latlon=True,
        levels=lvls,
        # shading="gouraud",
    )

    # p = ax.scatter(*xy.T, c=z, s=50, ec="k", vmin=0, vmax=400)

    if cbar_bottom:
        orientation = "horizontal"
        pad = 0.05
        shrink = 0.4
        aspect = 30
    else:
        orientation = "vertical"
        pad = 0.02
        shrink = 0.855
        aspect = 23

    _ = fig.colorbar(
        tmap,
        label="Temperature (K)",
        orientation=orientation,
        shrink=shrink,
        pad=pad,
        aspect=aspect,
    )

    cax = fig.axes[-1]

    loc = ticker.MultipleLocator(base=50.0)
    if not cbar_bottom:
        cax.plot([0, 0.15], [vmin, vmin], color="k", linewidth=0.9)
        cax.plot([0, 0.15], [vmax, vmax], color="k", linewidth=0.9)
        cax.text(-0.08, vmin, f"{vmin:.0f}", ha="right", va="center_baseline")
        cax.text(-0.08, vmax, f"{vmax:.0f}", ha="right", va="center_baseline")
        cax.yaxis.set_major_locator(loc)
    else:
        cax.plot([vmin, vmin], [0.75, 1.0], color="k", linewidth=0.9)
        cax.plot([vmax, vmax], [0.75, 1.0], color="k", linewidth=0.9)
        cax.text(vmin, 1.1, f"{vmin:.0f}", ha="center", va="bottom")
        cax.text(vmax, 1.1, f"{vmax:.0f}", ha="center", va="bottom")
        cax.xaxis.set_major_locator(loc)

    if proj is None or proj == "cyl":
        loc = ticker.MultipleLocator(base=30.0)
        ax.xaxis.set_major_locator(loc)

        loc = ticker.MultipleLocator(base=30)
        ax.yaxis.set_major_locator(loc)

    if save:
        out = Path("out/surface")
        if not out.exists():
            out.mkdir(parents=True)

        fig.savefig(out / "temperature.png", bbox_inches="tight", dpi=300)

    pyplot.show()