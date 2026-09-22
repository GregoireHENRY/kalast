# kalast

Kalast is a thermophysical model (TPM) for binary asteroids; it applies to
other airless bodies as well. Kalast is also an image simulator for spacecraft
cameras, in the visible and in the infrared. Its renderer serves several other
uses — viewing and interacting with meshes, generating lightcurves — see the
`examples/` folder.

## TPM

Several solvers of the heat conduction equation are implemented, all on a
variable-spacing depth grid:
- explicit: forward Euler, plus Runge–Kutta–Chebyshev super-time-stepping for
  problems where the depth system is no longer tridiagonal — lateral
  conduction, FEM, or a GPU port
- implicit: backward Euler, Crank–Nicolson, BDF2

Validated against analytical solutions (damped thermal wave, slab relaxation)
in `examples/analytical/`, and pinned by tests: error budgets, amplitude decay
and phase lag, and the observed order of accuracy — 1 for backward Euler, 2 for
Crank–Nicolson and BDF2.

The surface boundary condition includes:
- solar radiation
- self- and mutual heating (thermal re-emission and reflected sunlight)

Depth BC:
- adiabatic
- internal flux for larger bodies

Shadows, eclipses and occultations are computed by shadow mapping on the GPU
(a CPU ray-tracing path exists, and is slower).

View factors for self- and mutual heating are computed by the hemicube method
on the GPU: occlusion comes free from the depth test, one render yields a whole
row, and the result is stored sparse (0.3 % dense on the Didymos pair).
Radiosity is first order by default, with more bounces on request. Validated
to 0.07 % against the closed form for perpendicular squares.

Once surface temperatures are known, the infrared image is rendered from the
observer: emitted plus reflected flux, through a camera's resolution, field of
view, filters and spectral response function.

Surface roughness is treated twice, once per wavelength range. In the infrared,
the Kuehrt spherical-crater model corrects the emitted flux — beaming, which
makes a rough surface read hotter and flatter at low phase — with multiple
scattering after Lagerros (1996) and Mueller (2007). In the visible, Hapke's
macroscopic roughness (θ̄, 1984) enters the photometry and the lightcurves.

## Renderer

Kalast's renderer was first written to watch TPM results — surface temperatures
evolving as the bodies spin. Matplotlib and MATLAB had served before, and both
slowed the CPU simulation down.

The renderer runs on the GPU through [wgpu](https://github.com/gfx-rs/wgpu), a
pure-Rust implementation of the [WebGPU](https://gpuweb.github.io/gpuweb/)
standard, natively on every platform.

Shadow mapping uses PCF and fits its light frustum to the bounding boxes of the
bodies in the scene automatically.

Kalast is also a UI app for editing scripts and interacting with the rendered
scene, in the spirit of Blender or Unity.

## Shape models

Shape models represent a body's surface as triangular facets. Of the many
formats, kalast reads Wavefront `.obj` only.

## Getting started

Grab a release from https://github.com/GregoireHENRY/kalast/releases, unpack
it, and run it from inside the folder.

```sh
./kalast                                       # starts the kalast UI app
./kalast examples/two_spheres/main.py          # loads a Python example
./kalast examples/crater_self_shadow/step.rs   # a Rust one
./kalast some/shape.obj                        # a mesh
```

There is nothing to install and nothing is written outside the folder. The
archive carries its own Python, with kalast and its dependencies already in
it, and the `.rs` examples come pre-compiled. Editing one, or opening a `.rs`
of your own, means compiling it: if the machine has no cargo the kalast UI
app fetches a minimal toolchain into `toolchain/` beside the executable and
reuses it afterwards. On macOS that also wants Apple's command line tools for
the linker (`xcode-select --install`).

## Packages

You can also install kalast as a package, in a Python virtual environment or a
Rust project:

```sh
pip install kalast          # Python
cargo add kalast            # Rust
```

## Repo structure

- `src/`: Rust core. Written to be usable standalone by Rust users, independent
  of Python — the Python wrapper must not compromise its speed.
- `kalast/`: Python wrapper. Provides Pythonic usage of Kalast (e.g. object
  references) for users less familiar with Rust. Built with maturin.
- `shaders/`: wgpu shaders (`.wgsl`) used by the rendering pipeline.
- `examples/`: Examples of usage of Kalast. Scripts under `examples/old/`
  are earlier/superseded versions kept for reference, not maintained as
  user-facing examples.
- `res/`: resources folder.
- `out/`: default output directory for simulation results.

## If you want to clone and compile it yourself

Create a virtual environment for the dependencies and the build — I recommend
Astral's `uv` for Python. Then, from within your venv, run the following.

Build the kalast Rust extension, `kalast/_rs.abi3.so` (its name on macOS):

```sh
maturin develop
```

Build in debug (the default) while implementing features or fixing bugs. Use
`--release` for benchmarks, and once a feature works.

```sh
maturin develop --release
```

Beyond the default `opt-level = 3` there is nothing worth adding:
`lto = "fat"` + `codegen-units = 1` were measured on this project and gave
no improvement. Recorded in `Cargo.toml` so it is not retried blindly.

The UI app can also be started from the Python module, here loading an example:

```sh
python -m kalast examples/two_spheres/main.py
```

Then import kalast from Python (or add the crate from Rust) and write your own
scripts.

```python
import kalast
```
