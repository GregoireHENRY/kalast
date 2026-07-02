#!/usr/bin/env python

from pathlib import Path  # noqa

import pandas
import numpy
import spiceypy as spice

import kalast  # noqa
from kalast.util import DPR, RPD  # noqa
from kalast.spice_entities import earth, moon  # noqa


dates = [
    "1989-03-17 03:31",
    "1990-11-27 05:04",
    "1992-02-15 19:54",
    "1993-11-25 17:39",
    "1995-12-03 07:13",
    "1996-05-28 20:58",
    "1996-10-23 00:30",
    "1997-06-15 05:05",
    "1997-12-08 21:52",
    "2000-04-14 10:50",
    "2002-09-26 07:01",
    "2003-01-24 01:19",
    "2011-03-14 10:32",
    "2011-11-16 23:42",
    "2012-03-04 05:07",
    "2015-02-07 05:12",
    "2017-12-02 12:49",
    "2018-04-02 09:27",
    "2018-11-26 17:59",
    "2018-12-24 11:44",
    "2019-07-21 07:57",
    "2019-12-14 09:47",
]


# Load spice
spice.kclear()
spice.furnsh(
    "/Users/gregoireh/data/spice/wgc/mk/solar_system_v0060.tm",
)
frame = "j2000"

n = len(dates)
et = numpy.zeros(n)
pe = numpy.zeros((n, 3))
de = numpy.zeros(n)
ps = numpy.zeros((n, 3))
ds = numpy.zeros(n)
pha = numpy.zeros(n)
ster = numpy.zeros(n)
lola = numpy.zeros((n, 2))


for it, date in enumerate(dates):
    et[it] = spice.str2et(date)

    (p_, _lt) = spice.spkpos(earth.name, et[it], moon.frame, "none", moon.name)
    pe[it] = p_ * 1e3
    de[it] = numpy.linalg.norm(pe[it])

    (p_, _lt) = spice.spkpos("sun", et[it], moon.frame, "none", moon.name)
    ps[it] = p_ * 1e3
    ds[it] = numpy.linalg.norm(ps[it])

    area = numpy.pi * moon.radius**2
    ster[it] = area / de[it] ** 2

    sp_, h_, lo, la, pha[it] = kalast.spice.subobs(earth, moon, et[it])
    lola[it] = [lo, la]

path = Path("out")
numpy.save(path / "dates.npy", dates)
numpy.save(path / "et_dates.npy", et)
numpy.save(path / "pe.npy", pe)
numpy.save(path / "de.npy", de)
numpy.save(path / "ps.npy", ps)
numpy.save(path / "ds.npy", ds)
numpy.save(path / "pha.npy", pha)
numpy.save(path / "ster.npy", ster)
numpy.save(path / "lola.npy", lola)

df = pandas.DataFrame(
    {
        "date": dates,
        "d [km]": de * 1e-3,
        "pha [°]": pha * DPR,
        "ster": ster,
        "sb-lo [°]": lola[:, 0] * DPR,
        "sb-la [°]": lola[:, 1] * DPR,
    }
)
df.to_excel("dates.xlsx", sheet_name="sheet1", index=False)
