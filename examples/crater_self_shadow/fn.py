#!/usr/bin/env python

import numpy

from kalast.app import App, Hud, Simulation


def before_render(sim: Simulation, dt: float) -> None:
    it = sim.state.iteration
    if it >= 10000:
        return

    a = it * 0.005
    sim.sun.pos = [0.0, 20.0 * numpy.sin(a), 20.0 * numpy.cos(a)]


def after_render(sim: Simulation, dt: float) -> None:
    it = sim.state.iteration
    if it >= 10000:
        return

    illum = sim.facet_illumination(0)
    lit = float((illum > 0).mean()) if illum is not None else 0.0
    sim.huds[0].text = f"lit {lit * 100:.1f} %"


app = App()
app.simulation.config.axes.style = "gizmo"
app.simulation.config.light.cube_show = True
app.simulation.config.shading.render_back_face = True
app.simulation.config.shadows.access_shadow_map = True
app.simulation.config.wireframe.mode = 2
app.simulation.config.wireframe.color = [0.05, 0.05, 0.05, 1.0]
# app.simulation.config.shadows.resolution = 8192
# app.simulation.config.shadows.pcf = 2
app.simulation.huds = [Hud("", size=16)]
app.simulation.camera.pos = [1.5, 2.0, 1.5]
app.simulation.camera.look_anchor()
app.simulation.load_mesh(path="res/plane_crater_1024-5000_h=0.437.obj")

# See main.py to call kalast in main while loop iteration by iteration.
app.before_render = before_render
app.after_render = after_render
app.start()
