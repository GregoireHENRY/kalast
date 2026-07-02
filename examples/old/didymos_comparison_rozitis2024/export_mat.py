#!/usr/bin/env python

from pathlib import Path  # noqa

import trimesh
import scipy
import numpy  # noqa

import kalast  # noqa
from kalast.util import DPR, RPD, AU  # noqa


mesh = trimesh.load("in/moon.obj")
sph = numpy.array([kalast.util.cart2sph(v) for v in mesh.triangles_center])

et_images = numpy.load("work/scene/et_images.npy")
ds = numpy.load("work/scene/ds.npy")
inc = numpy.load("work/scene/inc_all.npy")
emi = numpy.load("work/scene/emi_all.npy")
ppha = numpy.load("work/scene/ppha_all.npy")
tmp = numpy.load("work/tpm/tmp_all.npy")

df = {}
df["sph"] = sph
df["tri"] = mesh.triangles_center
df["normals"] = mesh.face_normals
df["dau"] = ds / AU
df["inc"] = inc
df["emi"] = emi
df["ppha"] = ppha
df["tmp"] = tmp

scipy.io.savemat("all.mat", df)
