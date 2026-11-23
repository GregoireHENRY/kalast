"""`kalast._rs.tpm.exchange` in the page: not here yet."""

from kalast._rs._page import missing


def __getattr__(name):
    return missing(f"kalast.tpm.exchange.{name}")
