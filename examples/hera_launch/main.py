#!/usr/bin/env python3

from pathlib import Path

import pyarrow
import pyarrow.csv
import spiceypy as spice
import numpy

import kalast

spice.kclear()

mks = ["/Users/gregoireh/data/spice/hera/kernels/mk/hera_crema_2_1_LPO_ECP_PDP_DCP.tm"]
for mk in mks:
    spice.furnsh(mk)


# in kalast.util
# LPO_LAUNCH="2024-10-07 16:27:53.417",
date0 = "2024-10-07 16:27:53.417"

et0 = spice.str2et(date0)
print(f"et0: {et0}")

dt = 300
N = 10000

dur = N * dt
days = dur / kalast.util.day
print(f"Days covered: {days}")

bodies = [kalast.util.earth, kalast.util.moon]

# bodies = [
#     {"name": "MARS", "frame": "IAU_MARS"},
#     {"name": "PHOBOS", "frame": "IAU_PHOBOS"},
#     {"name": "DEIMOS", "frame": "IAU_PHOBOS"},
# ]

for ii in range(0, N):
    if ii == 1:
        break

    et = et0 + dt
    # print(f"et: {et}")

    date = spice.timout(et, "YYYY-MM-DD HR:MN:SC.### ::RND ::UTC")
    # print(f"date: {date}")

    for body in bodies:
        (pos, _lt) = spice.spkpos(body["name"], et, "HERA_SPACECRAFT", "NONE", "HERA")
        d = numpy.linalg.norm(pos)
        # print(f"spkpos: {pos}, {_lt}, {d}")

        (sp, et_sp, vec_sp) = spice.subpnt(
            "NEAR POINT/ELLIPSOID", body["name"], et, body["frame"], "NONE", "HERA"
        )
        h = numpy.linalg.norm(sp)
        # print(f"subpnt: {sp}, {et_sp}, {vec_sp}, {h}")

        (lo, la, _alt) = spice.recpgr(
            body["name"], sp, body["radii"][0], body["flattening"]
        )
        # print(f"recpgr: {lo}, {la}, {_alt}")

        (_, _, pha, inc, emi, _vis, _lit) = spice.illumf(
            "ELLIPSOID", body["name"], "SUN", et, body["frame"], "NONE", "HERA", sp
        )
        # print(f"illumf: {pha}, {inc}, {emi}, {_vis} {_lit}")

        projx = d * numpy.atan(kalast.util.tiri["fovx"])
        projy = d * numpy.atan(kalast.util.tiri["fovy"])

        resx = projx / kalast.util.tiri["pxx"]
        resy = projy / kalast.util.tiri["pxy"]

        area_px = resx * resy
        visible_area_targ = numpy.pi * body["radii"].mean() ** (1 / 2)
        npx_cov = numpy.clip(
            numpy.floor(visible_area_targ / area_px), 0, kalast.util.tiri["npx"]
        )
        cov = (npx_cov / kalast.util.tiri["npx"]) * 100.0
        print(
            f"proj/res: {projx}, {projy}, {resx}, {resy} {area_px} {visible_area_targ} {npx_cov} {cov}"
        )


spice.kclear()
