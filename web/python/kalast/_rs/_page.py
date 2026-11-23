"""kalast's Rust extension in a browser: the page's own.

On the desktop `kalast._rs` is the extension module the bindings of `src/py`
make. In kalast.greghy.com's page, kalast runs as WebAssembly beside this
CPython (Pyodide), and each class here is a handle on the engine's object of
that name: its attributes and methods are calls into the page
(`web/src/python.rs`), through JavaScript (`web/python.js`). The package on
top -- `kalast.app`, `kalast.tpm`, `kalast.editor` -- is the repository's,
unchanged, and scripts run as the UI app's scripts' thread runs them.

What the page does not have yet says so where it is used, naming the
desktop app, which has it.
"""

import importlib.abc
import importlib.machinery
import os
import sys
import traceback

import numpy
from pyodide.ffi import JsException, JsProxy, create_proxy, to_js

import kalast_bridge as _b  # the page's exports
import kalast_glue as _g  # python.js: numpy's memory for the engine

try:
    from pyodide.ffi import run_sync as _run_sync
except ImportError:  # an older Pyodide
    _run_sync = None
try:
    from pyodide.ffi import jsnull as _jsnull
except ImportError:
    _jsnull = None

_DESKTOP = "kalast's desktop app has it"


class _PyodidePackages(importlib.abc.MetaPathFinder):
    """A package Pyodide has -- scipy, matplotlib, astropy -- loaded the first
    time anything imports it, kalast's own modules too, where the browser can
    hold the import while it downloads (JavaScript Promise Integration)."""

    def find_spec(self, name, path=None, target=None):
        if path is not None or _run_sync is None or not _g.can_wait():
            return None
        package = _g.package_for(name)
        if not package:
            return None
        _run_sync(_g.load(package))
        importlib.invalidate_caches()
        return importlib.machinery.PathFinder.find_spec(name)


if not any(isinstance(f, _PyodidePackages) for f in sys.meta_path):
    sys.meta_path.append(_PyodidePackages())

_ERRORS = {
    e.__name__: e
    for e in (
        ValueError,
        TypeError,
        AttributeError,
        IndexError,
        OverflowError,
        RuntimeError,
        NotImplementedError,
        FileNotFoundError,
    )
}


def _python_error(e: JsException) -> BaseException:
    """The Python exception the page's message names: `ValueError: ...`."""
    text = str(e)
    for prefix in ("Error: ", "RuntimeError: unreachable"):
        if text.startswith(prefix):
            text = text[len(prefix):]
    kind, sep, message = text.partition(": ")
    if sep and kind in _ERRORS:
        return _ERRORS[kind](message)
    return RuntimeError(text)


def _call(f, *args):
    try:
        return f(*args)
    except JsException as e:
        raise _python_error(e) from None


# --- values to and from the page --------------------------------------------


def _floats(a, dtype=numpy.float64):
    """A JavaScript typed array of `a`'s values, flat."""
    return to_js(numpy.ascontiguousarray(numpy.asarray(a, dtype=dtype)).ravel())


def _enc(v):
    """A value for the page: Python's scalars as JavaScript's, an object as
    its index, a list of objects as their indices, numbers as a typed array."""
    if v is None or isinstance(v, (bool, int, float, str)):
        return v
    if isinstance(v, numpy.generic):
        return v.item()
    if isinstance(v, _Obj):
        return object.__getattribute__(v, "_i")
    if isinstance(v, (list, tuple)) and v and all(isinstance(x, _Obj) for x in v):
        return to_js([object.__getattribute__(x, "_i") for x in v])
    if isinstance(v, (list, tuple)) and not v:
        return to_js([])
    if callable(v):
        return v
    return _floats(v)


class _Linked(numpy.ndarray):
    """An array of kalast's that writes back when changed in place.

    On the desktop `body.mat` and `mesh.colors` are numpy views onto the
    engine's own memory, so `mat[:3, 3] = p` moves the body. The page's
    engine is another WebAssembly memory: the array here is a copy, and an
    assignment into it is sent back.
    """

    def __setitem__(self, key, value):
        super().__setitem__(key, value)
        back = getattr(self, "_back", None)
        if back is not None:
            owner, name = back
            setattr(owner, name, numpy.asarray(self))

    def __array_finalize__(self, obj):
        # A slice of it is a plain array: an assignment into a slice is the
        # slice's alone, as numpy has it for a copy.
        self._back = None


