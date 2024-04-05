import numpy
import read_kalast

import plot

path_root = read_kalast.path_data / "validation/heat1d"
path_run = path_root / "run #3"
cfg = read_kalast.Config(path=path_run)
d = cfg.read()

path_run = path_root / "heat1d.csv"
d2 = read_kalast.read_tmp_cols(path_run, nz=d["nz"])

# path_run = path_root / "multiheats.csv"
# d3 = read_kalast.read_tmp_cols(path_run, nz=d["nz"])

path_run = path_root / "schorghofer.csv"
d4 = read_kalast.read_tmp_cols(path_run, nz=110)

nt = 865
x = numpy.linspace(0, cfg.data["simulation"]["export"]["duration"], nt)
x /= 3600

x2 = numpy.linspace(0, cfg.data["simulation"]["export"]["duration"] - 200, nt - 1)
x2 /= 3600

path_run = path_root / "schorghofer_depth.csv"
x3 = read_kalast.read_csv_arr(path_run, no_header=True)

data = [
    plot.tool.Data(x=x, y=d["tmp-cols"][0][:, 0], label="kalast"),
    plot.tool.Data(x=x2, y=d2[0][:, 0], label="heat1d", ls="dashed"),
    # plot.daily.Data(x=x2, y=d3[0][:, 0], label="multiheats", ls="dotted"),
    plot.tool.Data(x=x2, y=d4[0][:, 0], label="schorghofer", ls="dotted"),
]

cfg_daily = plot.tool.Config(data=data)
cfg_daily.xax.label = "Hours elapsed"
cfg_daily.xax.lim = (0, x.max())
cfg_daily.yax.label = "Temperature (K)"
cfg_daily.yax.lim = (200, 400)
cfg_daily.yax.loc = 50
cfg_daily.name = "daily"
cfg_daily.show = False

plot.tool.plot(cfg_daily)

y = d["depth"] * 100
y2 = x3 * 100

data = [
    plot.tool.Data(x=d["tmp-cols"][0].mean(axis=0), y=y, label="kalast"),
    plot.tool.Data(x=d["tmp-cols"][0].min(axis=0), y=y),
    plot.tool.Data(x=d["tmp-cols"][0].max(axis=0), y=y),
    plot.tool.Data(x=d2[0].mean(axis=0), y=y, label="heat1d", ls="dashed"),
    plot.tool.Data(x=d2[0].min(axis=0), y=y, ls="dashed"),
    plot.tool.Data(x=d2[0].max(axis=0), y=y, ls="dashed"),
    plot.tool.Data(x=d4[0].mean(axis=0), y=y2, label="schorghofer", ls="dotted"),
    plot.tool.Data(x=d4[0].min(axis=0), y=y2, ls="dotted"),
    plot.tool.Data(x=d4[0].max(axis=0), y=y2, ls="dotted"),
]

cfg_depth = plot.tool.Config(data=data)
cfg_depth.xax.label = "Temperature (K)"
cfg_depth.xax.lim = (200, 400)
cfg_depth.xax.loc = 50
cfg_depth.yax.label = "Depth (cm)"
cfg_depth.yax.lim = (60.0, 0.0)
cfg_depth.yax.loc = 20
cfg_depth.name = "depth"
cfg_depth.show = True

plot.tool.plot(cfg_depth)