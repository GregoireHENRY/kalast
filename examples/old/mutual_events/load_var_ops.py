#!/usr/bin/env python

import spiceypy as spice

import kalast


# BODY2065803_RADII

spice.kclear()

spice.ldpool("/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm")

(n, _) = spice.dtpool("KERNELS_TO_LOAD")
kernels = spice.gcpool("KERNELS_TO_LOAD", 0, n, 80)

for kernel in kernels:
    print(kernel)

spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm")

print()
print(kalast.entity.DIDYMOS)
print(spice.bodn2c("DIDYMOS"))

print(spice.bodvcd(-658030, "RADII", 3))
# print(spice.gcpool("BODY-658030_RADII", 0, 1, 80))

print()
print(f"frames: {len(spice.kplfrm(-1))}")
for type_ in range(1, 7):
    ids = spice.kplfrm(type_)
    for id_ in ids:
        print(f"{type_} {id_} {spice.frmnam(id_, 33)}")
