#!/usr/bin/env python
"""Get the Hera SPICE kernel dataset, and make its meta-kernels loadable.

    python tools/hera_data.py ~/data/spice/hera --download
    python tools/hera_data.py ~/data/spice/hera            # already unpacked

`--download` fetches https://spiftp.esac.esa.int/data/SPICE/HERA/misc/skd/HERA.zip
(about 1.1 GB) and unpacks it into the directory given. With or without it,
every `kernels/mk/*.tm` then gets a `*_local.tm` twin whose `PATH_VALUES`
is the absolute path of `kernels/` here -- the pristine files say `'..'`,
which SPICE resolves against the *working directory*, not the file, so a
`furnsh` from anywhere else fails on the first kernel it names. The
examples load the `_local` twins. Finally it prints the line to set
`KALAST_HERA`, which is how the examples find all of this.

Standard library only, so it runs on any Python.
"""

import argparse
import pathlib
import re
import shutil
import sys
import urllib.request
import zipfile

URL = "https://spiftp.esac.esa.int/data/SPICE/HERA/misc/skd/HERA.zip"


def download(root: pathlib.Path) -> None:
    root.mkdir(parents=True, exist_ok=True)
    archive = root / "HERA.zip"
    with urllib.request.urlopen(URL) as r, open(archive, "wb") as out:
        total = int(r.headers.get("Content-Length") or 0)
        got = 0
        while chunk := r.read(1 << 20):
            out.write(chunk)
            got += len(chunk)
            if total:
                print(f"\r  {got / 1e6:7.0f} / {total / 1e6:.0f} MB", end="", flush=True)
    print()
    print(f"  unpacking {archive.name} into {root}")
    with zipfile.ZipFile(archive) as z:
        z.extractall(root)
    archive.unlink()


def dataset_root(root: pathlib.Path) -> pathlib.Path:
    """`root` itself, or the one subdirectory the archive unpacked into."""
    if (root / "kernels").is_dir():
        return root
    inner = [d for d in root.iterdir() if d.is_dir() and (d / "kernels").is_dir()]
    if len(inner) == 1:
        return inner[0]
    sys.exit(f"{root} has no kernels/ directory; is this where HERA.zip was unpacked?")


def localise(root: pathlib.Path) -> list[pathlib.Path]:
    kernels = (root / "kernels").resolve().as_posix()  # forward slashes work on Windows too
    written = []
    for tm in sorted((root / "kernels" / "mk").glob("*.tm")):
        if tm.stem.endswith("_local"):
            continue
        text = tm.read_text()
        new, n = re.subn(r"PATH_VALUES(\s*)=(\s*)\(\s*'[^']*'\s*\)",
                         lambda m: f"PATH_VALUES{m.group(1)}={m.group(2)}( '{kernels}' )", text, count=1)
        if n != 1:
            print(f"  skipped {tm.name}: no PATH_VALUES line")
            continue
        local = tm.with_name(tm.stem + "_local.tm")
        local.write_text(new)
        written.append(local)
    return written


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("root", type=pathlib.Path, help="where HERA.zip is, or is to be, unpacked")
    ap.add_argument("--download", action="store_true", help="fetch HERA.zip first (about 1.1 GB)")
    args = ap.parse_args()
    root = args.root.expanduser()
    if args.download:
        download(root)
    root = dataset_root(root)
    written = localise(root)
    print(f"  {len(written)} meta-kernels localised under {root / 'kernels' / 'mk'}:")
    for w in written:
        print(f"    {w.name}")
    print()
    print("Now tell the examples where this is:")
    print(f"    export KALAST_HERA={root.resolve()}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
