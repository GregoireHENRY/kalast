#!/usr/bin/env python

import matplotlib

import kalast

p = kalast.plot.cbar.Params()
p.vmin = 0
p.vmax = 5
p.dv = 0.5
p.label = "Flux [W/m2]"
p.path = "cbar.svg"

norm = matplotlib.colors.Normalize(vmin=p.vmin, vmax=p.vmax)
mappable = matplotlib.cm.ScalarMappable(
    cmap=matplotlib.cm.inferno.resampled(100), norm=norm
)
p.mappable = mappable

kalast.plot.style.load()
kalast.plot.cbar.create(p)
matplotlib.pyplot.show()