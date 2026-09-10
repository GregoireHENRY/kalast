from kalast import (  # noqa
    # Rust modules bindings
    app,
    astro,
    entity,
    math,
    mesh,
    scattering,
    spice,
    tpm,
    util,
    # Pure python modules
    io,
    plot,
    # typing,
)

# Tidied away rather than deleted outright, because whether it is bound here
# depends on how the bindings arrived. Importing the installed extension binds
# `_rs` on this package as a side effect of the submodule load; a Rust front
# door that embeds an interpreter supplies the same bindings through
# `sys.modules` instead, and no attribute is ever set.
globals().pop("_rs", None)  # noqa

__all__ = [
    "app",
    "astro",
    "entity",
    "io",
    "math",
    "mesh",
    "plot",
    "scattering",
    "spice",
    "tpm",
    "util",
]