def _array(v, back=None):
    data = numpy.asarray(v.d.to_py())
    dtype = getattr(v, "y", None)
    if dtype:
        data = data.astype(dtype)
    shape = tuple(int(n) for n in v.s.to_py())
    a = data.reshape(shape)
    if v.k and back is not None:
        a = a.view(_Linked)
        a._back = back
    return a


class _Method:
    pass


_METHOD = _Method()


def _dec(v, back=None):
    """A value from the page, as the desktop's bindings give it."""
    if v is _jsnull:
        return None
    if not isinstance(v, JsProxy):
        return v
    if _g.is_array(v):
        return [_dec(x) for x in v]
    t = getattr(v, "t", None)
    if t == "a":
        return _array(v, back)
    if t == "o":
        return _make(v.k, int(v.i))
    if t == "m":
        return _METHOD
    return v.to_py()


# --- objects -----------------------------------------------------------------

# Each method's parameters in order, for its keyword arguments, and their
# defaults where the page's engine wants a value the desktop's binding
# would have filled in.
_SIGNATURES = {
    ("Simulation", "load_mesh"): ("path", "mat", "smooth", "shadow_path", "flatten"),
    ("Simulation", "facet_mean_incidence"): ("body", "spin_axis"),
    ("Simulation", "meridian_facets"): ("body", "longitude", "step"),
    ("Simulation", "facet_shadow"): ("body",),
    ("Simulation", "facet_illumination"): ("body",),
    ("Simulation", "facet_incidence"): ("body",),
    ("Simulation", "request_facet_shadow"): ("body",),
    ("Simulation", "remove_body"): ("index",),
    ("Simulation", "toggle_facet"): ("body", "facet"),
    ("Simulation", "project"): ("point",),
    ("Simulation", "project_body"): ("body",),
    ("Simulation", "project_facet"): ("body", "facet"),
    ("Simulation", "pick_facet"): ("origin", "direction"),
    ("Eye", "set_target"): ("target",),
    ("Eye", "view_along"): ("axis", "orthographic", "positive"),
    ("App", "log"): ("line",),
    ("App", "set_script"): ("path", "source"),
}


def _bind(kind, name, args, kwargs):
    if not kwargs:
        return list(args)
    params = _SIGNATURES.get((kind, name))
    if params is None:
        raise TypeError(f"{name}() takes no keyword arguments in the browser")
    out = list(args) + [None] * (len(params) - len(args))
    for k, v in kwargs.items():
        if k not in params:
            raise TypeError(f"{name}() got an unexpected keyword argument {k!r}")
        out[params.index(k)] = v
    while out and out[-1] is None:
        out.pop()
    return out


class _Obj:
    """A handle on one of the engine's objects, by its kind and index."""

    __slots__ = ("_i",)
    _kind = ""

    def __init__(self, i: int = 0) -> None:
        object.__setattr__(self, "_i", i)

    def __getattr__(self, name):
        if name.startswith("__") and name.endswith("__"):
            raise AttributeError(name)
        kind, i = type(self)._kind, object.__getattribute__(self, "_i")
        v = _dec(_call(_b.py_get, kind, i, name), back=(self, name))
        if v is _METHOD:
            return lambda *args, **kwargs: self._invoke(name, args, kwargs)
        return v

    def __setattr__(self, name, value):
        kind, i = type(self)._kind, object.__getattribute__(self, "_i")
        _call(_b.py_set, kind, i, name, _enc(value))

    def _invoke(self, name, args, kwargs):
        kind, i = type(self)._kind, object.__getattribute__(self, "_i")
        args = _bind(kind, name, args, kwargs)
        return _dec(_call(_b.py_call, kind, i, name, to_js([_enc(a) for a in args])))

    def __repr__(self) -> str:
        return self._invoke("__repr__", (), {})

    def __eq__(self, other):
        return type(self) is type(other) and object.__getattribute__(self, "_i") == object.__getattribute__(other, "_i")

    def __hash__(self):
        return hash((type(self)._kind, object.__getattribute__(self, "_i")))


class AppConfig(_Obj):
    _kind = "AppConfig"


class Config(_Obj):
    _kind = "Config"


class _ConfigGroup(_Obj):
    _kind = "ConfigGroup"


class State(_Obj):
    _kind = "State"


class Eye(_Obj):
    _kind = "Eye"


class Projection(_Obj):
    _kind = "Projection"


class Body(_Obj):
    _kind = "Body"


class Hud(_Obj):
    """One HUD overlay: a template, a corner, and how it looks."""

    _kind = "Hud"

    def __init__(self, text: str = "", anchor="top-left", x=None, y=None, size=18.0, color=None,
                 align_h=None, align_v=None, *, _i=None) -> None:
        if _i is None:
            args = [text, anchor, x, y, size, color, align_h, align_v]
            made = _call(_b.py_new, "Hud", to_js([_enc(a) for a in args]))
            _i = int(made.i)
        object.__setattr__(self, "_i", _i)


