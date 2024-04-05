#!/usr/bin/env python

from dataclasses import dataclass, field
from pathlib import Path

import numpy
from matplotlib import pyplot, ticker

# import matplotlib
# import scipy
# import util


@dataclass
class Data:
    x: numpy.array = None
    y: numpy.array = None
    label: str = None
    color: str = "k"
    lw: float = 1
    ls: str | tuple = "solid"


@dataclass
class Legend:
    frame: bool = True
    show: bool = True


@dataclass
class Axis:
    label: str = None
    lim: tuple[float, float] = None
    loc: float = None
    scale: str = None


@dataclass
class Config:
    name: str = None
    data: list[Data] = None
    xax: Axis = field(default_factory=Axis)
    yax: Axis = field(default_factory=Axis)
    grid: bool = True
    legend: Legend = field(default_factory=Legend)
    show: bool = True


def plot(cfg: Config):
    pyplot.style.use("plot/main.mplstyle")
    fig, ax = pyplot.subplots(figsize=(15, 7.3))

    if cfg.xax.label is not None:
        ax.set_xlabel(cfg.xax.label)

    if cfg.yax.label is not None:
        ax.set_ylabel(cfg.yax.label)

    if cfg.data is not None:
        for data in cfg.data:
            ax.plot(
                data.x,
                data.y,
                color=data.color,
                lw=data.lw,
                ls=data.ls,
                label=data.label,
            )

    if cfg.grid:
        ax.grid(True)

    if cfg.xax.lim is not None:
        ax.set_xlim(cfg.xax.lim)

    if cfg.yax.lim is not None:
        ax.set_ylim(cfg.yax.lim)

    if cfg.xax.scale is not None:
        ax.set(xscale=cfg.xax.scale)

    if cfg.yax.scale is not None:
        ax.set(yscale=cfg.yax.scale)

    if cfg.xax.loc is not None:
        loc = ticker.MultipleLocator(base=cfg.xax.loc)
        ax.xaxis.set_major_locator(loc)

    if cfg.yax.loc is not None:
        loc = ticker.MultipleLocator(base=cfg.yax.loc)
        ax.yaxis.set_major_locator(loc)

    if cfg.legend.show:
        ax.legend(frameon=cfg.legend.frame)

    if cfg.name is not None:
        out = Path("out/surface")
        if not out.exists():
            out.mkdir(parents=True)

        fig.savefig(out / f"{cfg.name}.png", bbox_inches="tight", dpi=300)
        fig.savefig(out / f"{cfg.name}.pdf", bbox_inches="tight")

    if cfg.show:
        pyplot.show()