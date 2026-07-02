#!/usr/bin/env python

import spiceypy as spice


spice.kclear()

spice.ldpool("/Users/gregoireh/data/spice/hera_pre/mk/hera_study_v000.tm")

(n, _) = spice.dtpool("KERNELS_TO_LOAD")
kernels = spice.gcpool("KERNELS_TO_LOAD", 0, n, 80)

for kernel in kernels:
    print(kernel)
    
# BODY2065803_RADII


# spice.furnsh("/Users/gregoireh/data/spice/hera_pre/mk/hera_study_v000.tm")