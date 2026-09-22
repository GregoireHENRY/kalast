# kalast

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

## Getting it

Three ways in, and only the third needs anything installed.

**A release.** https://github.com/GregoireHENRY/kalast/releases — one archive
per platform. Unpack it and run it from inside the folder:

```sh
./kalast                                   # the kalast UI app
./kalast examples/two_spheres/main.py      # a Python example
./kalast examples/crater_self_shadow/step.rs   # a Rust one
./kalast some/shape.obj                    # a mesh
```

There is nothing to install and nothing is written outside the folder. The
archive carries its own Python, with kalast and its dependencies already in
it, and the `.rs` examples come compiled. Editing one, or opening a `.rs`
of your own, means compiling it: if the machine has no cargo the kalast UI
app fetches a minimal toolchain into `toolchain/` beside the executable and
reuses it afterwards. On macOS that also wants Apple's command line tools for
the linker (`xcode-select --install`).

**The package**, to use kalast from your own environment:

```sh
pip install kalast          # Python
cargo add kalast            # Rust
```

**A clone**, to work on kalast itself — that is what *Compilation* below is
about. If you are reading this inside an unpacked release, it is not for you.

## Structure

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

## Compilation

Create a virtual environment to install dependencies and compile code, I recommend astral uv for Python.
Then, from within you venv, run the following.

Build the kalast rust dynamic library `kalast/_rs.cpython-314-darwin.so` (example for Mac).

```sh
maturin develop
```

Use `--release` for anything measured or run in earnest -- debug is 2-15x
slower, worst on the per-pixel frame-export loops (measured 22.6 -> 53.1 it/s
at 3.1M facets with export on). Keep plain `maturin develop` while
implementing a feature, and rebuild with `--release` once it works.

```sh
maturin develop --release
```

Beyond the default `opt-level = 3` there is nothing worth adding:
`lto = "fat"` + `codegen-units = 1` was measured on this project and gave
no improvement (render loop 52.7 vs 55.1 it/s, mesh load+flatten 1.07 vs
1.10 s, both inside run-to-run noise) while pushing the build from ~59 s to
~87 s. Recorded in `Cargo.toml` so it is not retried blindly.

Run a Python example.

```sh
python -i examples/two_spheres/main.py
```

Import kalast from Python and start writing your own scripts.

```python
import kalast
```
