"""`kalast._rs.tpm.emit` in the page: not here yet."""

from kalast._rs._page import missing


def __getattr__(name):
    return missing(f"kalast.tpm.emit.{name}")