class Facet:
    """A facet: its centre, normal and area."""

    def __init__(self, pos=None, normal=None, area=None):
        self.pos, self.normal, self.area = pos, normal, area

    def __repr__(self):
        return f"Facet(pos={self.pos}, normal={self.normal}, area={self.area})"


class _Facets(_Obj):
    """`mesh.facets`: a sequence of the facets' centres, normals, areas."""

    _kind = "Facets"

    def __len__(self) -> int:
        return int(self._invoke("__len__", (), {}))

    def __getitem__(self, i):
        if isinstance(i, slice):
            return [self[k] for k in range(*i.indices(len(self)))]
        pos, normal, area = self._invoke("__getitem__", (i,), {})
        return Facet(pos=pos, normal=normal, area=area)

    def __iter__(self):
        return (self[k] for k in range(len(self)))


def _to_engine(path):
    """A file the script wrote, into the page's files, where the engine reads."""
    if path is not None and not _b.py_has_file(str(path)) and os.path.isfile(path):
        with open(path, "rb") as f:
            _b.py_write_file(str(path), to_js(f.read()))


class Mesh(_Obj):
    """A mesh: a body's, `sim.bodies[i].mesh`, or one of the script's own
    from a file, `Mesh(path=...)`, which `sim.add_mesh()` puts in the scene."""

    _kind = "Mesh"

    def __init__(self, path=None, update_pos=None, vertices=None, facets=None, indices=None, material_id=None,
                 *, _i=None):
        if _i is None:
            if path is None or any(a is not None for a in (update_pos, vertices, facets, indices, material_id)):
                raise NotImplementedError(f"Mesh() is in the browser from a file alone, Mesh(path=...): {_DESKTOP}")
            _to_engine(path)
            _i = int(_call(_b.py_new, "Mesh", to_js([str(path)])).i)
        object.__setattr__(self, "_i", _i)

    @classmethod
    def load(cls, path, update_pos=None):
        return cls(path=path, update_pos=update_pos)


class Simulation(_Obj):
    _kind = "Simulation"

    def load_mesh(self, path, mat=None, smooth=False, shadow_path=None, flatten=None):
        # `flatten=` until 17 September, inverted, as the desktop takes it.
        if flatten is not None:
            import warnings

            warnings.warn("load_mesh(flatten=...) is deprecated: meshes load flat by default; "
                          "pass smooth=True to keep the file's shared vertices", DeprecationWarning, stacklevel=2)
            smooth = not flatten
        for p in (path, shadow_path):
            _to_engine(p)
        args = [str(path), None if mat is None else numpy.asarray(mat, dtype=numpy.float64), bool(smooth),
                None if shadow_path is None else str(shadow_path)]
        _call(_b.py_call, "Simulation", 0, "load_mesh", to_js([_enc(a) for a in args]))

    def add_mesh(self, mesh, mat=None):
        args = [mesh, None if mat is None else numpy.asarray(mat, dtype=numpy.float64)]
        _call(_b.py_call, "Simulation", 0, "add_mesh", to_js([_enc(a) for a in args]))


_KINDS = {
    "AppConfig": AppConfig,
    "Config": Config,
    "ConfigGroup": _ConfigGroup,
    "Simulation": Simulation,
    "State": State,
    "Eye": Eye,
    "Projection": Projection,
    "Body": Body,
    "Mesh": Mesh,
    "Facets": _Facets,
}


def _make(kind: str, i: int):
    if kind == "Hud":
        return Hud(_i=i)
    if kind == "Ground":
        from kalast._rs.tpm.core import Ground

        return Ground(_i=i)
    if kind == "Mesh":
        return Mesh(_i=i)
    if kind == "App":
        return App()
    return _KINDS[kind](i)


class App(_Obj):
    """The page's app: the one on screen, whatever a script makes.

    As in the UI app's scripts' thread, `App()` in a script is the app it is
    shown in (`kalast.editor`), and `step()` waits for its next frame.
    """

    _kind = "App"

    def __init__(self) -> None:
        object.__setattr__(self, "_i", 0)

    def start(self) -> None:
        # The page's loop is running already; `kalast.editor` makes a
        # script's `start()` a loop of `step()`s, as the UI app does.
        while self.step():
            if not self.running:
                break

    def step(self) -> bool:
        if _run_sync is None or not _g.can_wait():
            raise RuntimeError(
                "step() waits for the page's next frame, which this browser cannot do: Chrome and Edge can "
                "(JavaScript Promise Integration). A script written with callbacks and start() runs here; "
                f"{_DESKTOP}"
            )
        _run_sync(_b.py_next_frame())
        return self.running

    def __setattr__(self, name, value):
        if name in ("before_render", "tick", "after_render"):
            value = None if value is None else _tick(value)
        _call(_b.py_set, "App", 0, name, value)

    def __repr__(self) -> str:
        return "App(the page's)"


