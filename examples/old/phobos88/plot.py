import numpy
from matplotlib import pyplot
import matplotlib

import kalast
from kalast.util import DPR


def smap(surf, sph, tmp, vmin, vmax, path_out):
    fig, axs = pyplot.subplots(2, 1, figsize=(15, 7.3), height_ratios=[9.5, 0.5])
    ax = axs[0]
    ax.set_xlabel("Longitude (°)")
    ax.set_ylabel("Latitude (°)")
    ax.set_xlim(-180, 180)
    ax.set_ylim(-90, 90)
    loc = matplotlib.ticker.MultipleLocator(base=30)
    ax.xaxis.set_major_locator(loc)
    loc = matplotlib.ticker.MultipleLocator(base=30)
    ax.yaxis.set_major_locator(loc)
    cnorm = matplotlib.colors.Normalize(vmin=vmin, vmax=vmax)
    # cmap = matplotlib.cm.cividis.resampled(14)
    cmap = matplotlib.cm.inferno.resampled(14)
    mappable = matplotlib.cm.ScalarMappable(cmap=cmap, norm=cnorm)
    cnormv = cnorm(tmp)
    cmapv = cmap(cnormv)

    # lon = sph[:, 0]
    # lat = sph[:, 1]

    for jj in range(0, sph.shape[0]):
        a = kalast.util.cart2sph(surf.mesh.triangles[jj, 0])[:2] * DPR
        b = kalast.util.cart2sph(surf.mesh.triangles[jj, 1])[:2] * DPR
        c = kalast.util.cart2sph(surf.mesh.triangles[jj, 2])[:2] * DPR
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
            color=cmapv[jj],
            edgecolor="k",
            lw=1,
            joinstyle="bevel",
        )
        if trisph2 is not None:
            ax.fill(
                trisph2[:, 0],
                trisph2[:, 1],
                color=cmapv[jj],
                edgecolor="k",
                lw=1,
                joinstyle="bevel",
            )
            trisph2 = None

    ax = axs[1]
    ax.set_visible(False)
    cax = fig.add_axes([0.26, 0.04, 0.5, 0.03])
    _cb = fig.colorbar(
        mappable,
        label="Temperature (K)",
        orientation="horizontal",
        cax=cax,
        # ticks=params.cbar.get_ticks(),
        # format=params.cbar.get_formatter(),
    )

    if path_out.is_dir():
        path_out = path_out / "tmap.png"
    fig.savefig(path_out, bbox_inches="tight", dpi=300)


def daily_surf(ts, tmp, path_out, xlim=None, xlabel=None, ylabel=None):
    fig, ax = pyplot.subplots(figsize=(6, 4))
    if xlabel is not None:
        ax.set_xlabel(xlabel)
    if ylabel is not None:
        ax.set_ylabel(ylabel)
    ax.plot(ts, tmp, lw=1, color="k")
    if xlim is not None:
        ax.set_xlim(xlim)
    # ax.set_ylim(0, None)
    # ax.set_yscale("log")
    # pyplot.legend()
    if path_out.is_dir():
        path_out = path_out / "surf.png"
    fig.savefig(path_out, bbox_inches="tight", dpi=300)
    # pyplot.show()


def depth(z, tmp, path_out, ylim=None, unity="cm"):
    fig, ax = pyplot.subplots(figsize=(6, 4))
    ax.set_xlabel("Temperature [K]")
    if unity is not None:
        ax.set_ylabel(f"Depth [{unity}]")
    for ii in range(0, tmp.shape[0]):
        ax.plot(tmp[ii, :], z, lw=1, color="k")
    # ax.set_xlim(0, None)
    if ylim is not None:
        ax.set_ylim(ylim)
    if path_out.is_dir():
        path_out = path_out / "depth.png"
    fig.savefig(path_out, bbox_inches="tight", dpi=300)
