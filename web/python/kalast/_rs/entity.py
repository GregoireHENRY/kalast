"""`kalast._rs.entity` in the page: the bodies, cameras and spacecraft kalast
knows, with their values, and the classes to make others. What they are for,
SPICE's names and frames, is not in the browser: spiceypy says so."""

import numpy

from kalast._rs._page import Record, _fn


class Body(Record):
    pass


class Camera(Record):
    pass


class Spacecraft(Record):
    pass


_CLASSES = {"Body": Body, "Camera": Camera, "Spacecraft": Spacecraft}

for _name, _fields in _fn("entities").items():
    _fields = dict(_fields)
    if "radii" in _fields:
        _fields["radii"] = numpy.array(_fields["radii"])
    globals()[_name] = _CLASSES[_fields.pop("kind")](**_fields)
del _name, _fields
