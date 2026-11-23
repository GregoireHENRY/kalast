from kalast._rs._page import Config, Hud, _fn  # noqa: F401


def colormap(name: str):
    """A built-in colormap as a 256x3 array, the way matplotlib hands one over."""
    return _fn("colormap", name)


def colormap_names():
    """Names accepted by `colormap()` and by `config.data.colormap`."""
    return list(_fn("colormap_names"))
