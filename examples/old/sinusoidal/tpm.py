#!/usr/bin/env python

import numpy

import kalast

import setup


t = numpy.linspace(0.0, setup.p, 3000, endpoint=True)
z = numpy.linspace(0.0, setup.zf, 100, endpoint=True)

nt = t.size
dt = numpy.diff(t)
dt0 = dt[0]

nz = z.size
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

T = setup.f(z, 0.0)
Te = numpy.zeros((setup.t.size, nz))
iie_ = 0

for iit in range(0, nt):
    t_ = t[iit]
    T[0] = setup.f(0.0, t_)
    T[1:-1] = kalast.tpm.core.conduction_1d(T, d, dtpdz2in)
    T[-1] = T[-2]

    if t_ >= setup.t[iie_]:
        Te[iie_] = T.copy()
        iie_ += 1
