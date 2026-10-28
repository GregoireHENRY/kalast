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

## Examples that work directly

- [cube](cube):
    - [cube/light.py](cube/light.py):
        - Simple cube with diffuse lighting from a light rotating around.
        - Only `.obj` mesh are supported so far, meshes are flattened by default
          to show per-facet data.
          You can change it to smooth.
        - `app.simulation.config.light.cube_show` shows the light source.
        - `app.simulation.config.axes.style = "gizmo"` shows XYZ 3D frame gizmo
          top right by default; use `"off"` to hide it.
          You can change it's location and size too.
          You can also click on individual X, Y or Z and negative to select view
          plane toggling orthographic view.
          There are also other axes style options like `blender`.
        - Wireframe mode `2` is the default to show facets + wireframe.
          Use mode `0` to show just facets.
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
        - `app.simulation.config.shading.color_mode = "data"`: the default,
          `"shaded"`, computes diffuse lighting from the light source and the
          mesh normals; `"data"` shows the facet/vertex colors as they are.
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
        - `app.simulation.config.shadows.pcf = 2` smooths the shadows' edges
          (the default); `0` draws them hard, larger values blur more.
          The blur is counted in shadow-map texels, so it is wider at lower
          shadow resolutions.
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
- [sphere](sphere):
    - [sphere/main.py](sphere/main.py):
        - A sphere with spin period 6h.
        - Only rendering diffuse lighting.
        - Try the two different sphere meshes: icosphere and sphere (triangulated from quad method fixed)
    - [sphere/tpm.py](sphere/tpm.py):
        - Simple TPM of the sphere at with Sun at 1 AU.
        - Colorbar inferno.
        - Thermophysical property of average S-type asteroid.
        - Adiabatic at 1x 2pi spin skin depth.
        - Max time step for stability.
        - Initialization with effective temperature at given latitudes.
        - Simple spin without obliquity, no solar orbit.
        - For each time step in the while loop, vectorized per facet:
            - Surface boundary condition with solar radiation.
            - Adiabatic condition at depth boundary.
            - 1D heat conduction.
    - [sphere/tpm_obliquity.py](sphere/tpm_obliquity.py):
        - Simple sphere TPM with obliquity (tilt of the spin axis).
    - [sphere/tpm_logo.py](sphere/tpm_logo.py):
        - Kalast logo was created in the past using a similar script. I was
          observing the evolution of the surface temperature of an oblate sphere
          resized to the extents of Didymos, using Didymos thermophysical
          and orbital properties.
        - Seasonal effects are simulated by taking 1x 2pi yearly skin depth and
          Didymos obliquity.
        - Solar orbit with Keplerian parameters.
        - Each column starts at the temperature its facet's mean sunlight over
          an orbit gives it, rather than 0 K.
        - After 30 Didymos years, the temperatures have converged down to the
          bottom of the columns (yearly polar variations and thermal printing
          at depth boundary).
        - Several TPM steps are simulated between rendering steps to go faster.
        - Toggle off the wireframe and you have kalast logo re-generated.
    - [sphere/tpm_variable_1.py](sphere/tpm_variable_1.py):
        - Varying surface and depth thermophysical properties.
        - Regolith over rock.
        - A brighter patch and bare rock in circular areas given by latitude,
          longitude and radius in degrees.
    - [sphere/tpm_variable_2.py](sphere/tpm_variable_2.py):
        - Another way: ponds of dust fading into compact ground, and each
          column turning from its own surface to a deep material with depth,
          as regolith compacts.
    - [sphere/tpm_plot.py](sphere/tpm_plot.py):
        - Run it after any sphere TPM script above: each saves, when it
          stops, the columns under the facets of the meridian at longitude 0
          in `out/sphere/`, and this draws the last one in a PNG it opens.
        - Surface temperature over the last two spins at each latitude, north
          solid and south dashed, and for `tpm_logo.py`, which has an orbit,
          over the last two years beside them.
        - The equator's and the poles' columns spin after spin, with how much
          they still move (converged when next to nothing), and through the
          last spin: the daily wave going down, lagging and dying out with
          depth; for `tpm_logo.py`, through the last year as well, the
          seasons' wave down the whole column.
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
      
