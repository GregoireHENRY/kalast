#!/usr/bin/env python

import numpy

import kalast


def f1(n: int) -> numpy.array:
    T = numpy.ones(n) * 300.0
    T[0] = 0.0
    T[-1] = 0.0
    return T


def f2(z: numpy.array, a: float, b: float) -> numpy.array:
    return a * numpy.exp(-z * b)


a = 300.0
b = 1.0
L = 0.1

tf = 40 * 3600.0

p = 2500.0
c = 600.0
k = 0.081667
d = kalast.tpm.core.diffusivity(k, p, c)

te = numpy.array([5 * 60.0, 1 * 3600.0, 4 * 3600.0, 10 * 3600.0, tf])
ne = te.size
