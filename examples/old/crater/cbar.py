#!/usr/bin/env python

import matplotlib

import kalast

p = kalast.plot.cbar.Params()
p.vmin = 0
p.vmax = 90
p.dv = 10
p.label = "incidence [deg]"

norm = matplotlib.colors.Normalize(vmin=p.vmin, vmax=p.vmax)
mappable = matplotlib.cm.ScalarMappable(
    cmap=matplotlib.cm.cividis_r.resampled(100), norm=norm
)
p.mappable = mappable

kalast.plot.style.load()
kalast.plot.cbar.create(p)
