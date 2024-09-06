from pathlib import Path

import scipy.io
import matplotlib
from matplotlib import pyplot

import kalast
import read_kalast
import util


path_run = read_kalast.path_runs / "sphere/lowres"
cfg = read_kalast.Config(path=path_run)
d = cfg.read()

path_mat = Path(
    "/Users/gregoireh/projects/kalast-utils/roughness-kuehrt/out/3/obs45.mat"
)
mat = scipy.io.loadmat(path_mat)

pyplot.plot.style.use("plot/main.mplstyle")


d["sph"] = d["A"]["sph"] * util.DPR
d["method"] = "mesh"
d["method_opt"] = None
d["path_mesh"] = "/Users/gregoireh/data/mesh/sphere.obj"
d["threshold_longitude_check"] = 300
d["show_limit_on_cbar"] = False

d["orientation"] = "horizontal"
d["cloc"] = 10
d["level"] = 1

d["vmin"] = 0
d["cmin"] = 0

d["save"] = True
d["show"] = False

# inc emi ppha
# for dataname in ("R",):
for dataname in ("inc", "emi", "ppha", "R", "f", "fc", "fr"):
    d["data"] = mat[dataname].reshape(-1) * util.DPR
    d["vmax"] = None
    d["cnorm"] = None
    d["cmap"] = matplotlib.cm.cividis

    if dataname == "inc":
        d["clabel"] = "Incidence angle (°)"
        d["vmax"] = 90
        d["cmap"] = matplotlib.cm.cividis_r
    elif dataname == "emi":
        d["clabel"] = "Emission angle (°)"
        d["vmax"] = 90
        d["cmap"] = matplotlib.cm.cividis_r
    elif dataname == "ppha":
        d["clabel"] = "Projected phase angle (°)"
        d["vmax"] = 180
        d["cmap"] = matplotlib.cm.cividis_r
    elif dataname == "R":
        d["clabel"] = "Roughness correction"
        # d["cnorm"] = "log"
    elif dataname == "f":
        d["clabel"] = "Radiance smooth (W/m^3/sr)"
    elif dataname == "fc":
        d["clabel"] = "Radiance crater (W/m^3/sr)"
    elif dataname == "fr":
        d["clabel"] = "Radiance rough (W/m^3/sr)"

    d["cmax"] = d["vmax"]
    d["show_axes_label"] = False
    d["figname"] = f"{dataname}.png"

    kalast.plot.smap.plot(d)
