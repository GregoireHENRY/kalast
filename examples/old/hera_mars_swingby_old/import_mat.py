#!/usr/bin/env python

from pathlib import Path  # noqa

import scipy
import numpy  # noqa
import matplotlib  # noqa
from matplotlib import pyplot  # noqa

import kalast  # noqa
from kalast.util import DPR, RPD, AU  # noqa


df = scipy.io.loadmat("work/rad/R.mat")
numpy.save("R.npy", df["R"])

# df2 = scipy.io.loadmat("work/R_12mu.mat")
# numpy.save("out/r/R_12mu.npy", df2["R"])

# x = list(range(0, df["R"].shape[1]))
# ax.scatter(x, df["R"][3], s=10, fc="none", marker="s", ec="r", label="8mu")
# ax.scatter(x, df2["R"][3], s=10, fc="none", marker="o", ec="b", label="12mu")

# fig, ax = pyplot.subplots(figsize=(6, 4))
# ax.set_xlabel("Face index")
# ax.set_ylabel("Roughness factor")
# ax.plot(df["R"][3], c="r", label="8mu")
# ax.plot(df2["R"][3], c="b", label="12mu")
# ax.set_xlim(0, df["R"].shape[1])
# ax.set_ylim(0, 10)
# ax.legend()
# fig.savefig("R_8-12mu_ii3.png", bbox_inches="tight", dpi=300)
#
# fig, ax = pyplot.subplots(figsize=(6, 4))
# ax.set_xlabel("Face index")
# ax.set_ylabel("Roughness factor")
# ax.plot(df["R"].mean(axis=0), c="r", label="8mu")
# ax.plot(df2["R"].mean(axis=0), c="b", label="12mu")
# ax.set_xlim(0, df["R"].shape[1])
# ax.set_ylim(0, 10)
# ax.legend()
# fig.savefig("R_8-12mu_mean_time.png", bbox_inches="tight", dpi=300)
#
# fig, ax = pyplot.subplots(figsize=(6, 4))
# ax.set_xlabel("Image index")
# ax.set_ylabel("Roughness factor")
# ax.plot(df["R"].mean(axis=1), c="r", label="8mu")
# ax.plot(df2["R"].mean(axis=1), c="b", label="12mu")
# ax.set_xlim(0, df["R"].shape[0])
# ax.set_ylim(0, 10)
# ax.legend()
# fig.savefig("R_8-12mu_mean_face.png", bbox_inches="tight", dpi=300)
