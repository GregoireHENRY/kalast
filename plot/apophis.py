import matplotlib
import numpy
import read_kalast
import util
from matplotlib import pyplot

import plot

path_root = read_kalast.path_runs / "Apophis flyby Hakan/tumbling"

path_run = path_root / "run #3 front"
cfg = read_kalast.Config(path=path_run)
d = cfg.read()

nt = 5041
x = numpy.linspace(0, cfg.data["simulation"]["export"]["duration"], nt)
x /= 3600

cols = list(d["tmp-cols"].keys())
depth_cm = d["depth"] * 100

# p = d["sph"] * util.DPR
# xy = p[:, :2]
# z = d["tmp-all"][0]

data = [
    plot.tool.Data(x=x, y=d["tmp-cols"][cols[0]][:, 0], label="#1", color="#a83232"),
    plot.tool.Data(x=x, y=d["tmp-cols"][cols[1]][:, 0], label="#2", color="#a88532"),
    plot.tool.Data(x=x, y=d["tmp-cols"][cols[2]][:, 0], label="#3", color="#8ba832"),
    plot.tool.Data(x=x, y=d["tmp-cols"][cols[3]][:, 0], label="#4", color="#32a863"),
    plot.tool.Data(x=x, y=d["tmp-cols"][cols[4]][:, 0], label="#5", color="#3296a8"),
    plot.tool.Data(x=x, y=d["tmp-cols"][cols[5]][:, 0], label="#6", color="#3632a8"),
    plot.tool.Data(x=x, y=d["tmp-cols"][cols[6]][:, 0], label="#7", color="#9432a8"),
]

cfgp = plot.tool.Config(data=data)
cfgp.xax.label = "Hours elapsed (h)"
# cfgp.xax.lim = (0, x.max())
# cfgp.xax.lim = (400, 500)
cfgp.xax.lim = (200, 300)
# cfgp.xax.loc = 30

cfgp.yax.label = "Temperature (K)"
# cfgp.yax.lim = (0, 400)
cfgp.yax.lim = (150, 350)
cfgp.yax.loc = 50

# cfgp.show = True
cfgp.grid = True
cfgp.name = "daily"
plot.tool.plot(cfgp)


data = [
    plot.tool.Data(x=x, y=d["tmp-cols"][cols[0]][:, 0], color="k"),
    plot.tool.Data(x=x, y=d["tmp-cols"][cols[0]][:, 1], color="k"),
    plot.tool.Data(x=x, y=d["tmp-cols"][cols[0]][:, 2], color="k"),
    plot.tool.Data(x=x, y=d["tmp-cols"][cols[0]][:, 3], color="k"),
    plot.tool.Data(x=x, y=d["tmp-cols"][cols[0]][:, 4], color="k"),
    plot.tool.Data(x=x, y=d["tmp-cols"][cols[0]][:, 5], color="k"),
    plot.tool.Data(x=x, y=d["tmp-cols"][cols[0]][:, 6], color="k"),
]

cfgp = plot.tool.Config(data=data)
cfgp.xax.label = "Hours elapsed (h)"
# cfgp.xax.lim = (0, x.max())
# cfgp.xax.lim = (400, 500)
cfgp.xax.lim = (200, 300)
# cfgp.xax.loc = 30

cfgp.yax.label = "Temperature (K)"
# cfgp.yax.lim = (0, 400)
cfgp.yax.lim = (200, 350)
cfgp.yax.loc = 50

# cfgp.show = True
cfgp.grid = True
cfgp.name = "daily_depth"
plot.tool.plot(cfgp)


