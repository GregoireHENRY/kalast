#!/usr/bin/env python
"""Put `kalast.app` in a macOS bundle, so a double-click opens the UI app
without Terminal -- as `kalast.exe` does on Windows.

The Finder runs a bare executable in Terminal; an application bundle it
launches itself. This app is only a launcher: its executable is a shell
script that `exec`s the `kalast` beside it. So the bundle keeps its layout
-- `./kalast script.py` from a terminal, `python/`, `res/`, `examples/`, the
updater replacing it entry by entry -- and kalast then moves into its own
folder, as it does whenever it is started from elsewhere with nothing to
open (`bundle_working_dir`).

    python tools/macos_app.py <bundle-dir>

On macOS: the icon is made with `sips` and `iconutil`. The version comes from
`pyproject.toml`, the icon from `res/kalast-256.png`.
"""

import pathlib
import plistlib
import shutil
import subprocess
import sys
import tempfile
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent

# The launcher. Beside the app, or nothing: when macOS runs a download it has
# not been told to trust from a copy of the app alone (App Translocation),
# the folder around it is not there, and the README's `xattr -cr` is the
# cure -- said in a dialog, since a double-click has no terminal to say it in.
LAUNCHER = """#!/bin/sh
# kalast, beside this app: the app is its launcher, for a double-click.
here=$(cd "$(dirname "$0")/../../.." && pwd)
if [ ! -x "$here/kalast" ]; then
    /usr/bin/osascript -e 'display alert "kalast cannot start" message "kalast.app has to stay in its folder, beside kalast and python/. If macOS moved it to run it, run  xattr -cr  on the unpacked folder once, as the README says, and open it again."' >/dev/null 2>&1
    exit 1
fi
exec "$here/kalast" "$@"
"""

# The iconset's sizes, name -> pixels. Up to 256: the logo is 256 square, and
# macOS scales the largest it has for the bigger views.
SIZES = {
    "icon_16x16.png": 16,
    "icon_16x16@2x.png": 32,
    "icon_32x32.png": 32,
    "icon_32x32@2x.png": 64,
    "icon_128x128.png": 128,
    "icon_128x128@2x.png": 256,
    "icon_256x256.png": 256,
}


def version() -> str:
    with open(ROOT / "pyproject.toml", "rb") as f:
        return tomllib.load(f)["project"]["version"]


def icns(into: pathlib.Path) -> None:
    logo = ROOT / "res" / "kalast-256.png"
    with tempfile.TemporaryDirectory() as tmp:
        iconset = pathlib.Path(tmp) / "kalast.iconset"
        iconset.mkdir()
        for name, px in SIZES.items():
            subprocess.run(
                ["sips", "-z", str(px), str(px), str(logo), "--out", str(iconset / name)],
                check=True,
                capture_output=True,
            )
        subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o", str(into)], check=True)


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    bundle = pathlib.Path(sys.argv[1])
    if not (bundle / "kalast").is_file():
        print(f"{bundle} holds no kalast executable")
        return 1

    app = bundle / "kalast.app"
    if app.exists():
        shutil.rmtree(app)  # this tool's own output, from a previous run
    macos = app / "Contents" / "MacOS"
    resources = app / "Contents" / "Resources"
    macos.mkdir(parents=True)
    resources.mkdir(parents=True)

    launcher = macos / "kalast"
    launcher.write_text(LAUNCHER)
    launcher.chmod(0o755)
    icns(resources / "kalast.icns")

    v = version()
    with open(app / "Contents" / "Info.plist", "wb") as f:
        plistlib.dump(
            {
                "CFBundleName": "kalast",
                "CFBundleDisplayName": "kalast",
                "CFBundleIdentifier": "io.github.gregoirehenry.kalast",
                "CFBundleExecutable": "kalast",
                "CFBundleIconFile": "kalast",
                "CFBundlePackageType": "APPL",
                "CFBundleShortVersionString": v,
                "CFBundleVersion": v,
                "LSMinimumSystemVersion": "11.0",
                "NSHighResolutionCapable": True,
            },
            f,
        )
    print(f"{app}: v{v}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
