#!/usr/bin/env python

import numpy

import kalast


def tick(sim: kalast.app.simulation.Simulation, dt: float):
    pass


app = kalast.app.App()

# see app/config.rs and app/uniform.rs for param values
# Mode 0 is for diffuse lighting shader + shadow mapping
app.config.color_mode = 0

# Shadow are computed using shadow mapping tehcnique.
# Bias artefact can appear, the 3 following parameters are here to help reduce it.
# In theory these params and both camera/sun frustrums should be computed and
# updated automatically, we will implement that in future.

# Ideally keep this value to 0.0 or as close as possible to 0.0.
# But you can increase this value to allow smaller bias before acne appears.
# Value too large will cause jagged/sawtooth shadow terminator.
app.config.shadow_normal_offset_scale = 2e-4

# You want this value as low as possible until shadow acne appears.
# Value too large will create petter-panning effect.
# Increase until you observe petter-panning.
app.config.shadow_bias_scale = 1e-5

# Increase to remove global shadow acne artifact and erroneous self-shadowing.
app.config.shadow_bias_minimum = 1e-5

# app.config.shadow_pcf = 2

app.config.debug_light_cube_show = True
app.simulation.sun.pos = [0.0, 20.0, 5.0]
# app.simulation.sun.up = [0.0, 1.0, 0.0]
app.simulation.sun.look_anchor()
app.simulation.sun.projection.side = 0.5
app.simulation.sun.projection.near = 10.0
app.simulation.sun.projection.far = 30.0

# app.simulation.camera.pos = [0.0, 5.0, 0.0]
# app.simulation.camera.up = [0.0, 0.0, 1.0]
# app.simulation.camera.look_anchor()

app.simulation.camera.pos = [1.5778934, 1.9384689, 1.5082116]
app.simulation.camera.up = [-0.3261482, -0.40068075, 0.85620236]
app.simulation.camera.dir = [-0.54051036, -0.6640262, -0.5166407]


mat = numpy.eye(4)
app.simulation.load_mesh(
    path="res/plane_crater_1024-5000_h=0.437.obj", mat=mat, flatten=True
)

app.tick = tick
app.start()
