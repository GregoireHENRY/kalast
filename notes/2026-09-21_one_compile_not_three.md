# One compile of the engine, not three

2026-09-21, after v0.5.2. Building a release bundle compiled kalast and its
~200 dependencies **three times**, verified from a local build log:

| step | what | time | why it could not reuse the previous one |
|---|---|---|---|
| wheel | `maturin build` | 38 s | first build |
| executable | `cargo build --bin kalast` | 1m 18s | different `RUSTFLAGS` (`-L`, rpath) |
| hosted examples | `--precompile` | 1m 21s | own target dir, different `-L` path, different rpath |

The wheel is a different link of the same crate and, in CI, a separate
parallel job that PyPI needs anyway. The executable is the product. The
third was waste, and it is gone: the guest build now takes **18 s and
compiles 3 crates** -- kalast and the two 30-line wrappers -- against 1m 21s
and 30 before.

## What had to be identical

Cargo fingerprints every crate on its flags, features and resolved
dependency versions. For the guest to find the engine already built, all of
them had to match the executable's build, and four things stood in the way.

**The target directory.** `build_dir()` deliberately gives hosted builds a
directory of their own, so a `python`-featured guest cannot land beside a
developer's plain build. `KALAST_HOSTED_TARGET_DIR` overrides it; the
release workflow points it at the executable's `target/`. Never set in a
user's editor, where the default is where `is_current` and the shipped
libraries agree to meet.

**The `-L` path.** The executable was linked against `runtime/python` and the
guest against `dist/<name>/python`, after the move. The bundle is now built
under a stable name, `dist/kalast-<target>`, with the interpreter already
inside it, and renamed to carry the version only at the end. Every rpath is
relative, so the rename is safe -- and the stable name is also what keeps
`Swatinem/rust-cache` valid across runs, which `dist/kalast-dev-<sha>` had
been defeating on every push.

**The rpath.** The executable wants `<origin>/python/lib`; a hosted library,
four directories down, wants `<origin>/../../../../python/lib`. Both
binaries now get both, in one fixed order, written identically by the
workflow and by `bundled_python_rustflags`. An extra search path is
harmless; a differing flag list is a full rebuild.

**The lockfile.** This was the one the first attempt missed. The wrapper is a
workspace of its own, so it resolved a lockfile of its own, and a fresh
resolution picked newer patches than the root's:

```
egui        0.36.1 -> 0.36.2
egui-winit  0.36.1 -> 0.36.2
font-types  0.12.4 -> 0.12.5
```

Different versions are different crates, and everything downstream of the
difference recompiled -- naga and wgpu-core among the thirty -- with flags,
features and interpreter all matching. `write_wrapper` copies the root's
`Cargo.lock` beside the wrapper now, from a source tree; a bundle has no
root lock and nothing to share. Pinned by the wrapper test.

The one crate still compiled twice is kalast itself, about 15 s: cargo
hashes a workspace root's own library differently from the same package as
a path dependency. Not worth chasing.

Windows keeps its second compile. The guest's `-L` comes out of
`current_exe` with backslashes and the host's out of bash with slashes, so
the strings differ. Correct, just slower.

## And the feature split, which the collapse forced

For the guest to share the executable's artefacts it has to be built with
the executable's *exact* features -- and "python" was not exact enough. It
bundled `pyo3/auto-initialize`, which is right for a binary that starts an
interpreter and wrong for an extension module loaded into one, and pyo3
treats the two as mutually exclusive.

```
python = ["dep:pyo3", "dep:numpy"]            # the bindings; says nothing about linking
embed  = ["python", "pyo3/auto-initialize"]   # a binary carrying an interpreter (default)
ext    = ["python", "pyo3/extension-module"]  # a module loaded into one (pyproject.toml)
```

`kalast_dependency` gives the guest whichever the host has, and an `ext`
host -- `python -m kalast` hosting a `.rs` -- gets `-undefined
dynamic_lookup` on macOS and the running interpreter as `PYO3_PYTHON`, since
its guest must link no libpython either.

**This closed a latent bug in every wheel on PyPI.** Without
`extension-module`, pyo3 links whatever libpython the build machine's Python
reports, by absolute path. python-build-standalone's `bin/python3` has its
interpreter linked in *statically* while reporting `Py_ENABLE_SHARED=1`, so
an extension that brings a second one segfaults on import -- which is what a
locally built wheel did, twice, inside the bundle. The released wheels
carried no libpython reference and worked only because the GitHub runner's
Python happens to be one pyo3 does not link against. With `ext`, the `.so`
from `maturin develop` links nothing, verified with `otool -L`.

## Measured, all local

macOS arm64, no CI run. Guest build 18 s / 3 crates; the bundle
`dist/kalast-dev-c3cfbe8-macos-arm64` at 291 MB unpacked and **113 MB**
compressed, against 149 MB for v0.5.2; inside it `kalast 0.5.2` from its own
site-packages, `2 up to date, 0 built, 0 failed`, the embedded interpreter
starting, and a hosted library loading with the two-rpath layout.

## And then the interpreter folder, which was 214 MB

Asked what a release bundle's `python/` was made of, and it split five ways:

| | MB | who needs it |
|---|---|---|
| libpython + stdlib | 36 | the UI app's embedded interpreter |
| numpy, spiceypy, kalast's Python | 25 | `import kalast` |
| scipy, matplotlib, PIL, fontTools | 104 | three shipped examples that plot, two that solve |
| `bin/python3`, kalast's `.so`, pip | 40 | only `python/bin/python3 -m kalast` |
| `include/`, `share/`, tcl/tk | 13 | nothing |

CPython was in there **twice** -- statically inside `bin/python3.14`, and
again as `lib/libpython3.14.dylib` for the executable -- and the UI app only
ever used the dylib. kalast's own `_rs.abi3.so` was never loaded either:
`append_to_inittab` hands the embedded interpreter the executable's
bindings, precisely so there are not two engines in one process.

The last two rows go. `python/` is 165 MB; the bundle 248 MB unpacked and
**97 MB** compressed, against 113 before and 149 at v0.5.2. What goes with
them is `python/bin/python3 -m kalast script.py` as a second way in, which
opened the same window; `./kalast script.py` is the way in. Plotting stays,
because three of the examples in the archive plot and shipping examples
that do not run is worse than 104 MB.

**The coupling.** Rebuilding a `.rs` from a bundle handed pyo3
`python/bin/python3` to introspect, and `bundled_python_dir()` used that
same binary as its marker -- so removing it would have cost the embedded
interpreter its `PYTHONHOME` as well. The marker is the standard library
now, and the workflow writes what pyo3 learned from the interpreter to
`python/pyo3-config.txt` while it still has one to ask; `configure_guest_link`
reads that through `PYO3_CONFIG_FILE`, rewriting the two build-machine paths
in it to the user's. Proven from the pruned bundle: a `.rs` rebuilt against
the source tree with no interpreter binary present, and loaded.

Against crates.io it cannot be proven until 0.5.3 is published: the
bundle's wrapper now asks for `kalast = "=<version>"` with feature `embed`,
and the 0.5.2 on the registry predates the split -- *"available features:
default, python, use_f64"*. A resolution error, not a linking one, and it
answers itself with the next tag.
