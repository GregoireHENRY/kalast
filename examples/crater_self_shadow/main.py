#!/usr/bin/env python

import numpy

import kalast
from kalast.app import App, Hud


def before_render(sim: kalast.app.simulation.Simulation, dt: float):
    pass


app = App()
app.config.vsync = False
app.config.debug_light_cube_show = True
app.config.render_back_face = True
app.config.wireframe_mode = 2
app.config.wireframe_color = [0.05, 0.05, 0.05, 1.0]
app.config.shadow_pcf = 8
app.simulation.huds = [
    Hud("it={it}/{nit} fps={fps} {paused}", size=14),
]
app.simulation.sun.pos = [0.0, 20.0, 5.0]
app.simulation.camera.pos = [1.5778934, 1.9384689, 1.5082116]
app.simulation.camera.up = [-0.3261482, -0.40068075, 0.85620236]
app.simulation.camera.dir = [-0.54051036, -0.6640262, -0.5166407]

mat = numpy.eye(4)
app.simulation.load_mesh(
    path="res/plane_crater_1024-5000_h=0.437.obj", mat=mat, flatten=True
)

app.before_render = before_render
app.start()
