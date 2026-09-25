# kalast

Kalast is a thermophysical model (TPM) for binary asteroids; it applies to
other airless bodies as well. Kalast is also an image simulator for spacecraft
cameras, in the visible and in the infrared. Its renderer serves several other
uses — viewing and interacting with meshes, generating lightcurves — see the
[`examples/`](examples/) folder.

Download the [latest version of kalast here](https://github.com/GregoireHENRY/kalast/releases).
You can directly run the executable and open an example script from the UI.
You can also run it in your terminal:

```sh
./kalast                                        # starts the kalast UI app
./kalast examples/crater_self_shadow/main.py    # load a Python example
./kalast examples/crater_self_shadow/main.rs    # load a Rust example
./kalast res/plane_crater_1024-5000_h=0.437.obj # load a mesh
```

More info on [Running kalast for the first time](#running-kalast-for-the-first-time).
There are also additional resources to read:
- [res/README.md](res/README.md) to get data
- [notes/API.md](notes/API.md) python API
- [notes/CONFIG.md](notes/CONFIG.md) config options
- [notes/CONTROLS.md](notes/CONTROLS.md) UI controls
- [examples/README.md](examples/README.md) examples scripts

You can also write me an email at [gregoireh@pm.me](mailto:gregoireh@pm.me) if
you have any question or want any feature added.

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

## Running kalast for the first time

Kalast carries its own Python. Rust examples `.rs` come pre-compiled.
Editing one, or opening a `.rs` of your own, means compiling it.
If the machine has no cargo the kalast UI app fetches a minimal toolchain into
`toolchain/` next to the executable. 

The bundles are not code-signed, so Windows warns once at the first launch of
`kalast.exe`: *More info*, then *Run anyway*.

macOS refuses to run a bundle downloaded with a browser until it is flagged
cleared, use: 

```sh
xattr -cr /path/to/kalast-v*-*
```

Kalast is also available as a [PyPI package](https://pypi.org/project/kalast)
and as a [crate](https://crates.io/crates/kalast).

```sh
pip install kalast          # Python -- do this in a venv, you can use astral uv
cargo add kalast            # Rust
```

## Linux requirements

- `glibc >= 2.35`

## If you want to clone and compile it yourself

- `src/`: Rust core. Written to be usable standalone by Rust users, independent
  of Python — the Python wrapper must not compromise its speed.
- `kalast/`: Python wrapper. Provides Pythonic usage of Kalast (e.g. object
  references) for users less familiar with Rust. Built with maturin.
- `shaders/`: wgpu shaders (`.wgsl`) used by the rendering pipeline.
- `examples/`: Examples of usage of Kalast. Scripts under `examples/old/`
  are earlier/superseded versions kept for reference, not maintained
- `res/`: resources folder.
- `out/`: default output directory for simulation results.

Create a virtual environment for the dependencies and the build. I recommend
Astral's `uv`. Then, from within your venv, run the following to build
kalast DLL for python `kalast/_rs.abi3.so` (its name on macOS):

```sh
maturin develop
```

Build in debug (the default) while implementing features or fixing bugs. But use
`--release` for benchmarks, and once a feature works.

```sh
maturin develop --release
```

Beyond the default `opt-level = 3` there is nothing worth adding:
`lto = "fat"` + `codegen-units = 1` were measured on this project and gave
no improvement.

The UI app can also be started from the kalast python module:

```sh
python -m kalast examples/two_spheres/main.py
```