data = [
    plot.tool.Data(
        x=d["tmp-cols"][cols[0]].min(axis=0), y=depth_cm, color="#a83232", label="#1"
    ),
    plot.tool.Data(x=d["tmp-cols"][cols[0]].mean(axis=0), y=depth_cm, color="#a83232"),
    plot.tool.Data(x=d["tmp-cols"][cols[0]].max(axis=0), y=depth_cm, color="#a83232"),
    plot.tool.Data(
        x=d["tmp-cols"][cols[1]].min(axis=0), y=depth_cm, color="#a88532", label="#2"
    ),
    plot.tool.Data(x=d["tmp-cols"][cols[1]].mean(axis=0), y=depth_cm, color="#a88532"),
    plot.tool.Data(x=d["tmp-cols"][cols[1]].max(axis=0), y=depth_cm, color="#a88532"),
    plot.tool.Data(
        x=d["tmp-cols"][cols[2]].min(axis=0), y=depth_cm, color="#8ba832", label="#3"
    ),
    plot.tool.Data(x=d["tmp-cols"][cols[2]].mean(axis=0), y=depth_cm, color="#8ba832"),
    plot.tool.Data(x=d["tmp-cols"][cols[2]].max(axis=0), y=depth_cm, color="#8ba832"),
    plot.tool.Data(
        x=d["tmp-cols"][cols[3]].min(axis=0), y=depth_cm, color="#32a863", label="#4"
    ),
    plot.tool.Data(x=d["tmp-cols"][cols[3]].mean(axis=0), y=depth_cm, color="#32a863"),
    plot.tool.Data(x=d["tmp-cols"][cols[3]].max(axis=0), y=depth_cm, color="#32a863"),
    plot.tool.Data(
        x=d["tmp-cols"][cols[4]].min(axis=0), y=depth_cm, color="#3296a8", label="#5"
    ),
    plot.tool.Data(x=d["tmp-cols"][cols[4]].mean(axis=0), y=depth_cm, color="#3296a8"),
    plot.tool.Data(x=d["tmp-cols"][cols[4]].max(axis=0), y=depth_cm, color="#3296a8"),
    plot.tool.Data(
        x=d["tmp-cols"][cols[5]].min(axis=0), y=depth_cm, color="#3632a8", label="#6"
    ),
    plot.tool.Data(x=d["tmp-cols"][cols[5]].mean(axis=0), y=depth_cm, color="#3632a8"),
    plot.tool.Data(x=d["tmp-cols"][cols[5]].max(axis=0), y=depth_cm, color="#3632a8"),
    plot.tool.Data(
        x=d["tmp-cols"][cols[6]].min(axis=0), y=depth_cm, color="#9432a8", label="#7"
    ),
    plot.tool.Data(x=d["tmp-cols"][cols[6]].mean(axis=0), y=depth_cm, color="#9432a8"),
    plot.tool.Data(x=d["tmp-cols"][cols[6]].max(axis=0), y=depth_cm, color="#9432a8"),
]

cfgp = plot.tool.Config(data=data)
cfgp.xax.label = "Temperature (K)"
# cfgp.xax.lim = (0, 400)
cfgp.xax.lim = (150, 350)
cfgp.xax.loc = 50

cfgp.yax.label = "Depth (cm)"
cfgp.yax.lim = (60, 0)
cfgp.yax.loc = 20

# cfgp.show = True
cfgp.grid = True
cfgp.name = "depth_minmax"
plot.tool.plot(cfgp)


data = [
    plot.tool.Data(x=d["tmp-cols"][cols[0]][ii + 3000, :], y=depth_cm, color="k")
    for ii in range(0, 200, 20)
]

cfgp = plot.tool.Config(data=data)
cfgp.xax.label = "Temperature (K)"
# cfgp.xax.lim = (0, 400)
cfgp.xax.lim = (200, 350)
cfgp.xax.loc = 50

cfgp.yax.label = "Depth (cm)"
cfgp.yax.lim = (60, 0)
cfgp.yax.loc = 20

# cfgp.show = True
cfgp.grid = True
cfgp.name = "depth"
plot.tool.plot(cfgp)


data = [
    plot.tool.Data(
        x=d["sph"][:, 0] * util.DPR,
        y=d["sph"][:, 1] * util.DPR,
        z=d["tmp-all"][0],
        label="kalast",
    ),
    # plot.tool.Data(x=x, y=d["tmp-cols"][0][:, 0], label="kalast"),
]

cfg_map = plot.tool.Config(data=data)
# cfg_map.xax.label = "Longitude °"
cfg_map.xax.lim = (-180, 180)
cfg_map.xax.loc = 30

# cfg_map.yax.label = "Latitude °"
cfg_map.yax.lim = (-90, 90)
cfg_map.yax.loc = 30

cfg_map.map = plot.tool.Map()
cfg_map.map.ax.label = "Temperature (K)"
cfg_map.map.ax.lim = (0, 400)
cfg_map.map.ax.loc = 50
cfg_map.map.cmap = matplotlib.cm.inferno
cfg_map.map.nx = 50
cfg_map.map.ny = 50
cfg_map.map.vmin = data[0].z.min()
cfg_map.map.vmax = data[0].z.max()

# cfgp.grid = True
# cfgp.show = True
cfg_map.legend.show = False
cfg_map.name = "map"
plot.tool.plot(cfg_map)

pyplot.show()
