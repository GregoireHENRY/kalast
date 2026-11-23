"""`kalast._rs.tpm.core` in the page: the thermophysical model's steps, the
engine's own, on the arrays the script holds -- `t` stepped in place, as on
the desktop: `python.js` hands the engine numpy's own memory."""

import numpy
from pyodide.ffi import to_js

from kalast._rs._page import Properties, _call, _fn, _g, _Obj, _shaped, _temperatures, missing

# Not in the page yet: they say so when called, and the desktop has them.
Stepper = missing("kalast.tpm.core.Stepper")
flux_bc = missing("kalast.tpm.core.flux_bc")
bottom_flux = missing("kalast.tpm.core.bottom_flux")

def stability(d, dt, dx2):
    return _fn("stability", d, dt, dx2)


def stability_maxdt(d, dx2, s=0.5):
    return _fn("stability_maxdt", d, dx2, s)


def radiation_sun_reflected(viewf, a, cosi, dau):
    return _fn("radiation_sun_reflected", viewf, a, cosi, dau)


def radiation_sun_reflected_reuse(viewf, f, a):
    return _fn("radiation_sun_reflected_reuse", viewf, f, a)


def newton_method_fn(t, f, set3, k, subt1, subt2, twodx):
    return _fn("newton_method_fn", t, f, set3, k, subt1, subt2, twodx)


def newton_method_dfn(set3, k, twodx):
    return _fn("newton_method_dfn", set3, k, twodx)


def newton_method(t, f, se, k, subt1, subt2, twodx):
    return _fn("newton_method", t, f, se, k, subt1, subt2, twodx)


def conduction_1d(t, d, dtpdx2):
    return _fn("conduction_1d", t, d, dtpdx2)


def conduction_1d_nonuniform(t, d, coef_lo, coef_hi):
    return _fn("conduction_1d_nonuniform", t, d, coef_lo, coef_hi)


def conduction(t, f, k, dx):
    return _fn("conduction", t, f, k, dx)


def radiation_sun(dau, cosi, a):
    return _fn("radiation_sun", dau, cosi, a)


def radiation_emitted(viewf, t, e):
    return _fn("radiation_emitted", viewf, t, e)


def _node(v, layers: int, facets: int, name: str):
    """`v` one a layer and a facet: a number, one a facet, or anything that
    broadcasts to (layers, facets) as the desktop's bindings take it."""
    v = numpy.asarray(v, dtype=numpy.float32)
    shape = {0: (1, 1), 1: (1,) + v.shape}.get(v.ndim, v.shape)
    rows, cols = shape if len(shape) == 2 else (0, 0)
    if not ((rows == layers or rows == 1) and (cols == facets or cols == 1)):
        hint = " -- one value a layer is (layers, 1): values[:, None]" if v.shape == (layers,) else ""
        raise ValueError(f"{name} of shape {list(v.shape)} does not broadcast to ({layers}, {facets}), layers by facets{hint}")
    return numpy.ascontiguousarray(numpy.broadcast_to(v.reshape(rows, cols), (layers, facets)), dtype=numpy.float32)


def _facet(v, facets: int, name: str):
    v = numpy.asarray(v, dtype=numpy.float32).ravel()
    if v.size == 1:
        return numpy.full(facets, v[0], dtype=numpy.float32)
    if v.size != facets:
        raise ValueError(f"{name} is one value a facet, {facets} of them, or one for all: not {list(v.shape)}")
    return v


def columns(layers: int, facets: int, t):
    """The temperatures of `layers` layers under each of `facets` facets
    (K), `t` broadcast to them: a number, one a facet, or (layers, 1)."""
    return _node(t, layers, facets, "t")


class Ground(_Obj):
    """A body's thermal properties facet by facet and layer by layer: each
    an array, changed in place where the ground differs."""

    _kind = "Ground"

    def __init__(self, prop=None, layers=None, facets=None, *, _i=None):
        if _i is None:
            _i = int(_fn("ground_new", _body(prop), int(layers), int(facets))._i)
        object.__setattr__(self, "_i", _i)

    @staticmethod
    def graded(prop, facets, dz, depth, ratio=1.2):
        """Layers thin at the surface and thicker with depth, `ratio` each."""
        return _fn("ground_graded", _body(prop), int(facets), float(dz), float(depth), float(ratio))

    def __setattr__(self, name, value):
        layers, facets = self.layers, self.facets
        if name in ("albedo", "emissivity"):
            value = _facet(value, facets, name)
        elif name in ("conductivity", "density", "heat_capacity"):
            value = _node(value, layers, facets, name)
        super().__setattr__(name, value)


def _broadcast(**given):
    arrays = {k: numpy.asarray(v, dtype=numpy.float64) for k, v in given.items()}
    try:
        shape = numpy.broadcast_shapes(*(a.shape for a in arrays.values()))
    except ValueError:
        raise ValueError("the arguments do not broadcast together: " + ", ".join(f"{k} {list(a.shape)}" for k, a in arrays.items())) from None
    return shape, [numpy.broadcast_to(a, shape).ravel() for a in arrays.values()]


def effective_temperature(dau, r, a, e):
    shape, (r, a, e) = _broadcast(r=r, a=a, e=e)
    return _shaped(_fn("effective_temperature", dau, r, a, e), shape)


def mean_incidence(lat, dec):
    shape, (lat, dec) = _broadcast(lat=lat, dec=dec)
    return _shaped(_fn("mean_incidence", lat, dec), shape)


def _body(prop):
    if not isinstance(prop, Properties):
        raise TypeError("prop must be the body's Properties, or a Ground for them facet by facet and layer by layer")
    return prop._seven()


def solar_bc(t, dau, cosi, prop, dz) -> None:
    _temperatures(t)
    cosi = to_js(numpy.ascontiguousarray(cosi, dtype=numpy.float32).ravel())
    if isinstance(prop, Ground):
        _call(_g.step, t, "solar_bc_ground", t.shape[0], float(dau), cosi, object.__getattribute__(prop, "_i"), float(dz))
        return
    _call(_g.step, t, "solar_bc", t.shape[0], float(dau), cosi, _body(prop), float(dz))


def bottom_adiabatic(t) -> None:
    _temperatures(t)
    _call(_g.step, t, "bottom_adiabatic", t.shape[0])


def heat_conduction(t, prop, dt, dz) -> None:
    _temperatures(t)
    if isinstance(prop, Ground):
        _call(_g.step, t, "heat_conduction_ground", t.shape[0], object.__getattribute__(prop, "_i"), float(dt), float(dz))
        return
    _call(_g.step, t, "heat_conduction", t.shape[0], _body(prop), float(dt), float(dz))
