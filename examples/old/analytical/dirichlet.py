#!/usr/bin/env python

import numpy
import scipy

import setup


nz = 100
z = numpy.linspace(0.0, setup.L, nz, endpoint=True)

T0 = setup.f1(nz)
T = T0.copy()
Te = numpy.zeros((setup.ne, nz))

n = 100
D = numpy.zeros(n)
for ii in range(n):
    D[ii] = (
        2.0
        / setup.L
        * scipy.integrate.simpson(
            T0 * numpy.sin((ii + 1) * numpy.pi * z / setup.L), x=z
        )
    )

for iie_, t_ in enumerate(setup.te):
    for ii in range(n):
        Te[iie_] += (
            D[ii]
            * numpy.sin((ii + 1) * numpy.pi * z / setup.L)
            * numpy.exp(-(((ii + 1) * numpy.pi / setup.L) ** 2) * setup.d * t_)
        )
