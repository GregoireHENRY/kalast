#!/usr/bin/env python3

import spiceypy as spice


# As you wrote, the format of sc clk is defined as
# =====================================
# SCLK Format
#   The on-board clock, the conversion for which is provided by this SCLK
#   file, consists of two fields:
#          SSSSSSSSSS:FFFFF
#   where:
#          SSSSSSSSSS -- count of on-board seconds
#          FFFFF      -- count of fractions of a second with one fraction
#                        being 1/65536 of a second;
# =====================================
#
#
# The unit of FFFFF is 1/65536 (= 1/2^16).
# You have mentioned below.
# ==========================================
# Up until now I didnt really care about sub-seconds so I was using for the the file name UTC 272408.0 instead of  272408.11690367.
# But now trying to put in the fits header the UTC time accurately, I face this problem:
# - 272408.0 -> 20241010_104607
# - 272408.11690367 -> 20241010_104905
# ==========================================
# The 3 min difference will come from the subsecond format. 11690367 is converted to second by multiplying 1/65536, meaning 178.380844 sec. So 2min58s difference can occur. Therefore, FFFFF should be lower than 65536.
#
#
# Hera SC subsecond is given by 3 bytes.
# By using first 2 bytes, subsecond with the unit of 1/65536 sec can be obtained.
#
#
# Attached are my codes to convert clock to UTC, including subsecond.
#
# For example,
# 00042818B2617F
# => second: 00042818, subsecond: B261
# => formatted sc clk = 272408:45665
# => et = 781829236.898241
# => UTC = 2024-10-10T10:46:07.716
#
# If we use 00042818000000 (subsecond = 0)
# => formatted sc clk = 272408:00000
# => et = 781829236.201423
# => UTC = 2024-10-10T10:46:07.019
#
#
# For validation, AFC FITS header
# '1/0013495781.01367' / SPICE Sc Clk SPICETIM=
# '2025-03-12T12:03:31.925' / SPICE Time
# is checked.
# 1/0013495781.01367 is 00CDEDE5055700 in HEX format, the output from my code is 2025-03-12T12:03:31.925. consistent with AFC header.

print("Spacecraft clock")
print()

mk = "/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm"
spice.furnsh(mk)
print(f"Loaded {mk}")
print()

hera = -91

print("Partitions")
parts = spice.scpart(hera)
print(f"Starts: {parts[0]}")
print(f"Stops:  {parts[1]}")
print()

print("Formatting sc clk hex str")
scs_hex = "00042818b2617f"
print(scs_hex)
print("--------****")
print("seconds  sub-seconds")
print("=42818   =b261")
scs_sec = int(scs_hex[:8], 16)
scs_sub = int(scs_hex[8:12], 16)
scs_fmt = f"{scs_sec}:{scs_sub}"
print(f"-> {scs_fmt}")

# 00042818b2617f
# --------****
# seconds  sub-seconds
# =42818   =b261
# -> 272408:45665
print()

print("scs2et UTC")
# scs = "272505.0"

scs = "0.0"
et = spice.scs2e(hera, scs)
utc = spice.et2utc(et, "c", 3)
print(f"{scs:>12} {et:17.7f} -> {utc}")

scs = "272408:00000"
et = spice.scs2e(hera, scs)
utc = spice.et2utc(et, "c", 3)
print(f"{scs:>12} {et:17.7f} -> {utc}")

scs = "272408:45665"
et = spice.scs2e(hera, scs)
utc = spice.et2utc(et, "c", 3)
print(f"{scs:>12} {et:17.7f} -> {utc}")

# scs2et UTC
#          0.0 781556818.4641840 -> 2024 OCT 07 07:05:49.282
# 272408:00000 781829236.2014228 -> 2024 OCT 10 10:46:07.019
# 272408:45665 781829236.8982410 -> 2024 OCT 10 10:46:07.716
print()

print("str2et")
s = "2027-10-10 10:00:00"
et = spice.str2et(s)
print(f"{s} -> {et}")

# str2et: 2027-10-10 10:00:00 to 876434469.1823479
# print()
