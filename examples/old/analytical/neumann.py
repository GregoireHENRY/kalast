#!/usr/bin/env python

import numpy
import scipy

import setup


nz = 100
z = numpy.linspace(0.0, setup.L, nz, endpoint=True)

T0 = setup.f2(z, 300.0, 1.0)
T = T0.copy()
Te = numpy.zeros((setup.ne, nz))

n = 100
B = numpy.zeros(n)
B[0] = scipy.integrate.simpson(T0, x=z) / setup.L
for ii in range(1, n):
    B[ii] = (
        2.0
        / setup.L
        * scipy.integrate.simpson(T0 * numpy.cos(ii * numpy.pi * z / setup.L), x=z)
    )

for iie_, t_ in enumerate(setup.te):
    for ii in range(0, n):
        Te[iie_] += (
            B[ii]
            * numpy.cos(ii * numpy.pi * z / setup.L)
            * numpy.exp(-((ii * numpy.pi / setup.L) ** 2) * setup.d * t_)
        )
