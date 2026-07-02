#!/usr/bin/env python

import numpy

import kalast


rho = 2500.0
c = 600.0
k = 0.081667
d = kalast.tpm.core.diffusivity(k, rho, c)
p = 6.0 * 3600.0
zf = 0.1
ls = kalast.tpm.core.skin_depth_1(d, p)

tm = 300.0
ta = 100.0

z = numpy.linspace(0.0, zf, 100)
t = numpy.linspace(0.0, p, 10)


def f(z: numpy.array, t: float) -> numpy.array:
    return tm + ta * numpy.exp(-z / ls) * numpy.sin(z / ls - 2.0 * numpy.pi * t / p)
