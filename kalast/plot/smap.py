#!/usr/bin/env python

from pathlib import Path

import matplotlib
import numpy
import scipy
import util
from matplotlib import pyplot, ticker, tri
import pywavefront

Data = dict[str, numpy.array]
Data_m = dict[str, str]

cbar_bottom = True


def plot(d: dict[str, Data]):
    pyplot.plot.style.use("plot/main.mplstyle")

    # fig, ax = pyplot.plot.subplots(figsize=(15, 7.3))

    fig, axs = pyplot.plot.subplots(2, 1, figsize=(15, 7.3), height_ratios=[9.5, 0.5])
    ax = axs[0]

    if d["show_axes_label"]:
        ax.set_xlabel("Longitude (°)")
        ax.set_ylabel("Latitude (°)")

    ax.set_xlim(-180, 180)
    ax.set_ylim(-90, 90)

    p = d["sph"]
    xy = p[:, :2]
    z = d["data"]

    cmin = d["cmin"] if "cmin" in d else None
    cmax = d["cmax"] if "cmax" in d else None
    vmin = d["vmin"] if "vmin" in d else None
    vmax = d["vmax"] if "vmax" in d else None

    vvmax = vmax
    if vmax is None:
        vvmax = z.max()

    clabel = d["clabel"] if "clabel" in d else ""

    lvl = d["level"]
    if lvl is not None:
        lvls = (
            numpy.arange(cmin, cmax + lvl / 10, lvl, dtype=int)
            if cmin is not None and cmax is not None
            else None
        )

        if cmax is not None and lvls[-1] < cmax:
            lvls = numpy.append(lvls, cmax)
        else:
            lvls = None
    else:
        lvls = None

    if d["cnorm"] == "log":
        norm = matplotlib.colors.LogNorm(vmin=1e-5, vmax=vvmax)
    else:
        norm = matplotlib.colors.Normalize(vmin=cmin, vmax=vvmax)

    cmap = d["cmap"] if "cmap" in d else matplotlib.cm.viridis
    mapp = matplotlib.cm.ScalarMappable(cmap=cmap, norm=norm)

    # n = z.size
    nx = 30
    ny = 30

    xx = numpy.linspace(-numpy.pi, numpy.pi, nx) * util.DPR
    yy = numpy.linspace(-numpy.pi / 2, numpy.pi / 2, ny) * util.DPR
    xgrid, ygrid = numpy.meshgrid(xx, yy)

    if d["method"] == "RBFinterp":
        grid = numpy.array((xgrid, ygrid))
        fgrid = grid.reshape(2, -1).T
        zflat = scipy.interpolate.RBFInterpolator(xy, z)(fgrid)
        zgrid = zflat.reshape(nx, ny)
        _mmap = ax.contourf(xgrid, ygrid, zgrid, levels=lvls, cmap=cmap, norm=norm)

    elif d["method"] == "triinterp":
        triang = tri.Triangulation(xy[:, 0], xy[:, 1])
        interpolator = tri.LinearTriInterpolator(triang, z)
        xxgrid, yygrid = numpy.meshgrid(xgrid, ygrid)
        zz = interpolator(xxgrid, yygrid)
        _mmap = ax.contourf(xxgrid, yygrid, zz, cmap=cmap, norm=norm, levels=lvls)

    elif d["method"] == "griddata":
        zz = scipy.interpolate.griddata(
            (xy[:, 0], xy[:, 1]), z, (xx[None, :], yy[:, None]), method="linear"
        )
        if d["method_opt"] == "contour":
            _mmap = ax.contourf(xx, yy, zz, cmap=cmap, norm=norm, levels=lvls)

        elif d["method_opt"] == "pcolor":
            _mmap = ax.pcolor(xx, yy, zz, cmap=cmap, norm=norm)

        elif d["method_opt"] == "imshow":
            _mmap = ax.imshow(
                zz,
                extent=(xx.min(), xx.max(), yy.min(), yy.max()),
                cmap=cmap,
                norm=norm,
            )
        else:
            print("Warning: no options for method griddata found.")

    elif d["method"] == "tricontour":
        _mmap = ax.tricontourf(xy[:, 0], xy[:, 1], z, cmap=cmap, norm=norm, levels=lvls)

    elif d["method"] == "mesh":
        scene = pywavefront.Wavefront(d["path_mesh"], collect_faces=True)
        faces = scene.mesh_list[0].faces
        vertices = scene.vertices
        h = d["threshold_longitude_check"]

        for ii in range(len(faces)):
            a = util.cart2sph(*vertices[faces[ii][0]])[:2] * util.DPR
            b = util.cart2sph(*vertices[faces[ii][1]])[:2] * util.DPR
            c = util.cart2sph(*vertices[faces[ii][2]])[:2] * util.DPR
            s1 = b - a
            s2 = c - b
            s3 = a - c
            d1 = numpy.linalg.norm(s1)
            d2 = numpy.linalg.norm(s2)
            d3 = numpy.linalg.norm(s3)
            sph = numpy.array([a, b, c])

            cond = numpy.array([d1, d2, d3]) > h
            if cond.sum() >= 1:
                cond2 = sph[:, 0] == 180
                if cond2.sum() == 1:
                    sph[cond2, 0] = -180
                elif cond2.sum() == 2:
                    sph[cond2, 0] = -180

            lon = sph[:, 0]
            lat = sph[:, 1]
            value = z[ii] / vvmax
            color = util.cmapv_to_rbg(value, d["cmap"])
            ax.fill(
                lon,
                lat,
                color=color,
                edgecolor="k",
                lw=1,
                joinstyle="bevel",
            )
    else:
        print("Warning: no method found.")

    loc = ticker.MultipleLocator(base=30.0)
    ax.xaxis.set_major_locator(loc)

    loc = ticker.MultipleLocator(base=30)
    ax.yaxis.set_major_locator(loc)

    # for key, z in d["tmp-cols"].items():
    #     print(key, z.shape)
    #     xy = p[key, :2]
    #     ax.scatter(*xy.T, s=50, c="none", ec="k", lw=2)

    ax = axs[1]
    ax.set_visible(False)
    cax = fig.add_axes([0.25, 0.05, 0.5, 0.03])
    _cb = fig.colorbar(mapp, label=clabel, orientation=d["orientation"], cax=cax)

    # if cbar_bottom:
    #     orientation = "horizontal"
    #     pad = 0.05
    #     shrink = 0.4
    #     aspect = 30
    # else:
    #     orientation = "vertical"
    #     pad = 0.02
    #     shrink = 0.855
    #     aspect = 23

    # _ = fig.colorbar(
    #     mmap,
    #     label=clabel,
    #     orientation=orientation,
    #     shrink=shrink,
    #     pad=pad,
    #     aspect=aspect,
    # )

    # cax = fig.axes[-1]

    loc = ticker.MultipleLocator(base=d["cloc"] if "cloc" in d else 10.0)
    if d["show_limit_on_cbar"]:
        if d["orientation"] == "vertical":
            cax.plot([0, 0.15], [vmin, vmin], color="k", linewidth=0.9)
            cax.plot([0, 0.15], [vmax, vmax], color="k", linewidth=0.9)
            cax.text(-0.08, vmin, f"{vmin:.0f}", ha="right", va="center_baseline")
            cax.text(-0.08, vmax, f"{vmax:.0f}", ha="right", va="center_baseline")
            cax.yaxis.set_major_locator(loc)
        elif d["orientation"] == "horizontal":
            cax.plot([vmin, vmin], [0.75, 1.0], color="k", linewidth=0.9)
            cax.plot([vmax, vmax], [0.75, 1.0], color="k", linewidth=0.9)
            cax.text(vmin, 1.1, f"{vmin:.0f}", ha="center", va="bottom")
            cax.text(vmax, 1.1, f"{vmax:.0f}", ha="center", va="bottom")
            cax.xaxis.set_major_locator(loc)

    if d["save"]:
        out = Path("out")
        if not out.exists():
            out.mkdir(parents=True)

        fig.savefig(out / d["figname"], bbox_inches="tight", dpi=300)

    if d["show"]:
        pyplot.plot.show()
