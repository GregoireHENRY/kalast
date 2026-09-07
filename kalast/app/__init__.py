from kalast.app import (  # noqa
    _core,
    body,
    frame,
    config,
    gpu,
    simulation,
)

from kalast.app._core import (  # noqa
    App,
)

from kalast.app.simulation import (  # noqa
    Simulation,
)

from kalast.app.config import (  # noqa
    Hud,
    colormap,
    colormap_names,
)

del _core

# PEP 561: a re-export is only public if it is named here. Without it a strict
# type checker rejects `from kalast.app import App` -- which is what every
# example writes -- with "does not explicitly export attribute".
__all__ = [
    "App",
    "Hud",
    "Simulation",
    "body",
    "colormap",
    "colormap_names",
    "config",
    "frame",
    "gpu",
    "simulation",
]
