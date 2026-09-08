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

    # Insolation, not occlusion: a facet with nothing between it and the Sun
    # is still dark if it faces away, which on this crater is most of the far
    # wall. `facet_shadow` alone answered the wrong question.
    illum = sim.facet_illumination(0)
    lit = float((illum > 0).mean()) if len(shadow) else 0.0
    sim.huds[0].text = f"it={it}  lit {lit * 100:.1f} %"


app = App()
app.simulation.config.vsync = False
app.simulation.config.render_back_face = True
app.simulation.config.debug_light_cube_show = True
app.simulation.config.access_shadow_map = True
app.simulation.config.wireframe_mode = 2
app.simulation.config.wireframe_color = [0.05, 0.05, 0.05, 1.0]
# app.simulation.config.shadow_pcf = 8
# app.simulation.config.axes = "blender"
# app.simulation.config.colorbar = True
app.simulation.huds = [
    Hud("it={it}/{nit} fps={fps} {paused}", size=14),
]
app.simulation.sun.pos = [0.0, 20.0, 5.0]
app.simulation.camera.pos = [1.5778934, 1.9384689, 1.5082116]
app.simulation.camera.up = [-0.3261482, -0.40068075, 0.85620236]
app.simulation.camera.dir = [-0.54051036, -0.6640262, -0.5166407]
app.simulation.load_mesh(
    path="res/plane_crater_1024-5000_h=0.437.obj", mat=numpy.eye(4), flatten=True
)

# See step.py for a different way of starting kalast.
# app.start() blocks, and is up to 30% faster than app.step() depending on load
app.before_render = before_render
app.after_render = after_render
app.start()
