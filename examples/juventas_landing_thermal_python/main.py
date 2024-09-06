#!/usr/bin/env python3

from pathlib import Path

import pyarrow
import pyarrow.csv

import kalast


tmp = kalast.io.read_binary.read_binary_temperature(
    "/Users/gregoireh/data/kalast/runs/dimorphos-daily/2027-06-04T00_00_00/temperature-surface",
    (2160, 3072),  # 1997 for didymos
)

tab = pyarrow.table(
    [
        tmp[:, 783],
    ],
    names=[
        "tmp-surf-783",
    ],
)

path = Path("out") / "new.csv"
pyarrow.csv.write_csv(tab, path)
