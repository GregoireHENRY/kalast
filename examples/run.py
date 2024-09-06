#!/usr/bin/env python3

from pathlib import Path
import argparse
import sys

# import subprocess


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Helper to run Python examples.")

    # Name of an example folder containing a main.py file.
    parser.add_argument("name", type=str)

    args = parser.parse_args()

    path = Path(f"examples/{args.name}")
    path_main = path / "main.py"

    if not path_main.exists():
        raise ValueError(f"Path {path} does not exist.")

    sys.path.append(".")
    sys.path.append(path.absolute().__str__())

    # execute the script as it is just written like a quick script directly without if __name__ or function.
    import main  # noqa: F401
