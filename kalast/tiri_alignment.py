"""Measured TIRI boresight alignment, which the frames kernel does not carry.

`hera_v16.tf` defines `HERA_TIRI` as **nominally** co-aligned with the
spacecraft -- `TKFRAME_ANGLES = (0, 0, 0)` -- a pre-flight value never
calibrated in flight. Deimos appears 0.73 degrees from where that puts it in
the Mars swing-by frames, and the discrepancy is not subtle: within 25 px of
the prediction no pixel exceeds 5 sigma, while the source where Deimos
actually is reaches 519 sigma.

Applying this correction takes the residual from 53-58 px to 0.5-5.1 px.

**Provenance and limits.** Measured from three frames spanning five minutes
(11:56:03, 11:58:43, 12:01:23), the only ones where Mars is out of shot and
Deimos is unambiguous. The rotation is 0.7297 deg, spread 0.0254. Its X
component is steady to 0.014 deg; the Y component drifts 0.586 -> 0.467 over
those five minutes, which is larger than the scatter and unexplained, and is
most of the 3.4 px that remains.

Three closely spaced frames constrain one direction, so the rotation *about*
the boresight is not determined -- the minimal rotation is used, which adds no
arbitrary roll. That is right for these frames and would not be for a target
far off-axis.

**Do not switch this on to make things agree.** It is off by default and should
stay off until the 0.73 deg is explained, because applying it assumes this
renderer is correct and the kernels are not -- the wrong way round. The
trajectory is reconstructed and trusted; the more likely explanation is a fault
here that has not been found.

**Mars cannot arbitrate, contrary to a first attempt.** The obvious test is
whether the same rotation also fixes Mars: a rigid misalignment moves
everything by one angle, while a projection fault grows with distance from the
boresight. But the bright/dark boundary on a thermal infrared image of Mars is
the **terminator**, not the limb, and a thermal terminator at that, lagging the
illumination one. Fitting it as a circle gave implied Mars distances of 39,343
and 19,561 km on two frames 32 seconds apart, where the truth moves from 24,438
to 24,174. The measurement is broken, and any conclusion drawn from it -- this
module included -- is unsupported.

What would settle it is an independent renderer driven by the same kernels:
ShapeViewer or Cosmographia. If they place Deimos where this code does, the
fault is common to the kernels or the instrument definition; if they place it
where the observation does, the fault is here.
"""

import numpy

# Minimal rotation carrying the predicted direction onto the observed one,
# in the nominal HERA_TIRI frame.
AXIS = numpy.array([-0.6977, -0.7164, 0.0016])
ANGLE_DEG = 0.7297


def rotation(axis=None, angle_deg=None):
    """The 3x3 correction, to be applied to vectors in the nominal frame."""
    a = numpy.asarray(AXIS if axis is None else axis, dtype=float)
    a = a / numpy.linalg.norm(a)
    th = numpy.radians(ANGLE_DEG if angle_deg is None else angle_deg)
    k = numpy.array([[0.0, -a[2], a[1]], [a[2], 0.0, -a[0]], [-a[1], a[0], 0.0]])
    return numpy.eye(3) + numpy.sin(th) * k + (1.0 - numpy.cos(th)) * k @ k


def apply(v, **kw):
    """Correct a position or direction expressed in the nominal TIRI frame."""
    return rotation(**kw) @ numpy.asarray(v, dtype=float)
