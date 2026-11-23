"""`kalast._rs.telescope` in the page: not here yet; each name says so when used."""

from kalast._rs._page import missing


def __getattr__(name):
    return missing(f"kalast.telescope.{name}")
