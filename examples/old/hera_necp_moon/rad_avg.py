#!/usr/bin/env python

import numpy

# import kalast


rad = numpy.load("hera_necp_moon/saved/rad_flat/rad.npy")
e = numpy.load("hera_necp_moon/saved/scene/emi_all.npy")

rad_avg = numpy.zeros(rad.size)
for ii in range(0, rad.size):
    nb_vis = (e[:, ii] < numpy.pi / 2).sum()
    rad_avg[ii] = rad[ii] / nb_vis
