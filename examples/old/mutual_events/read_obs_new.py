#!/usr/bin/env python

# path = "/Users/gregoireh/projects/kalast-renew/examples/mutual_events/eli_dates/5 examples/2022-08-16_ALCDEF.txt"
# jd_start = "2022-08-16 18:58:33"

# path = "/Users/gregoireh/projects/kalast-renew/examples/mutual_events/eli_dates/5 examples/2022-08-24_ALCDEF.txt"
# jd_start = "2022-08-23 22:01:07"

# path = "/Users/gregoireh/projects/kalast-renew/examples/mutual_events/eli_dates/5 examples/2022-08-26_ALCDEF.txt"
# jd_start = "2022-08-25 17:44:58"

# path = "/Users/gregoireh/projects/kalast-renew/examples/mutual_events/eli_dates/5 examples/2022-09-13_ALCDEF.txt"
# jd_start = "2022-09-13 20:41:19"

path = "/Users/gregoireh/projects/kalast-renew/examples/mutual_events/eli_dates/5 examples/2022-09-17_ALCDEF.txt"
jd_start = "2022-09-17 17:59:40"

jds = []
mags = []
errs = []

with open(path, "r") as F:
    for line in F.readlines():
        line = line.strip()
        if line == "":
            break

        cols = line.split("|")

        jd = float(cols[0].split("=")[1])
        mag = float(cols[1])
        err = float(cols[2])

        jds.append(jd)
        mags.append(mag)
        errs.append(err)
