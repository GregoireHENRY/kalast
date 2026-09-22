# Changelog

Each version's section is the text of its GitHub release, verbatim and
nothing else. Two rules hold it together: the version gate refuses a tag
whose section is missing or has no entries, and the section is shown to a
person and approved before the tag is pushed.

## v0.5.4 — unreleased

- Rust examples compile from the editor's buffer: edit a `.rs` in the UI app and hit Play or Compile, no Save needed — the same as a `.py` has always worked.
- A tag whose commit was rehearsed publishes the rehearsal's artefacts instead of rebuilding them.

## v0.5.3

- Opening a `.obj` from the command line frames it: camera and Sun are placed from the mesh, with Blender axes and wireframe on. Also `app.simulation.frame_all()`.
- `import kalast` no longer loads matplotlib, scipy and pyarrow; `kalast.plot` and `kalast.tpm` import on first use. pyarrow is no longer in the bundle.
- The bundle's interpreter is pruned to what the UI app uses; archives are 90–134 MB, from 149–217.
- A `.rs` rebuilds from a bundle whose path contains a space.
- The wheel no longer links libpython (features split into `python` / `embed` / `ext`); `pip install kalast` is unaffected.
- `pyproject.toml` is no longer shipped in the bundle.
- "the editor" is the kalast UI app in docs and release notes.

## v0.5.2

- Handing the UI app a `.py` runs it in the window already open; it no longer closes and relaunches through `python -m kalast`. The executable carries an interpreter.
- Windows: the executable ships with the DLLs it needs beside it.

## v0.5.1

- The release bundle carries its own Python with kalast installed: a `.py` example runs with nothing installed.
- Rust examples ship precompiled, and can be rebuilt from a bundle — against crates.io, with a toolchain fetched into the bundle folder if the machine has none.
- Clear errors when a `.py` or `.rs` cannot run from a bundle; `shaders/` is no longer shipped, being compiled in.
- Release pipeline fixes: Linux wheel built in a manylinux container, Windows wheel via pyo3's generated import library, `kalast_macros` publishable to crates.io.
