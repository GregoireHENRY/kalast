"""kalast's Rust extension in a browser: the page's own. See `_page`.

A module of the desktop's extension the page has not got -- `kalast._rs.
shadowing`, `kalast._rs.tpm.gpu` -- is made when imported, and each of its
names says, when used, that it is not in the browser yet.
"""

import importlib.abc
import importlib.machinery
import sys


class _Unported(importlib.abc.MetaPathFinder, importlib.abc.Loader):
    """The last finder asked: what the page's files do not have."""

    def find_spec(self, name, path=None, target=None):
        if name.startswith(__name__ + "."):
            return importlib.machinery.ModuleSpec(name, self, is_package=True)
        return None

    def create_module(self, spec):
        return None

    def exec_module(self, module):
        from kalast._rs._page import missing

        public = module.__name__.replace("kalast._rs.", "kalast.", 1)
        module.__path__ = []

        def __getattr__(name):
            if name.startswith("__"):
                raise AttributeError(name)
            return missing(f"{public}.{name}")

        module.__getattr__ = __getattr__


if not any(isinstance(f, _Unported) for f in sys.meta_path):
    sys.meta_path.append(_Unported())
