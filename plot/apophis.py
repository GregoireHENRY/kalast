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
path_run = path_desktop / "apophis thermal/run thermal #2"

path_cfg = path_run / "cfg/cfg.toml"
path_simu = path_run / "simu"
path_rec = path_simu / "rec"

d["body"] = "Apophis"
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
d["nf"] = d["centers"].shape[0]

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
print(
    "TMP all: ",
    d["tmp-all"][:, 0].min(),
    d["tmp-all"][:, 0].max(),
    d["tmp-all"][:, 0].mean(),
    d["tmp-all"].shape,
)

p_csv = path_date / d["body"] / "temperatures/temperatures-rows.csv"
if p_csv.exists():
    read_options = csv.ReadOptions()
    tab = csv.read_csv(p_csv, read_options)
    df = tab.to_pandas()
    if len(df.keys()) > 0:
        d["tmp-rows"] = dict()
    for key in df.keys():
        t = df[key].array.reshape((-1, d["nf"]))
        key = int(key)
        d["tmp-rows"][key] = t
        print(f"TMP rows #{key}: {t.min()} {t.max()} {t.mean()} {t.shape}")
        if key == "0":
            d["tmp-surf"] = dict()
            d["tmp-surf"]["min"] = t.min()
            d["tmp-surf"]["min"] = t.min()
            d["tmp-surf"]["max"] = t.max()
            d["tmp-surf"]["mean"] = t.mean()

p_csv = path_date / d["body"] / "temperatures/temperatures-columns.csv"
if p_csv.exists():
    read_options = csv.ReadOptions()
    tab = csv.read_csv(p_csv, read_options)
    df = tab.to_pandas()
    if len(df.keys()) > 0:
        d["tmp-cols"] = dict()
    for key in df.keys():
        t = df[key].array.reshape((-1, d["nz"]))
        key = int(key)
        d["tmp-cols"][key] = t
        print(f"TMP cols #{key}: {t.min()} {t.max()} {t.mean()} {t.shape}")

plot.smap.plot(d, save=True)

if any([key in d.keys() for key in ["tmp-cols", "tmp-rows"]]):
    # plot.daily.plot(d, save=True)
    pass

if "tmp-cols" in d.keys():
    # plot.depth.plot(d, save=True)
    pass