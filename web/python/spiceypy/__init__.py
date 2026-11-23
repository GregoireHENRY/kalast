"""SPICE is not in the browser: spiceypy wraps CSPICE, which has no build for
the page's Python. Importing works, so that a script importing it at its top
runs its other parts; calling it says so. kalast's desktop app has it, with
the kernels on your disk."""


def __getattr__(name):
    if name.startswith("__"):
        raise AttributeError(name)

    def unavailable(*args, **kwargs):
        raise NotImplementedError(
            f"spiceypy.{name}: SPICE is not in the browser -- kalast's desktop app has it, with your kernels"
        )

    return unavailable
