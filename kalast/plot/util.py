#!/usr/bin/env python

import numpy
from matplotlib import pyplot
from matplotlib.ticker import MultipleLocator
import matplotlib

import kalast
from kalast.util import DPR


class MidPointLogNorm(matplotlib.colors.LogNorm):
    def __init__(self, vmin=None, vmax=None, midpoint=None, clip=False):
        matplotlib.colors.LogNorm.__init__(self, vmin=vmin, vmax=vmax, clip=clip)
        self.midpoint = midpoint

    def __call__(self, value, clip=None):
        x, y = (
            [numpy.log(self.vmin), numpy.log(self.midpoint), numpy.log(self.vmax)],
            [0, 0.5, 1],
        )
        return numpy.ma.masked_array(numpy.interp(numpy.log(value), x, y))


def smap(
    mesh: kalast.mesh.Mesh,
    colors: numpy.array,
    label: str = None,
    mappable: matplotlib.cm.ScalarMappable = None,
    name: str = "smap.png",
):
    fig, axs = pyplot.subplots(2, 1, figsize=(15, 7.3), height_ratios=[9.5, 0.5])
    ax = axs[0]

    # fig = pyplot.figure(figsize=(15, 7.3))
    # ax = pyplot.gca()

    ax.set_xlabel("Longitude (°)")
    ax.set_ylabel("Latitude (°)")
    ax.set_xlim(-180, 180)
    ax.set_ylim(-90, 90)
    loc = matplotlib.ticker.MultipleLocator(base=30)
    ax.xaxis.set_major_locator(loc)
    loc = matplotlib.ticker.MultipleLocator(base=30)
    ax.yaxis.set_major_locator(loc)

    for iif in range(0, len(mesh.facets)):
        a = mesh.positions[iif * 3 + 0, :]
        b = mesh.positions[iif * 3 + 1, :]
        c = mesh.positions[iif * 3 + 2, :]
        a = kalast.math.cart2sph(a)[1:] * DPR
        b = kalast.math.cart2sph(b)[1:] * DPR
        c = kalast.math.cart2sph(c)[1:] * DPR
        trisph = numpy.array([a, b, c])
        trisph2 = None
        s1 = b - a
        s2 = c - b
        s3 = a - c
        condx = numpy.abs(numpy.array([s1[0], s2[0], s3[0]])) > 180

        # d1 = numpy.linalg.norm(s1)
        # d2 = numpy.linalg.norm(s2)
        # d3 = numpy.linalg.norm(s3)
        # cond = numpy.array([d1, d2, d3]) > 200
        # condy = numpy.abs(numpy.array([s1[1], s2[1], s3[1]])) > 180

        if condx.sum() >= 1:
            # print(jj, a, b, c, s1, s2, s3, condx)
            signp = []
            signn = []
            for kk in range(0, 3):
                if trisph[kk, 0] >= 0:
                    signp.append(kk)
                else:
                    signn.append(kk)
            # print(signp, signn)
            for kk in signn:
                trisph[kk, 0] = 360 - abs(trisph[kk, 0])
            trisph2 = numpy.array([a, b, c])
            for kk in signp:
                if trisph[kk, 0] != 0:
                    trisph2[kk, 0] = -360 + abs(trisph[kk, 0])
            # print(trisph[:, 0])
            # print(trisph2[:, 0])

        ax.fill(
            trisph[:, 0],
            trisph[:, 1],
            color=colors[iif],
            edgecolor="k",
            lw=1,
            joinstyle="bevel",
        )
        if trisph2 is not None:
            ax.fill(
                trisph2[:, 0],
                trisph2[:, 1],
                color=colors[iif],
                edgecolor="k",
                lw=1,
                joinstyle="bevel",
            )
            trisph2 = None

    ax = axs[1]
    ax.set_visible(False)
    cax = fig.add_axes([0.26, 0.04, 0.5, 0.03])
    _cb = fig.colorbar(mappable, label=label, orientation="horizontal", cax=cax)

    # if cticks is not None:
    #     _cb.set_ticks(cticks)

    fig.savefig(name, bbox_inches="tight", dpi=300)


def depth(z, tmp, ylim=None, unity="cm", name="depth.png"):
    fig, ax = pyplot.subplots(figsize=(6, 4))
    ax.set_xlabel("Temperature [K]")
    if unity is not None:
        ax.set_ylabel(f"Depth [{unity}]")
    for ii in range(0, tmp.shape[0]):
        ax.plot(tmp[ii, :], z, lw=1, color="k")
    # ax.set_xlim(0, None)
    if ylim is not None:
        ax.set_ylim(ylim)
    fig.savefig(name, bbox_inches="tight", dpi=300)


def daily_surf(
    et, y, xlim=None, ylim=None, xlabel=None, ylabel=None, legend=None, name="surf.png"
):
    fig, ax = pyplot.subplots(figsize=(6, 4))
    if xlabel is not None:
        ax.set_xlabel(xlabel)
    if ylabel is not None:
        ax.set_ylabel(ylabel)
    if not isinstance(y, list):
        y = [y]
    lbl = None
    for ii in range(len(y)):
        if legend is not None:
            lbl = legend[ii]
        ax.plot(et, y[ii], lw=1, color="k", label=lbl)
    if xlim is not None:
        ax.set_xlim(xlim)
    if ylim is not None:
        ax.set_ylim(ylim)
    # ax.set_ylim(0, None)
    # ax.set_yscale("log")

    if legend is not None:
        pyplot.legend(frameon=False)
    fig.savefig(name, bbox_inches="tight", dpi=300)
    # pyplot.show()


def daily_lola(et, lola, xlim=None, ylim=None, loc=None, xlabel=None, ylabel=None):
    fig, ax = pyplot.subplots(figsize=(6, 4))
    if xlabel is not None:
        ax.set_xlabel(xlabel)
    if ylabel is not None:
        ax.set_ylabel(ylabel)
    ax.scatter(et, lola, s=10, lw=0.5, marker="+", color="k")
    if xlim is not None:
        ax.set_xlim(xlim)
    if ylim is not None:
        ax.set_ylim(ylim)
    if loc is not None:
        ax.yaxis.set_major_locator(MultipleLocator(base=loc))
    fig.savefig("lola.png", bbox_inches="tight", dpi=300)
