#!/usr/bin/env python

import numpy

import kalast

import setup


# nz = 100
# nt = 16_000

nz = 1000
nt = 2_000_000

z = numpy.linspace(0.0, setup.L, nz, endpoint=True)
t = numpy.linspace(0, setup.tf, nt, endpoint=True)

dt = numpy.diff(t)
dt0 = dt[0]

dz = numpy.diff(z)
dz0 = dz[0]
twodz0 = 2 * dz0
dz02 = dz0 * dz0
dz2in = dz[:-1] * dz[:-1]
dtpdz2in = dt0 / dz2in

d0 = setup.d
d = numpy.ones(nz) * d0

maxdt = kalast.tpm.core.stability_maxdt(d0, dz02, s=0.5)
S = kalast.tpm.core.stability(d0, dt0, dz02)
if S > 0.5:
    raise ValueError("Stability criteria not valid.")

T0 = setup.f2(z, 300.0, 1.0)
T = T0.copy()
Te = numpy.zeros((setup.ne, nz))
iie_ = 0

for iit in range(0, nt):
    T[1:-1] = kalast.tpm.core.conduction_1d(T, d, dtpdz2in)
    T[0] = T[1]
    T[-1] = T[-2]

    if t[iit] >= setup.te[iie_]:
        Te[iie_] = T.copy()
        iie_ += 1
