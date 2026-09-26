# Kalast examples scripts

Kalast is shipped with its own Python.
Rust examples come pre-compiled in bundle releases.
But if you want to edit them or write your own you need `Cargo` compiler.

Here you can find info about the contents of the examples scripts.
Not everything will be detailed because they are already well detailed at:
- [docs/API.md](../docs/API.md) python API
- [docs/CONFIG.md](../docs/CONFIG.md) config options
- [docs/CONTROLS.md](../docs/CONTROLS.md) UI controls

You can also write me an email at [gregoireh@pm.me](mailto:gregoireh@pm.me) if
you have any question or want any feature added.

Examples that work directly:

- [cube](cube):
    - [cube/light.py](cube/light.py):
        - Simple cube with diffuse lighting from a light rotating around.
        - Only `.obj` mesh are supported so far, meshes are flattened by default
          to show per-facet data.
          You can change it to smooth.
        - `app.simulation.config.light.cube_show` shows the light source.
        - `app.simulation.config.axes.style = "gizmo"` shows XYZ 3D frame gizmo
          top right, the default; `"off"` hides it.
          You can change it's location and size too.
          You can also click on individual X, Y or Z and negative to select view
          plane toggling orthographic view.
          There are also other axes style options like `blender`.
        - Using wireframe `2` to show mesh + wireframe.
          Default is `0` with just mesh.
          Use `1` if you want wireframe only.
        - Setting initial camera position and direction.
          Direction is set by asking camera to look at anchor point.
          Default anchor point is origin.
        - Main while loop stepping through frames iterations.
        - Updating light position in main loop.
        - You can click on a facet to select it.
          When clicking on a facet it shows facet info.
          You can click again to unselect it.
          You can select multiple facets and the list of all selected facets is
          always printed.
          You can also change the color of the selected facet.
    - [cube/color_default.py](cube/color_default.py):
        - `app.simulation.config.shading.color_mode = 1` default value is `0`
          and auto compute diffuse lighting from light source and mesh normals.
          Using value `1` shows facet/vertex colors.
          Default color is white.
        - main while loop is not even needed here
    - [cube/color_map.py](cube/color_map.py):
        - `app.simulation.config.data.colormap = "gray"` to use
          [matplotlib gray colormap](https://matplotlib.org/stable/users/explain/colors/colormaps.html).
          You can do more customisation like this to have inferno reversed:
          `app.simulation.config.data.colormap = matplotlib.colormaps["inferno"][::-1]`
          Such details are fully explained in [docs/CONFIG.md](../docs/CONFIG.md)
          and [docs/API.md](../docs/API.md).
        - `app.simulation.config.selection.labels = True` to display facets
          indices.
        - Access loaded mesh with `mesh = app.simulation.bodies[0].mesh`.
          The mesh struct is really well written from Rust with a Pythonic
          wrapper and there are so many things you can do with it, please check
          [docs/API.md mesh](../docs/API.md#what-a-mesh-carries),
          [tests/test_mesh.py](https://github.com/GregoireHENRY/kalast/blob/main/tests/test_mesh.py)
          and
          [tests/test_mesh_intercept.py](https://github.com/GregoireHENRY/kalast/blob/main/tests/test_mesh_intercept.py)
        - `mesh.values = numpy.arange(nf, dtype=float)` goes from `0` to `11`
          cus there are `12` facets to the cube.
          The colormap will be auto scaled to min and max if they are not
          mentioned, so facet index `#0` will be black and facet index `#11`
          will be white.
    - [cube/color_custom.py](cube/color_custom.py):
        - Alternative to using colormap, you can also directly write RGB color
          to facets.
          This is an example coloring them from black to white manually from 
          facets indices.
- [crater_self_shadow](crater_self_shadow):
    - [crater_self_shadow/main.py](crater_self_shadow/main.py):
        - With default shading color mode `0`, self shadows are automatically
          computed. Mutual shadows between multiple bodies too, but this example
          only loads a single crater.
          Shadows are computed on the GPU using shadow mapping technique in
          shaders.
        - Back faces need to be rendered to see the crater from under it
          `app.simulation.config.shading.render_back_face = True`.
          By default back faces are not rendered because most of meshes are
          closed but this crater is open.
          Not rendering back faces saves time but it is necessary here.
          Facets normals are pointing upward inside the crater.
        - `app.simulation.config.shadows.access_shadow_map = True` to request
          access to the shadows computed in the GPU shaders.
          By default this data is not shared between the GPU and CPU in the
          pipeline to save time.
        - `app.simulation.config.shadows.resolution = 8192` to change the
          resolution of the shadow mapping.
          Default is `4096` which might not be enough for detailed cases but
          runs really fast for fast iterations.
        - `app.simulation.config.shadows.pcf = 2` play with cautious with this,
          increase the value too much could yield strange results but low values
          like `2` or `4` allows to smooth shadows.
          PCF results is affected by shadow resolution.
          You see larger effects at lower shadow resolutions.
        - `app.simulation.huds = [kalast.app.Hud("", size=16)]` let's  you
          create an HUD completely customizable in position/size/font/alignment.
          You can create multiple HUDs.
          This one is empty string so far but it is being updated in the while
          loop.
        - The sun position is updated before `app.step()` in the main while
          loop.
          Doing one iteration step renders the scene as is.
          Once rendered, you can ask the percent of illuminated and shadowed
          facets, and update the HUD.
    - [crater_self_shadow/fn.py](crater_self_shadow/fn.py):
        - The function way of calling kalast.
          This is up to your preference if you prefer to use `app.step()` in a
          while loop to step manually or to let kalast own the main blocking
          loop and call `app.before_render()` and `app.after_render()`. 
          You can also write functions as lambdas.
    - [crater_self_shadow/main.rs](crater_self_shadow/main.rs) and [crater_self_shadow/fn.rs](crater_self_shadow/fn.rs):
        - Same example scripts but written in rust. They come pre-compiled but
          you are free to update and re-compile them.
- [two_spheres/main.py](two_spheres/main.py):
    - Showcase with two bodies and mutual shadows.
    - The second body is loaded with a custom 4D model matrix to change its
      position and size.
    - Before the main loop starts, a rotating matrix is created around the Z
      axis.
    - In the main loop, if the simulation is not paused, the rotating matrix is
      applied to the second body model matrix.
- [mesh/decimate.py](mesh/decimate.py):
    - Use it like `python examples/mesh/decimate.py IN.obj OUT.obj 10000` to
      decimate `IN.obj` from its current size to `10000` facets and save it at
      `OUT.obj` path.
      It uses pymeshlab to do this.
- [misc/cbar.py](misc/cbar.py):
    - Quick example to create a colorbar.
      
      
Examples where full data is not shipped and you have to get your hands on the
data:

- Look at [res/README.md](../res/README.md) to get Hera data.
- [didymos/main.py](didymos/main.py):
    - Load Didymos and Dimorphos shape models.
    - Showcase kalast loading and rendering speed with 2x 3M+ facets meshes.
    - Use SPICE to set Didymos/Dimorphos and Sun positions/orientations.
    - You should run with decimated mesh `10k` or `100k` and compare speed.
- [hera_didymos](hera_didymos):
    - [hera_didymos/afc.py](hera_didymos/afc.py):
        - Load Didymos/Dimorphos/Sun/Earth in AFC FOV frame centered on AFC
          using camera dir and up vectors from the kernels.
          The camera can't really be moved manually between the steps to
          inspect around but you can always change camera pos/anchor point.
        - Window is set at AFC resolution so it's 1 to 1 pixel simulation of
          what AFC will see at the corresponding dates.
        - If you want to export frames and then compile later yourself into
          a movie, you use `app.simulation.export_once()` in the main loop.
          This requests kalast to export the rendering to a 1 to 1 pixels
          PNG in `out/frames` folder.
          Images are numbered automatically, you need to clear the folder
          yourself before a future execution or you can also change the 
          folder where kalast exports.
    - [hera_didymos/afc_eclip_didy.py](hera_didymos/afc_eclip_didy.py):
        - Same as [hera_didymos/afc.py](hera_didymos/afc.py) but everything
          is simulated in ECLIPJ2000 frame centered on Didymos.
          This way it is easier to rotate around the camera to inspect.
          Both scripts produce the same output.
- [landmark_tracking/main.py](landmark_tracking/main.py):
    - Load camera/sun and bodies info positions/orientations from a CSV file
      provided by GUBAS instead of SPICE kernels.
    - Create random landmarks locations by selecting facets.
    - Export landmarks screen-space X/Y positions in CSV file.

Examples that are on-going work not ready for users:

- [analytical](analytical)
- [hera_didymos](hera_didymos):
    - [hera_didymos/heating_preflight.py](hera_didymos/heating_preflight.py)
    - [hera_didymos/tiri_fits.py](hera_didymos/tiri_fits.py)
    - [hera_didymos/tiri_movie_compose.py](hera_didymos/tiri_movie_compose.py)
    - [hera_didymos/tiri_movie.py](hera_didymos/tiri_movie.py)
    - [hera_didymos/tpm_phase2.py](hera_didymos/tpm_phase2.py)
    - [hera_didymos/tpm.py](hera_didymos/tpm.py)
- [hera_mars_swingby](hera_mars_swingby)
- [lightcurve](lightcurve)

Legacy examples in [old](old).