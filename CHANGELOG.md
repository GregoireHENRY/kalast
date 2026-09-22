# Changelog

Each version's section is the text of its GitHub release, verbatim and
nothing else, and it is written for the people who *use* kalast: what
changed for them. Release engineering, CI and internal refactors do not
belong here. The version gate refuses a tag whose section is missing or has
no entries, and the section is approved by a person before the tag is
pushed.

## v0.5.4 — unreleased

- Rust examples compile from the editor's buffer: edit a `.rs` in the UI app and hit Play or Compile, no Save needed — the same as a `.py` has always worked.

## v0.5.3

- Opening a `.obj` from the command line frames it: camera and Sun are placed from the mesh, with Blender axes and wireframe on. Also `app.simulation.frame_all()`.
- `import kalast` no longer loads matplotlib, scipy and pyarrow; `kalast.plot` and `kalast.tpm` import on first use. pyarrow is no longer in the bundle.
- The bundle's interpreter is pruned to what the UI app uses; archives are 90–134 MB, from 149–217.
- A `.rs` rebuilds from a bundle whose path contains a space.
- Cargo features: `python` is the bindings alone; `embed` (the default) adds an interpreter for a binary; `ext` builds an extension module. `cargo add kalast --no-default-features --features python` now gets neither link policy.
- `pyproject.toml` is no longer shipped in the bundle.
- "the editor" is the kalast UI app in docs and release notes.

## v0.5.2

- Handing the UI app a `.py` runs it in the window already open; it no longer closes and relaunches through `python -m kalast`. The executable carries an interpreter.
- Windows: the executable ships with the DLLs it needs beside it.

## v0.5.1

- The release bundle carries its own Python with kalast installed: a `.py` example runs with nothing installed.
- Rust examples ship precompiled, and can be rebuilt from a bundle — against crates.io, with a toolchain fetched into the bundle folder if the machine has none.
- Clear errors when a `.py` or `.rs` cannot run from a bundle; `shaders/` is no longer shipped, being compiled in.
