#!/usr/bin/env python3

import spiceypy as spice

mk = "/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm"
spice.furnsh(mk)

hera = -91

print("Formatting sc clk hex str")
scs_hex = "01C4576935FE90"
print(scs_hex)
print("--------****")
print("seconds  sub-seconds")
scs_sec = int(scs_hex[:8], 16)
scs_sub = int(scs_hex[8:12], 16)
scs_fmt = f"{scs_sec}:{scs_sub}"
print(f"-> {scs_fmt}")
print()

print("scs2et UTC")
et = spice.scs2e(hera, f"{scs_sec}.0")
utc = spice.et2utc(et, "c", 3)
print(f"{scs_sec:>8d}.{0:<5d} {et:17.7f} -> {utc}")

et = spice.scs2e(hera, scs_fmt)
utc = spice.et2utc(et, "c", 3)
utc2 = spice.timout(et, "YYYYMMDD_HRMNSC.#### ::RND")
print(f"{scs_fmt:>12} {et:17.7f} -> {utc} | {utc2}")
