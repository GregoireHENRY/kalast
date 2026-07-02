#!/usr/bin/env python

import pandas
import spiceypy as spice

import kalast

import setup


spice.kclear()

# spice.furnsh("/Users/gregoireh/data/spice/hera_pre/mk/hera_study_v000.tm")
# spice.furnsh("/Users/gregoireh/data/spice/hera_more/hera_didymos.tpc")

# spice.furnsh("/Users/gregoireh/data/spice/hera_more/hera_ops_pre.tm")

# spice.furnsh("/Users/gregoireh/data/spice/dart_more/d520_v03_pre.tm")

# spice.furnsh("/Users/gregoireh/data/spice/dart_pre/mk/kernels_soc_pre.tm")
# spice.furnsh("/Users/gregoireh/data/spice/dart/lsk/naif0012.tls")
# spice.furnsh("/Users/gregoireh/data/spice/dart/pck/didymos_system_15.tpc")
# spice.furnsh("/Users/gregoireh/data/spice/dart/fk/didymos_system_008.tf")

spice.furnsh("/Users/gregoireh/data/spice/dart_pre/mk/current_2022200T00.tm")

# et_start = spice.str2et("2022-12-14 08:00:00")
# et_end = spice.str2et("2022-12-15 08:00:00")

# pre-impact (with dart d520_v03)
# 10:37 - 22:32 -> 11:55
et_start = spice.str2et("2022-08-01 08:00:00")
et_end = spice.str2et("2022-08-02 08:00:00")

# post-impact (with dart d520_v03)
# 1h20 primary eclipse
# 11:16 - 22:38 -> 11:22
# et_start = spice.str2et("2022-12-15 08:00:00")
# et_end = spice.str2et("2022-12-16 08:00:00")

# dart (with current_2022200T00)
# pre  10:33 - 22:28 -> 11:55
# post 08:00 - 19:35 -> 11:35

et = et_start
dt = 60

events = []

oc = 0
ec = 0

while True:
    date = spice.timout(et, kalast.util.SPICE_PICTUR_3)

    ocid = spice.occult(
        "didymos",
        "ellipsoid",
        "didymos_fixed",
        "dimorphos",
        "ellipsoid",
        "dimorphos_fixed",
        "none",
        "earth",
        et,
    )

    ecid = spice.occult(
        "didymos",
        "ellipsoid",
        "didymos_fixed",
        "dimorphos",
        "ellipsoid",
        "dimorphos_fixed",
        "none",
        "sun",
        et,
    )

    if ocid != oc:
        events.append([et, date, setup.OCID_NAME[ocid]])
        oc = ocid

    if ecid != ec:
        events.append([et, date, setup.ECID_NAME[ecid]])
        ec = ecid

    et += dt

    if et > et_end:
        break

spice.kclear()

df = {}
df["et"] = [et for (et, _, _) in events]
df["date"] = [date for (_, date, _) in events]
df["event"] = [event for (_, _, event) in events]
df = pandas.DataFrame(df)
df.to_csv("out/events_spice.csv", index=False, encoding="utf-8-sig")
