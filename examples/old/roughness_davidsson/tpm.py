#!/usr/bin/env python

import numpy
import glm

import kalast


# params davidsson
# a = 0.0
# e = 1.0
# p = 3600.0
# c = 830.0
# k = 0.4694  # tiu 1184
# spin_period = 160.0 * 3600.0
# ls1 = 0.12
# diameter crater = 0.036 (3 * ls1)
# dz = 60 for each ls1
# crater at equator of body at 1 AU from Sun
# spin axis perpendicular


# params

a = 0.1
e = 0.9
p = 2500.0
c = 600.0
k = 0.081667  # tiu 350
d0 = kalast.tpm.core.diffusivity(k, p, c)

spin_period = 6.0 * 3600.0
spin_axis = numpy.array([0.0, 0.0, 1.0])

dz = 1e-2
dt = 60.0
tf = 30.0 * spin_period

sun = numpy.array([1.0, 0.0, 0.0]) * kalast.util.AU

# computed automatically

twodz = 2 * dz
dz2 = dz * dz
ls1 = kalast.tpm.core.skin_depth_1(d0, spin_period)
ls2pi = kalast.tpm.core.skin_depth_2pi(d0, spin_period)
zmax = ls2pi
z = numpy.arange(0, zmax + dz, dz)
nz = z.size
# nz = 100
# z = numpy.linspace(0.0, zmax, nz, endpoint=True)
nz_ls1 = (z <= ls1).sum()
nz_ls2pi = (z <= ls2pi).sum()
nz_save = (z <= 4 * ls1).sum()
dz_diff = numpy.diff(z)
dz2in = dz_diff[:-1] * dz_diff[:-1]
dtpdz2in = dt / dz2in

t = numpy.arange(0, tf + dt, dt)
nt = t.size
# nt = int(numpy.ceil(tf / dt)) + 1
# t = numpy.linspace(0, tf, nt, endpoint=True)

maxdt = kalast.tpm.core.stability_maxdt(d0, dz2, s=0.5)
S = kalast.tpm.core.stability(d0, dt, dz2)
if S > 0.5:
    raise ValueError("Stability criteria not valid.")

d = numpy.ones(nz) * d0
T = numpy.ones(nz) * 300.0

m_spin = numpy.array(kalast.astro.matspin(spin_period, spin_axis))[:3, :3]
m_crater_to_body = numpy.array(glm.rotate(90.0 * kalast.util.RPD, glm.dvec3(0, 1, 0)))[
    :3, :3
]

# load mesh

mesh = kalast.mesh.Mesh("res/plane_crater_1024-512_h=0.437.obj", lambda x: x * 1e3)

# simulation time loop

for iit in range(0, nt):
    break
    # T[0] = ... boundary condition
    T[1:-1] = kalast.tpm.core.conduction_1d(T, d, dtpdz2in)
    T[-1] = T[-2]