_SIM = Simulation(0)


def _tick(f):
    """A callback for the page's frames: called with the simulation and
    `dt`, its traceback shown in the script tab, false when it raised."""

    def called(_sim, dt):
        try:
            f(_SIM, dt)
            return True
        except BaseException:
            traceback.print_exc()
            return False

    return create_proxy(called)


# --- the TPM ------------------------------------------------------------------


class Properties:
    """A body's thermal properties, as the desktop's `Properties`."""

    _FIELDS = ("albedo", "emissivity", "density", "heat_capacity", "thermal_inertia", "conductivity", "diffusivity")

    def __init__(self, albedo=0.0, emissivity=1.0, density=0.0, heat_capacity=0.0, thermal_inertia=0.0,
                 conductivity=0.0, diffusivity=0.0):
        self.albedo = float(albedo)
        self.emissivity = float(emissivity)
        self.density = float(density)
        self.heat_capacity = float(heat_capacity)
        self.thermal_inertia = float(thermal_inertia)
        self.conductivity = float(conductivity)
        self.diffusivity = float(diffusivity)

    def _seven(self):
        return to_js([float(getattr(self, k)) for k in self._FIELDS])

    def _take(self, values):
        for k, v in zip(self._FIELDS, values):
            setattr(self, k, float(v))
        return self

    def compute_conductivity_diffusivity(self) -> None:
        self._take(_call(_b.py_fn, "compute_conductivity_diffusivity", to_js([self._seven()])).to_py())

    def __repr__(self) -> str:
        fields = ", ".join(f"{k}={getattr(self, k)}" for k in self._FIELDS)
        return f"Properties({fields})"


def _preset(name: str) -> Properties:
    return Properties()._take(_call(_b.py_fn, "preset", to_js([name])).to_py())


def _fn(name, *args):
    return _dec(_call(_b.py_fn, name, to_js([_enc(a) for a in args])))


def _temperatures(t, name="t"):
    if not (isinstance(t, numpy.ndarray) and t.dtype == numpy.float32 and t.ndim == 2 and t.flags.writeable
            and t.flags.c_contiguous):
        raise TypeError(f"{name} must be a writable float32 array of (layers, facets), as kalast.tpm.core.columns makes it")
    return t


def _shaped(values, shape):
    """Values back in the shape the arguments broadcast to, a number for numbers."""
    a = numpy.asarray(values).reshape(shape)
    return a.item() if a.ndim == 0 else a


def missing(qualname: str):
    """A name of the desktop's extension the page has not got yet: it says
    so when used."""

    class Missing:
        def __init__(self, *args, **kwargs):
            raise NotImplementedError(f"{qualname} is not in the browser yet: {_DESKTOP}")

        def __call__(self, *args, **kwargs):
            raise NotImplementedError(f"{qualname} is not in the browser yet: {_DESKTOP}")

    Missing.__name__ = Missing.__qualname__ = qualname.rsplit(".", 1)[-1]
    return Missing


class Record:
    """An object the page only holds: what a script sets on it, kept as it
    is -- `kalast.entity`'s bodies, made as the package imports."""

    def __init__(self, *args, **kwargs):
        for k, v in kwargs.items():
            setattr(self, k, v)


# --- what `python.js` runs ---------------------------------------------------


def run_script(path: str, source: str) -> None:
    """Run a script as the UI app's scripts' thread does: `kalast.editor`'s
    `run_toplevel`, its prints to the script tab, `App()` the page's app."""
    import kalast.editor as editor

    # A browser that cannot hold a call for a frame cannot loop `step()`s,
    # which the UI app makes of `start()`: the page's loop goes on drawing,
    # and the script's callbacks with it.
    if _run_sync is None or not _g.can_wait():
        editor._ScriptApp.start = lambda self: None
    app = App()
    editor.capture_output(app)
    editor.run_toplevel(app, source, path)


def serve_console() -> None:
    """The lines typed at the python tab, and a Tab, between frames."""
    import kalast.editor as editor

    app = App()
    editor.capture_output(app)
    editor.serve_console(app)