## Examples that work after you get data and fix paths

Look at [res/README.md](../res/README.md) to get Hera data.

- [didymos/main.py](didymos/main.py):
    - Load Didymos and Dimorphos shape models.
    - Showcase kalast loading and rendering speed with 2x 3M+ facets meshes.
    - Use SPICE to set Didymos/Dimorphos and Sun positions/orientations.
    - You should run with decimated mesh `10k` or `100k` and compare speed.
    - `app.simulation.config.shading.color_mode = "shaded"`, the default: lit
      by the Sun, with shadows.
    - `app.simulation.config.shading.srgb_mode = "linear"`: an exported pixel
      is `exposure * I/F * 255`, I/F being the brightness relative to a white
      surface facing the Sun, as a camera records it. The default `"screen"`
      stores it through the sRGB curve, as a screen shows light: natural to
      the eye, but a pixel no longer reads I/F. The data mode stores its
      colours as given either way.
    - `app.simulation.config.light.exposure = 5.0`: the light scaled as a
      camera's exposure does, for a dark surface to show.
    - `body.scattering = Hapke(...)`: how the surface reflects light, here the
      Hapke law Buratti et al. (2024) fitted to DART's images of the system,
      the colour left at 1, the law's own albedo. Without a law a body is
      Lambertian, its colour the albedo, white by default. Other laws in
      [docs/API.md](../docs/API.md#kalastscattering--reflected-sunlight).
- [lightcurve_didymos](lightcurve_didymos): Didymos and Dimorphos one
  night from an observatory on Earth, their mutual events in it. Its
  [README](lightcurve_didymos/README.md) says where to get the data.
    - [lightcurve_didymos/lightcurve.py](lightcurve_didymos/lightcurve.py):
      the light curve. TRAPPIST-North on 2022-11-18, the bodies placed from
      DART's SPICE kernels at the time their light left,
      `kalast.lightcurve.flux` each epoch in a plain loop, the night
      plotted -- with DART's shape models (`meshes`) or Scheirich et al.'s
      spheroids from `res/ico5.obj` (`ellipsoids`), `SHAPES` at its top;
      apparent magnitudes or reduced to unit distances and one phase angle,
      `MAGNITUDES`.
    - [lightcurve_didymos/scene.py](lightcurve_didymos/scene.py): the same
      night drawn, as the observatory looks at it, north up, orthographic,
      with either shape model -- the poses set from SPICE every frame, the
      time in a HUD, the frame rate capped (`sim.state.rate_limited`), the
      spheroids a sphere stretched by its pose.
    - [lightcurve_didymos/scene_lightcurve.py](lightcurve_didymos/scene_lightcurve.py):
      the two in one -- the night drawn and its light curve made as it goes,
      each frame's light summed on the GPU from the frame itself
      (`config.photometry`), the plot written and opened at the night's end.
    - [lightcurve_didymos/telescope.py](lightcurve_didymos/telescope.py):
      the night as TRAPPIST-North recorded it -- drawn for photometry, each
      frame's own light making a FITS frame on the GPU (`sim.expose`), the
      field's stars from Gaia, every effect of a real frame switchable --
      and its reduction from the frames: aperture photometry against the
      comparison stars, the measured Rc beside the rendered.
- [hera_didymos](hera_didymos) ([README](hera_didymos/README.md)): Hera's
  cameras at Didymos, at the times of AFC's and TIRI's images.
    - [hera_didymos/afc.py](hera_didymos/afc.py): what AFC saw -- each AFC-1
      image drawn in the products' rows and columns, a pixel the I/F of the
      ground it sees (kalast's reference image), written as FITS -- set
      beside the images: the pointing's error, Didymos's size and signal,
      Dimorphos beside it, in `out/hera_didymos/afc`. Without images, from
      `START` to `END` every 15 minutes.
    - [hera_didymos/tiri.py](hera_didymos/tiri.py): what TIRI saw -- kalast's
      thermophysical model run before the first image, each image drawn in
      its filter, a pixel the radiance of the ground it sees, written as
      FITS -- set beside the images: the pointing's error, Dimorphos beside
      Didymos, each one's signal, in `out/hera_didymos/tiri`.
    - [hera_didymos/coverage.py](hera_didymos/coverage.py): Hera's distance
      to Didymos and Dimorphos, the phase angle, the sub-spacecraft points
      and AFC's and TIRI's resolution at the images' times, and maps of the
      ground each instrument saw.
    - [hera_didymos/afc_eclip_didy.py](hera_didymos/afc_eclip_didy.py): AFC
      from November 2026 to April 2027 every 15 minutes, simulated in
      ECLIPJ2000 centred on Didymos, where the camera is easier to move
      around.
- [hera_mars_swingby](hera_mars_swingby)
    - [hera_mars_swingby/tiri_diffuse_light.py](hera_mars_swingby/tiri_diffuse_light.py):
      - Simulate TIRI image similar to [hera_didymos/afc.py](hera_didymos/afc.py)
        but for Mars swing-by.
      - Apply flip Y axis to match real TIRI image already generated.
      - Loading Mars DTM with features 10x exagerated, Deimos, and Phobos
      - Can apply a 10x `LARGER` matrix to Deimos and Phobos to identify easier
        where they are.
      - Adaptative time step to observe Deimos closest images.
- [landmark_tracking/main.py](landmark_tracking/main.py):
    - Load camera/sun and bodies info positions/orientations from a CSV file
      provided by GUBAS instead of SPICE kernels.
    - Create random landmarks locations by selecting facets.
    - Export landmarks screen-space X/Y positions in CSV file.
- [phobos_shadows_fit](phobos_shadows_fit): four Mars Express HRSC SRC
  images of Phobos, drawn by kalast and compared with the real ones. Its
  [README](phobos_shadows_fit/README.md) says where to get the data.
    - [phobos_shadows_fit/simple.py](phobos_shadows_fit/simple.py):
        - One of the four images, picked in `IMAGES`: its time, the pointing
          correction in pixels -- where the image's craters put Phobos
          against SPICE's pointing, measured by `compare.py` -- and how many
          pixels Phobos drifted during the exposure.
        - `pck00011.tpc` loaded after the meta-kernel: Phobos's rotation in
          the frame of Ernst et al.'s shape model.
        - Positions from SPICE as seen from Mars Express, corrected for
          light time and aberration (`"LT+S"`), each body turned as it was
          when its light left it (`et - lt`).
        - The SRC is 1008 x 1018 pixels of a 1024 x 1024 CCD, 9 micron
          pixels behind a 984.76 mm focal length: the image's size, and
          `fovy` the lines times a pixel's angle. The camera looks along
          the boresight, turned to the image's centre on the CCD and by the
          pointing correction.
        - `app.simulation.config.light.exposure = 40.0`, the light linear as
          for Didymos: Phobos is dark, its
          lit ground at I/F 0.004-0.021 in h8963, so 40 brings it to
          0.17-0.85 of white without clipping. Its night side, lit by Mars,
          is around 0.003.
        - Not in the script: `app.simulation.config.stretch.scale = "sqrt"`
          with `stretch.limits = "percent"` shows the image as `compare.py`'s
          figures do, the square root of its brightest 99.5 %, the night side
          and the sky included. Other scales and limits are SAO DS9's. An
          exported pixel is then the stretched value, not I/F.
        - `app.simulation.config.light.sun_as_point = False`: the Sun as a
          disc, shadows with penumbrae. The default `True` is a point.
        - `app.simulation.config.shadows.method = "reference"`: the image
          averaged over many samples a pixel, until it stops changing
          (`reference.error`) or at `reference.max_samples`. The toolbar
          shows how far it is and can stop it. `"maps"`, the default, and
          `"rays"` draw in real time, without bounced light.
        - `app.simulation.config.reference.bounces = 1`: sunlight reflected
          once more by the surfaces, Mars's light on Phobos's night side.
          The default `0` is the direct light alone, twice as fast.
        - `app.simulation.config.response`: the camera's response, fitted to
          these images. A blur of 2.19 pixels, two halos spreading 1.2 % of
          the light over about 53 pixels and 28 % over about 434, which
          light the images' sky and shadows, and the smear. `enabled =
          False` shows the image without it.
        - Phobos's law is the Hapke law Fornasier et al. (2024) fitted to
          HRSC images.
        - Mars is a sphere stretched to its radii in its model matrix, of
          colour 0.25, near its albedo, there for its light on Phobos.
    - [phobos_shadows_fit/main.py](phobos_shadows_fit/main.py): the four
      images in a loop, each exported once `sim.reference_done`, with
      options for the shadow method, the law and the shape model, listed in
      its docstring. `albedo` colours Phobos with
      `mesh.colors_from_map` from a map fitted to these images
      ([phobos_src_albedo.npz](phobos_shadows_fit/phobos_src_albedo.npz)).
    - [phobos_shadows_fit/compare.py](phobos_shadows_fit/compare.py): the
      images beside kalast's -- shadows as lit-or-not masks, pointing from
      the craters, brightness through the SRC's response in
      [hrsc.py](phobos_shadows_fit/hrsc.py) -- figures and a table in
      `out/phobos_hrsc/figures`.
- [comet67p_mesh_and_shadows](comet67p_mesh_and_shadows): comet
  67P/Churyumov-Gerasimenko from Rosetta's OSIRIS narrow-angle camera: its
  shape made from 354 of its images, and 57 of them drawn as Phobos's are,
  2014 to 2016, phase 2 to 120 deg. Its
  [README](comet67p_mesh_and_shadows/README.md) says where to get the data.
    - [comet67p_mesh_and_shadows/mesh.py](comet67p_mesh_and_shadows/mesh.py):
      the shape from the images and the kernels alone, no shape model to
      start from -- the frames of
      [frames.csv](comet67p_mesh_and_shadows/frames.csv) fetched
      (`fetch`), their silhouettes carved into a closed hull (`hull`),
      maplets over it at 40, 20 and 10 m, the mesh made again from them
      each round (`tiles`), the result against SHAP7 (`compare`).
    - [comet67p_mesh_and_shadows/shadows.py](comet67p_mesh_and_shadows/shadows.py):
      the images download themselves from ESA's PSA on first use
      ([osiris.py](comet67p_mesh_and_shadows/osiris.py)), drawn on Preusker
      et al.'s SHAP7 shape model of 44M facets. `hapke-fit` is a Hapke law
      refitted to these images. `spin=DEG` turns the nucleus about its spin
      axis and `light=DEG` the light alone, to test the rotation against
      the shadows. `bounces=N` lights the ground in shadow by the comet's
      own light. `response` draws the images as the NAC records them,
      through its response fitted to them, and `albedo` gives each facet
      the albedo these images found where it is, from a map of the surface
      ([67p_albedo.npz](comet67p_mesh_and_shadows/67p_albedo.npz)): a
      colour per facet from a few lines of numpy.
    - [comet67p_mesh_and_shadows/compare.py](comet67p_mesh_and_shadows/compare.py):
      as for Phobos, with one sheet of all the images sorted by phase, in
      `out/67p_osiris/figures`.

## Examples that worked but not ready for users

- [analytical](analytical)
- [hera_didymos](hera_didymos):
    - [hera_didymos/heating_preflight.py](hera_didymos/heating_preflight.py)
    - [hera_didymos/tiri_fits.py](hera_didymos/tiri_fits.py)
    - [hera_didymos/tiri_movie_compose.py](hera_didymos/tiri_movie_compose.py)
    - [hera_didymos/tiri_movie.py](hera_didymos/tiri_movie.py)
    - [hera_didymos/tpm_phase2.py](hera_didymos/tpm_phase2.py)
    - [hera_didymos/tpm.py](hera_didymos/tpm.py)
- [hera_mars_swingby](hera_mars_swingby)

## Legacy examples

in [old](old)