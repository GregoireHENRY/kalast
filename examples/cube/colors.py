#!/usr/bin/env python
"""Colouring individual facets — there are three ways, for three questions.

Pick by what you have, not by preference:

1. **A number per facet** — a temperature, an insolation, a shadowed fraction.
   Set `mesh.values` and a colormap, and let the renderer map it. This is what
   the example below does, and what you almost always want for anything
   scientific: the colour bar and the surface read the same lookup table, so
   they cannot disagree about what a colour means.

2. **An arbitrary colour per facet**, answering to nothing in particular — a
   category, a hand-picked scheme. Write `mesh.colors` directly, three vertices
   per facet on a flattened mesh:

       for k in range(3):
           mesh.colors[iface * 3 + k, :] = (r, g, b)

   Still the right tool when the colour is not a function of a number.

3. **A handful of facets to mark**, on an otherwise normally lit body — a
   selection, one bad facet. Write `mesh.colors` *and* set
   `mesh.color_modes[...] = 1` on the same vertices: those facets are drawn
   flat in their own colour while everything else stays lit. This is the only
   one of the three that does not force the whole body unlit.

Two things that catch people out, both about *when*:

- Colours written after `app.start()` need `mesh.mark_colors_dirty()` to be
  uploaded. Written before, as here, the first upload carries them.
- `color_mode = 1` means "the colour is the data", not "data times lighting".
  Leave it at 0 and a colormap gets multiplied by the diffuse term, so the
  unlit side of a body reads black whatever its value is.
"""

import numpy

import kalast


app = kalast.app.App()

# The colour *is* the measurement here, so it must not be shaded. See the note
# above: at color_mode 0 this cube's shaded side would come out black
# regardless of the values on it.
app.simulation.config.color_mode = 1
app.simulation.config.wireframe_mode = 2

app.simulation.config.colormap = "viridis"
app.simulation.config.colorbar = True
app.simulation.config.colorbar_label = "facet index"

app.simulation.camera.pos = [10.0, 0.0, 0.0]
app.simulation.camera.dir = [-1.0, 0.0, 0.0]

app.simulation.load_mesh(path="res/cube.obj", mat=numpy.eye(4), flatten=True)
mesh = app.simulation.bodies[0].mesh
nface = len(mesh.facets)

# One number per facet. The range is fitted to the data unless value_min and
# value_max are set, so this needs no scaling by hand -- and the bar is
# labelled in these units, not in 0..1.
mesh.values = numpy.arange(nface, dtype=float)

while app.running:
    app.step()
