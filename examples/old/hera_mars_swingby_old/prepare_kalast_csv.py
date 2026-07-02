#!/usr/bin/env python

import numpy
import spiceypy as spice

import kalast

spice.kclear()

spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm")

et = 795053385.2
image = "20250312_120836_31"

(p, _lt) = spice.spkpos("deimos", et, "hera_tiri", "none", "hera_tiri")
print(p)

m = spice.pxform("iau_deimos", "hera_tiri", et)
print(m)

(p, _lt) = spice.spkpos("sun", et, "hera_tiri", "none", "hera_tiri")
print(p)
