#!/usr/bin/env python

import numpy

from kalast.app import App, Hud

app = App()
app.config.vsync = False
app.config.render_back_face = True
app.config.access_shadow_map = True
app.config.wireframe_mode = 2
app.config.wireframe_color = [0.05, 0.05, 0.05, 1.0]
app.simulation.huds = [Hud("", size=16)]
app.simulation.sun.pos = [0.0, 20.0, 5.0]
app.simulation.camera.pos = [1.5778934, 1.9384689, 1.5082116]
app.simulation.camera.dir = [-0.54051036, -0.6640262, -0.5166407]
app.simulation.camera.up = [-0.3261482, -0.40068075, 0.85620236]
app.simulation.load_mesh(
    path="res/plane_crater_1024-5000_h=0.437.obj", mat=numpy.eye(4), flatten=True
)

sim = app.simulation

while app.running:
    it = sim.state.iteration

    a = it * 0.005
    sim.sun.pos = [0.0, 20.0 * numpy.sin(a), 20.0 * numpy.cos(a)]

    # Everything before app.step() is app.before_render()
    # Everything after is app.after_render()
    if not app.step():

    shadow = sim.facet_shadow(0)
    lit = float((shadow < 0.5).mean()) if len(shadow) else 0.0
    sim.huds[0].text = f"it={it}  lit {lit * 100:.1f} %"

    if it >= 10000:
        app.close()
