from dataclasses import dataclass
from pathlib import Path

import numpy
import tomllib
from natsort import natsorted
from pyarrow import csv

Out = dict[str, numpy.array]

path_data = Path("/Users/gregoireh/data/kalast")
path_desktop = Path("/Users/gregoireh/Desktop/kalast-runs")
path_runs = path_data / "runs"
path_draft = path_data / "graphs/draft"


def read_csv(path: Path, no_header: bool = False) -> numpy.ndarray | None:
    if path.exists():
        opt = None
        if no_header:
            opt = csv.ReadOptions(autogenerate_column_names=True)
        tab = csv.read_csv(path, opt)
        return tab.to_pandas()


def read_csv_arr(path: Path, no_header: bool = False) -> numpy.ndarray | None:
    d = read_csv(path, no_header)
    if d is not None:
        d = d.to_numpy()
    return d


def read_tmp(path: Path, no_header: bool = False) -> dict[int, numpy.array]:
    df = read_csv(path, no_header)
    if df is not None:
        d = dict()
        for key in df.keys():
            t = df[key].array
            print(f"key: {key}")
            key = int(key)
            d[key] = t
            # print(f"TMP surf #{key}: {t.min()} {t.max()} {t.mean()} {t.shape}")
        return d
    return df


def read_tmp_cols(
    path: Path, no_header: bool = False, nz: int = None
) -> dict[int, numpy.array]:
    d = read_tmp(path, no_header)
    for key in d.keys():
        if nz is not None:
            d[key] = d[key].reshape((-1, nz))
            # print(f"TMP cols #{key}: {t.min()} {t.max()} {t.mean()} {t.shape}")
    return d


@dataclass
class Config:
    path: Path = None
    data: dict = None

    def read_cfg(self):
        if self.path is None:
            raise Exception("Config path not set.")

        with open(self.path / "cfg/full.toml", "rb") as f:
            self.data = tomllib.load(f)

    def read(self) -> Out:
        self.read_cfg()
        d = dict()

        path_simu = self.path / "simu"
        path_rec = path_simu / "rec"

        # d["body"] = "Apophis"
        d["body"] = self.data["bodies"][0]["name"]
        path_setup_body = path_simu / f"{d['body']}"
        # path_progress = path_simu / "progress.csv"

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

        print(path_rec)
        list_path_date = [
            p for p in natsorted((path_rec).glob("*"), key=str) if p.is_dir()
        ]
        it_list_path_date = iter(list_path_date)
        # list_elapsed = [int(p.name) for p in list_path_date]

        path_date = next(it_list_path_date)

        p_csv = path_date / d["body"] / "temperatures/temperatures-all.csv"
        read_options = csv.ReadOptions()
        tab = csv.read_csv(p_csv, read_options)
        df = tab.to_pandas()
        d["tmp-all"] = df["tmp"].array.reshape((d["nf"], -1)).T
        print(
            "TMP all: ",
            d["tmp-all"][0, :].min(),
            d["tmp-all"][0, :].max(),
            d["tmp-all"][0, :].mean(),
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

        return d