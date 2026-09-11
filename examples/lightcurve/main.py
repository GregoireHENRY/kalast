#!/usr/bin/env python
"""A rotation light curve, from a shape, a spin state and a scattering law.

This is what `kalast.scattering` and `kalast._rs.shadowing` are for: the first
says how bright a facet is, the second how much of it the Sun and the observer
can see, and `kalast.lightcurve` sums them over the body once per epoch.

A triaxial body, because a sphere's light curve is a flat line. Needs only
`res/`, so it runs on a fresh clone with no data paths.

Run:  python examples/lightcurve/main.py [--plot]
"""

import sys

import numpy

import kalast.mesh
from kalast.lightcurve import Spin, lightcurve
from kalast.scattering import Hapke

PERIOD = 7.63  # hours
PHASE_ANGLE = numpy.radians(20.0)

mesh = kalast.mesh.Mesh(path="res/ico3.obj")
shape = mesh.positions * [1.6, 1.1, 1.0]

spin = Spin(pole_lon=numpy.radians(85.0), pole_lat=numpy.radians(-40.0), period=PERIOD)
epochs = numpy.linspace(0.0, PERIOD, 90)

curve = lightcurve(
    shape,
    mesh.indices,
    spin,
    sun=[1.0, 0.0, 0.0],
    observer=[numpy.cos(PHASE_ANGLE), numpy.sin(PHASE_ANGLE), 0.0],
    epochs=epochs,
    law=Hapke(w=0.10, b=0.30, c=0.60, b0=1.0, h=0.05),
    # A convex shape self-shadows nowhere, so the two clipping passes return 1
    # everywhere and cost the run. Drop these two lines for a real, concave
    # shape model -- that is what the exact partial shadowing is for.
    shadowing=False,
    visibility=False,
)

mag = curve.magnitude()
print(f"{len(curve)} epochs over {PERIOD} h, phase angle {numpy.degrees(PHASE_ANGLE):.0f} deg")
print(f"amplitude       {(mag.max() - mag.min()) * 1000:.1f} mmag")
print(f"lit fraction    {curve.lit_fraction.min():.4f} (1 = nothing self-shadowed)")
print()
print(" t (h)   rot (deg)   rel. mag")
for t, p, m in list(zip(curve.epoch, curve.phase, mag))[::10]:
    print(f"{t:6.2f}   {numpy.degrees(p) % 360:7.1f}   {m * 1000:+8.1f} mmag")

if "--plot" in sys.argv:
    import matplotlib.pyplot as plt

    plt.plot(curve.epoch, mag * 1000)
    plt.gca().invert_yaxis()  # brighter is up
    plt.xlabel("time (h)")
    plt.ylabel("relative magnitude (mmag)")
    plt.title(f"{PERIOD} h rotation, Hapke, {numpy.degrees(PHASE_ANGLE):.0f} deg phase")
    plt.show()
