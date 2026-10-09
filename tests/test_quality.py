#!/usr/bin/env python
"""`config.quality` sets the shadow settings for one use, and reads back the
name the settings are while they still are that preset's.

No window: the settings are plain config.
"""

import sys

from kalast.app import App

failures: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def main() -> int:
    app = App()
    c = app.simulation.config
    check("the defaults are 'point'", c.quality == "point", f"{c.quality!r}")
    want = {
        "quick": (True, False, False, 1, 2048),
        "fast": (True, True, True, 2, 4096),
        "point": (True, True, False, 2, 4096),
        "accurate": (False, True, False, 2, 4096),
    }
    c.shadows.cascades = 3
    for name, (point, per_body, cache, pcf, resolution) in want.items():
        c.quality = name
        got = (c.light.sun_as_point, c.shadows.per_body, c.shadows.cache, c.shadows.pcf, c.shadows.resolution)
        check(f"'{name}' sets the Sun, the layers, cache, PCF and resolution, cascades off",
              got == (point, per_body, cache, pcf, resolution) and c.shadows.cascades == 0, f"{got}, cascades {c.shadows.cascades}")
        check(f"and reads back '{name}'", c.quality == name, f"{c.quality!r}")
    c.quality = "accurate"
    check("'accurate' has the second depth layer and the near layer", c.shadows.second_depth and c.shadows.near_layer)
    c.shadows.pcf = 4
    check("changed by hand, it is none", c.quality is None, f"{c.quality!r}")
    try:
        c.quality = "best"
        check("an unknown name is refused", False)
    except ValueError as e:
        check("an unknown name is refused", "quick" in str(e), str(e))
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
