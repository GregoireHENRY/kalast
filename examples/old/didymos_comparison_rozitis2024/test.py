#!/usr/bin/env python

import spiceypy as spice


spice.kclear()
spice.furnsh("/Users/gregoireh/data/spice/dart/mk/d520_v03.tm")
# spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm")

etstr = "2022-01-01 00:00"
et = spice.str2et(etstr)

(sun, _lt) = spice.spkpos("SUN", et, "DIDYMOS_FIXED", "NONE", "DIDYMOS")
print(sun)

(d1, _lt) = spice.spkpos("DIMORPHOS", et, "DIDYMOS_FIXED", "NONE", "DIDYMOS")
print(d1)
