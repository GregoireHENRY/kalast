from pathlib import Path

import glm
import numpy

import kalast

if __name__ == "__main__":
    path = Path("../assets/records/didymos (new).parquet")
    record = kalast.Record.read(path)
