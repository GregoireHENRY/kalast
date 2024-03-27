from pathlib import Path

import numpy
from natsort import natsorted
from pyarrow import csv

import plot

d = dict()

path_data = Path("/Users/gregoireh/data/kalast")
path_desktop = Path("/Users/gregoireh/Desktop/kalast-runs")
path_runs = path_data / "runs"
path_draft = path_data / "graphs/draft"

# path_run = path_runs / "Apophis flyby Hakan"
path_run = path_desktop / "run #29"

path_cfg = path_run / "cfg/cfg.toml"
path_simu = path_run / "simu"
path_rec = path_simu / "rec"

d["nf"] = 1
d["body"] = "Simple"
path_setup_body = path_simu / f"{d['body']}"
path_progress = path_simu / "progress.csv"

path_sph = path_setup_body / "mesh.csv"
read_options = csv.ReadOptions(
    column_names=["x", "y", "z", "lon", "lat", "rad"], skip_rows=1
)
tab = csv.read_csv(path_sph, read_options)
df = tab.to_pandas()
d["centers"] = numpy.array(
    [df["x"].to_numpy(), df["y"].to_numpy(), df["z"].to_numpy()]
).T
d["sph"] = numpy.array(
    [df["lon"].to_numpy(), df["lat"].to_numpy(), df["rad"].to_numpy()]
).T

path_depth = path_setup_body / "depth.csv"
read_options = csv.ReadOptions(column_names=["depth"], skip_rows=1)
tab = csv.read_csv(path_depth, read_options)
df = tab.to_pandas()
d["depth"] = df["depth"].to_numpy()
d["nz"] = d["depth"].size

list_path_date = [p for p in natsorted((path_rec).glob("*"), key=str) if p.is_dir()]
list_elapsed = [int(p.name) for p in list_path_date]
it_list_path_date = iter(list_path_date)

path_date = next(it_list_path_date)

p_csv = path_date / d["body"] / "temperatures/temperatures-all.csv"
read_options = csv.ReadOptions()
tab = csv.read_csv(p_csv, read_options)
df = tab.to_pandas()
d["tmp-all"] = df["tmp"].array.reshape((d["nf"], -1)).T
print(d["tmp-all"].shape)
print(d["tmp-all"][0].min(), d["tmp-all"][0].max(), d["tmp-all"][0].mean())

p_csv = path_date / d["body"] / "temperatures/temperatures-columns.csv"
read_options = csv.ReadOptions()
tab = csv.read_csv(p_csv, read_options)
df = tab.to_pandas()
# d["tmp-cols"] = df["0"].array.reshape((d["nz"], -1)).T
d["tmp-cols"] = df["0"].array.reshape((-1, d["nz"]))
print(d["tmp-cols"].shape)

# plot.smap.plot(d, save=True)
plot.daily.plot(d, save=True)
plot.depth.plot(d, save=True)