#!/usr/bin/env python

import matplotlib

import kalast

p = kalast.plot.cbar.Params()
p.vmin = 50
p.vmax = 350
p.dv = 50

norm = matplotlib.colors.Normalize(vmin=p.vmin, vmax=p.vmax)
mappable = matplotlib.cm.ScalarMappable(
    cmap=matplotlib.cm.inferno.resampled(100), norm=norm
)
p.mappable = mappable

kalast.plot.style.load()
kalast.plot.cbar.create(p)
