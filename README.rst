======
kalast
======

Kalast is a thermophysical model (TPM) for binary asteroids, used to simulate
images as they would be seen from a spacecraft.

Shape models are made of triangular facets. The TPM is a forward finite
difference solver of the heat conduction equation, with boundary conditions:

1. at the surface, from solar radiation, mutual heating with a secondary
   body (thermal reflection and emission), and self-heating (with extra
   constraints for neighboring facets, e.g. shadowing/visibility);
2. at depth, using either an adiabatic condition or an internal flux
   condition for larger bodies (e.g. the Moon).

The surface boundary condition only applies to airless bodies (asteroids,
moons) without an atmosphere.

The image simulator renders shape models with a customizable observer
frustrum (position/specs), to reproduce images taken from a spacecraft. It
uses a custom wgpu + shader rendering pipeline. Images can be simulated in
visible light (diffuse lighting shader) or infrared (using TPM surface
temperature output and an infrared flux simulation — emission and
reflection — based on a thermal camera's specifications, e.g. its spectral
response function).

Structure
=========

- `kalast/`: Python wrapper. Provides Pythonic usage of Kalast (e.g. object
  references) for users less familiar with Rust. Built with maturin.
- `src/`: Rust core of the simulation. Written to be usable standalone by
  Rust users, independent of Python — the Python wrapper must not compromise
  its speed.
- `shaders/`: wgpu shaders (`.wgsl`) used by the rendering pipeline.
- `examples/`: Examples of usage of Kalast. Scripts under `examples/old/`
  are earlier/superseded versions kept for reference, not maintained as
  user-facing examples.
- `res/`: resources folder (if missing get it from cloud-as.oma.be).
- `out/`: default output directory for simulation results.

Compilation
===========

Build the kalast rust dynamic library `kalast/_rs.cpython-314-darwin.so` (example for Mac)

.. code:: sh

    uv run maturin develop

Run an example.

.. code:: sh

    uv run python -i examples/two_spheres/illum.py

Import kalast from Python.

.. code:: python
   
   # uv run python
   import kalast;