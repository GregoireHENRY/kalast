from pathlib import Path

import pandas
import numpy
import trimesh
import matplotlib  # noqa
from matplotlib import pyplot  # noqa
import spiceypy as spice
# import scipy
# from astropy.io import fits

import kalast
from kalast.util import DPR, RPD, SPEED_LIGHT, JANSKY  # noqa

spice.kclear()
spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm")

sc = kalast.spice_entities.hera
cam = sc.tiri
bod = kalast.spice_entities.deimos

_, _, bsight, _, bounds = spice.getfvn(cam.name, 4)
bnz = bounds[0, 2]

mesh = trimesh.load("work/deimos.obj")
sph = numpy.array([kalast.util.cart2sph(v) for v in mesh.triangles_center])
nf = mesh.faces.shape[0]

et_images = numpy.load("work/scene/et_images.npy")
images = numpy.load("work/scene/images.npy", allow_pickle=True)
tmp_all = numpy.load("work/scene/tmp_all.npy")
rad_all = numpy.load("work/rad/rad_all.npy")

# R = numpy.load("work/rad/R.npy")
R = numpy.ones((images.size, nf))

cols = ["lo", "la", "inc", "emi", "pha", "tmp", "rad", "R", "nx", "ny", "nz"]
labels = [
    "longitude [°]",
    "latitude [°]",
    "incidence [°]",
    "emission [°]",
    "phase angle [°]",
    "temperature [K]",
    "radiance [W/m2/sr]",
    "roughness factor",
    "normal-x",
    "normal-y",
    "normal-z",
]

for it in range(0, images.size):
    pxv = numpy.full((768, 1024, 11), numpy.nan)

    (psc, _lt) = spice.spkpos(sc.name, et_images[it], bod.frame, "none", bod.name)
    psc *= 1e3

    for jj in range(0, cam.px[1]):
        for kk in range(0, cam.px[0]):
            kj = numpy.array([kk, jj])
            try:
                b = bounds[0] / bnz
                b[:2] += 2 * bounds[2, :2] / bnz * kj / (cam.px - 1)
                b /= numpy.linalg.norm(b)
                sp, h, lo, la, pha, inc, emi = kalast.spice.cam_cpt_surf_2(
                    b, sc.tiri, bod, et_images[it]
                )
            except spice.NotFoundError:
                pass
            else:
                pxv[jj, kk, :2] = [lo * DPR, la * DPR]
                pxv[jj, kk, 2] = inc * DPR
                pxv[jj, kk, 3] = emi * DPR
                pxv[jj, kk, 4] = pha * DPR
                ds = [
                    kalast.util.distance_haversine(
                        bod.diameter, lo, la, sph_[0], sph_[1]
                    )
                    for sph_ in sph
                ]
                jj2 = numpy.argmin(ds)

                pxv[jj, kk, 5] = tmp_all[it, jj2]
                pxv[jj, kk, 6] = rad_all[it, jj2]
                pxv[jj, kk, 7] = R[it, jj2]
                pxv[jj, kk, 8:11] = mesh.face_normals[jj2]

    ii_nonan = ~numpy.isnan(pxv[:, :, 0])
    n_nonan = ii_nonan.sum()

    if n_nonan == 0:
        print(f"No camera pixel intercepted with {bod.name} for {images[it]}")

    pxv_nonan = pxv[ii_nonan]
    x, y = numpy.where(ii_nonan)

    path_out = Path("tmp")

    numpy.save(path_out / f"campx_{images[it]}.npy", pxv_nonan)

    df = {}
    df["x"] = x
    df["y"] = y
    df["lon"] = pxv_nonan[:, 0]
    df["lat"] = pxv_nonan[:, 1]
    df["inc"] = pxv_nonan[:, 2]
    df["emi"] = pxv_nonan[:, 3]
    df["pha"] = pxv_nonan[:, 4]
    df["temp"] = pxv_nonan[:, 5]
    df["rad"] = pxv_nonan[:, 6]
    df["R"] = pxv_nonan[:, 7]
    df["nx"] = pxv_nonan[:, 8]
    df["ny"] = pxv_nonan[:, 9]
    df["nz"] = pxv_nonan[:, 10]
    df = pandas.DataFrame(df)
    df.to_csv(path_out / f"campx_{images[it]}.csv", index=False, encoding="utf-8-sig")

    dpi = 100

    for ii in [2, 3, 4, 5, 6, 7]:
        fig = pyplot.figure(figsize=(1024 / dpi, 768 / dpi), frameon=False)
        ax = pyplot.gca()
        cmap = matplotlib.cm.gray
        cmap.set_bad(color="purple")
        im = pyplot.imshow(pxv[:, :, ii], cmap=cmap)
        pyplot.colorbar(im, ax=ax, label=labels[ii], fraction=0.035, pad=0.04)
        ax.invert_yaxis()
        pyplot.savefig(path_out / f"campx_preview_{images[it]}_{cols[ii]}.png", dpi=dpi)

        fig = pyplot.figure(figsize=(1024 / dpi, 768 / dpi), frameon=False)
        ax = pyplot.Axes(fig, [0.0, 0.0, 1.0, 1.0])
        ax.set_axis_off()
        fig.add_axes(ax)
        cmap = matplotlib.cm.gray
        cmap.set_bad(color="purple")
        im = pyplot.imshow(pxv[:, :, ii], cmap=cmap)
        ax.invert_yaxis()
        pyplot.savefig(path_out / f"campx_{images[it]}_{cols[ii]}.png", dpi=dpi)

    # pyplot.show()
