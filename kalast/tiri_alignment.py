"""Measured TIRI boresight alignment, which the frames kernel does not carry.

`hera_v16.tf` defines `HERA_TIRI` as **nominally** co-aligned with the
spacecraft -- `TKFRAME_ANGLES = (0, 0, 0)`, a pre-flight value never
calibrated in flight. After the detector convention was corrected (see below),
targets still sit 0.60 deg from where that puts them, consistently.

    rotation 0.6002 deg, spread 0.0228, about (+0.8674, +0.4976, -0.0014)

which moves the boresight by (+0.299, -0.521) deg in TIRI X and Y. The roll
component about the boresight is -0.0008 deg, i.e. none is invented: the
minimal rotation is used, as the fitting frames are nearly collinear and do
not constrain roll. A least-squares (Kabsch) fit on the same frames absorbs a
spurious 5.96 deg roll, which is why it is not used.

**Provenance.** Fitted to the four Mars swing-by frames where Mars is out of
shot and Deimos is unambiguous (11:56:03, 11:58:43, 12:01:23, 12:04:03; SNR
198, 170, 137, 97). Residual after applying it: 3.5 px mean, 7.0 px worst.

**Independently cross-validated on Mars**, which took no part in the fit. Mars
is a 600-700 px disc against Deimos's point source, observed 06:10-09:11, more
than three hours earlier. Its limb-fit centre in Y lands +0.59 px from the
corrected prediction (sd 3.20), against +40.4 px uncorrected. Two targets
differing ~100x in apparent size, hours apart, agreeing to under a pixel is
what makes this an alignment rather than a fudge.

Mars's X is *not* used: its limb fit is pulled ~10 px toward the bright
afternoon hemisphere, so only Deimos constrains X.

--- retracted predecessor, kept as the record of a wrong turn ---

An earlier version of this module carried 0.7297 deg about
(-0.6977, -0.7164, 0.0016) and was correctly held at ALIGN = False, because
the evidence for it was three frames spanning five minutes and its Y component
drifted unexplained.

That value was mostly an artefact. TIRI's detector runs 180 deg from the frame
the render assumed -- `camera.up` was -Y and should be +Y -- so predictions
were wrong by up to 3.5 deg and a rotation was being fitted to the residual of
an axis flip. The tell, missed at the time: the "offset" grew from 0.66 to
1.05 deg as Mars closed in, which no rigid rotation can do.

Three things settled the flip, none of them relying on this renderer:

1.  Deimos traverses the field during the flyby and its measured column runs
    *opposite* to the prediction, so no translation can fit it. Residual
    against the real radiances: 270 px unflipped, 17 px flipped.
2.  Mars's thermal peak sits 150 px from the predicted sub-solar point
    unflipped, 41 px flipped.
3.  Reprojecting Mars onto a lat/lon grid only yields a map consistent between
    epochs under the 180 deg convention: correlation 0.74, against 0.20
    (unflipped), 0.04 (X only) and 0.01 (Y only). Mars rotates 19.5 deg
    between the epochs used, so this test needs no Mars map and no renderer.

Note the Y component of the retracted value, +0.509 deg, was close to the
+0.521 deg measured here. Deimos sits near the field centre, where flipping Y
barely moves it, so that part of the old fit was measuring the real residual
all along; its X component was fitting the flip.

See notes/2026-09-02_tiri_geometry/.
"""

import numpy

AXIS = [0.867385, 0.497636, -0.001393]
ANGLE_DEG = 0.600215


def rotation():
    """The 3x3 correction, applied to a vector in the HERA_TIRI frame."""
    a = numpy.asarray(AXIS, dtype=float)
    a = a / numpy.linalg.norm(a)
    t = numpy.radians(ANGLE_DEG)
    k = numpy.array([[0.0, -a[2], a[1]],
                     [a[2], 0.0, -a[0]],
                     [-a[1], a[0], 0.0]])
    return numpy.eye(3) + numpy.sin(t) * k + (1.0 - numpy.cos(t)) * (k @ k)


def apply(v):
    """Correct a HERA_TIRI-frame vector. Preserves magnitude."""
    return rotation() @ numpy.asarray(v, dtype=float)
