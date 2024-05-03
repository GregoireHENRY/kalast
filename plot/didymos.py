import numpy
import matplotlib
import read_kalast

import plot

path_root = read_kalast.path_runs / "didymos thermal"
path_run = path_root / "mutual"
cfg = read_kalast.Config(path=path_run)
d = cfg.read()

nt = 7154
x = numpy.linspace(0, cfg.data["simulation"]["export"]["duration"], nt) / 3600

# d["Didymos"]["sph"][:, 1].argmax()
#
# ii = numpy.where(numpy.abs(numpy.pi * 0 - d["Didymos"]["sph"][:, 0]) < 0.1)[0]
# jj = numpy.abs(d["Didymos"]["sph"][ii, 1]).argmin()
# kk = ii[jj]
# print(kk, d["Didymos"]["sph"][kk])
#
# faces = util.find_closest(d["Didymos"]["sph"], refv1=0.0, i1=0, threshold=0.1, refv2=0.45, i2=1, N=5)
# for ii, face in faces: print(ii, face)
#
# path = read_kalast.path_data  / "didymos-surface.csv"
# read_kalast.write_csv(path, ["tmp-surf"], [d["Didymos"]["tmp-rows"][0][0],], no_header=True)
facets = [
    25533,
    17341,
    9917,
    34621,
    25360,
    17159,
    9995,
    34579,
    6168,
    2214,
    3299,
    7123,
    4796,
    25453,
    17395,
    9967,
    34664,
    47208,
    43098,
    43930,
    48041,
    45634,
]
N = len(facets)

cmap = matplotlib.colormaps["cividis"]

data = [
    plot.tool.Data(x=x, y=d["Dimorphos"]["tmp-rows"][0][:, 25533]),
    # plot.tool.Data(
    #     x=x,
    #     y=d["Dimorphos"]["tmp-rows"][0][:, facet],
    #     color=cmap.colors[int(ii / (N - 1) * 255)],
    # )
    # for ii, facet in enumerate(facets)
]

cfg_daily = plot.tool.Config(data=data)
cfg_daily.xax.label = "Hours elapsed"
cfg_daily.xax.lim = (0, x.max())
cfg_daily.yax.label = "Temperature (K)"
cfg_daily.yax.lim = (140, 360)
cfg_daily.yax.loc = 20
cfg_daily.name = "daily"
cfg_daily.legend.show = False
cfg_daily.show = False

plot.tool.plot(cfg_daily)